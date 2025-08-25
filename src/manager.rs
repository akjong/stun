use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::{
    process::Child,
    sync::{RwLock, mpsc},
    time::{Instant, interval, sleep},
};
use tracing::{debug, error, info, warn};

use crate::{
    config::Config,
    error::StunResult,
    forwarding::ForwardingSpec,
    health::{HealthChecker, TunnelHealth},
    ssh::SshClient,
};

/// A managed tunnel with its associated process and health status
#[derive(Debug)]
struct TunnelInfo {
    /// The SSH process for this tunnel
    process: Option<Child>,
    /// Current health status
    health: TunnelHealth,
    /// Forwarding specification
    spec: ForwardingSpec,
    /// Number of consecutive health check failures
    failure_count: u32,
    /// Next allowed restart time (with backoff). None means restart allowed immediately
    next_restart_at: Option<Instant>,
    /// Current backoff duration in seconds
    backoff_secs: u64,
}

/// Main tunnel manager that handles multiple SSH port forwarding connections
pub struct TunnelManager {
    config: Config,
    ssh_client: SshClient,
    health_checker: HealthChecker,
    tunnels: Arc<RwLock<HashMap<String, TunnelInfo>>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
    health_check_interval: Duration,
    max_failures: u32,
    backoff_base_secs: u64,
    backoff_max_secs: u64,
}

impl TunnelManager {
    /// Create a new tunnel manager with the given configuration
    pub fn new(config: Config) -> StunResult<Self> {
        config.validate()?;

        let timeout = config.timeout.unwrap_or(2);
        let backoff_base = config.backoff_base_secs.unwrap_or(1);
        let backoff_max = config.backoff_max_secs.unwrap_or(30);
        let ssh_client = SshClient::new(config.clone());
        let health_checker = HealthChecker::new(timeout);

        Ok(Self {
            config,
            ssh_client,
            health_checker,
            tunnels: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            health_check_interval: Duration::from_secs(5), // Health check every 5 seconds
            max_failures: 3, // Max consecutive failures before scheduling restart
            backoff_base_secs: backoff_base,
            backoff_max_secs: backoff_max,
        })
    }

    /// Start the tunnel manager
    pub async fn start(&mut self) -> StunResult<()> {
        info!("Starting tunnel manager");

        // Parse forwarding specifications
        let mut specs = Vec::new();
        for spec_str in &self.config.forwarding_list {
            let spec = ForwardingSpec::parse(spec_str)?;
            specs.push(spec);
        }

        // Initialize tunnels
        {
            let mut tunnels = self.tunnels.write().await;
            for spec in specs {
                let key = spec.to_ssh_arg();
                tunnels.insert(
                    key,
                    TunnelInfo {
                        process: None,
                        health: TunnelHealth::Unknown,
                        spec,
                        failure_count: 0,
                        next_restart_at: None,
                        backoff_secs: self.backoff_base_secs,
                    },
                );
            }
        }

        // Start all tunnels initially
        self.start_all_tunnels().await?;

        // Start health checking and management loop
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let tunnels = Arc::clone(&self.tunnels);
        let ssh_client = SshClient::new(self.config.clone());
        let health_checker = self.health_checker.clone();
        let health_check_interval = self.health_check_interval;
        let max_failures = self.max_failures;
        let backoff_max_secs = self.backoff_max_secs;

        let management_task = tokio::spawn(async move {
            Self::management_loop(
                tunnels,
                ssh_client,
                health_checker,
                health_check_interval,
                max_failures,
                backoff_max_secs,
                shutdown_rx,
            )
            .await;
        });

        info!("Tunnel manager started successfully");

        // For CLI usage, wait for the management task
        if let Err(e) = management_task.await {
            error!("Management task failed: {}", e);
        }

        Ok(())
    }

    /// Start the tunnel manager but return immediately with the management task handle.
    /// Use stop() to trigger shutdown and then await the returned handle to finish.
    pub async fn start_background(&mut self) -> StunResult<tokio::task::JoinHandle<()>> {
        info!("Starting tunnel manager (background)");

        // Parse forwarding specifications
        let mut specs = Vec::new();
        for spec_str in &self.config.forwarding_list {
            let spec = ForwardingSpec::parse(spec_str)?;
            specs.push(spec);
        }

        // Initialize tunnels
        {
            let mut tunnels = self.tunnels.write().await;
            for spec in specs {
                let key = spec.to_ssh_arg();
                tunnels.insert(
                    key,
                    TunnelInfo {
                        process: None,
                        health: TunnelHealth::Unknown,
                        spec,
                        failure_count: 0,
                        next_restart_at: None,
                        backoff_secs: self.backoff_base_secs,
                    },
                );
            }
        }

        // Start all tunnels initially
        self.start_all_tunnels().await?;

        // Start health checking and management loop
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);

        let tunnels = Arc::clone(&self.tunnels);
        let ssh_client = SshClient::new(self.config.clone());
        let health_checker = self.health_checker.clone();
        let health_check_interval = self.health_check_interval;
        let max_failures = self.max_failures;
        let backoff_max_secs = self.backoff_max_secs;

        let management_task = tokio::spawn(async move {
            Self::management_loop(
                tunnels,
                ssh_client,
                health_checker,
                health_check_interval,
                max_failures,
                backoff_max_secs,
                shutdown_rx,
            )
            .await;
        });

        info!("Tunnel manager started successfully (background)");
        Ok(management_task)
    }

    /// Stop the tunnel manager and all tunnels
    pub async fn stop(&mut self) -> StunResult<()> {
        info!("Stopping tunnel manager");

        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        // Stop all tunnels
        self.stop_all_tunnels().await?;

        info!("Tunnel manager stopped");
        Ok(())
    }

    /// Start all configured tunnels
    async fn start_all_tunnels(&self) -> StunResult<()> {
        // Snapshot which tunnels need to be started without holding the lock across awaits
        let to_start: Vec<(String, ForwardingSpec)> = {
            let tunnels = self.tunnels.read().await;
            tunnels
                .iter()
                .filter_map(|(key, info)| {
                    if info.process.is_none() {
                        Some((key.clone(), info.spec.clone()))
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Start them without holding the lock, then apply results
        let mut results: Vec<(String, Result<Child, crate::error::StunError>)> = Vec::new();
        for (key, spec) in to_start {
            let res = self.ssh_client.start_forwarding(&spec).await;
            results.push((key, res));
        }

        // Apply results under a short write lock
        let mut tunnels = self.tunnels.write().await;
        for (key, res) in results {
            if let Some(info) = tunnels.get_mut(&key) {
                if info.process.is_some() {
                    continue; // already started elsewhere
                }
                match res {
                    Ok(process) => {
                        info!("Started tunnel: {}", key);
                        info.process = Some(process);
                        info.health = TunnelHealth::Unknown;
                        info.failure_count = 0;
                    }
                    Err(e) => {
                        error!("Failed to start tunnel {}: {}", key, e);
                        info.health = TunnelHealth::Down;
                    }
                }
            }
        }

        Ok(())
    }

    /// Stop all tunnels
    async fn stop_all_tunnels(&self) -> StunResult<()> {
        // Take out all processes under a short lock
        let to_stop: Vec<(String, Option<Child>)> = {
            let mut tunnels = self.tunnels.write().await;
            tunnels
                .iter_mut()
                .map(|(key, info)| (key.clone(), info.process.take()))
                .collect()
        };

        // Stop outside of the lock
        for (key, process_opt) in to_stop {
            if let Some(process) = process_opt {
                info!("Stopping tunnel: {}", key);
                if let Err(e) = SshClient::kill_process(process).await {
                    warn!("Error stopping tunnel {}: {}", key, e);
                }
            }
        }

        Ok(())
    }

    /// Main management loop that runs health checks and restarts failed tunnels
    async fn management_loop(
        tunnels: Arc<RwLock<HashMap<String, TunnelInfo>>>,
        ssh_client: SshClient,
        health_checker: HealthChecker,
        health_check_interval: Duration,
        max_failures: u32,
        backoff_max_secs: u64,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        let mut interval = interval(health_check_interval);
        interval.tick().await; // Skip first tick

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Local mode allows local TCP probing; remote mode should not attempt local TCP checks
                    let is_local_mode = ssh_client.is_local_mode();
                    Self::perform_health_checks(&tunnels, &ssh_client, &health_checker, max_failures, backoff_max_secs, is_local_mode).await;
                }
                _ = shutdown_rx.recv() => {
                    debug!("Received shutdown signal in management loop");
                    break;
                }
            }
        }
    }

    /// Perform health checks on all tunnels and restart failed ones
    async fn perform_health_checks(
        tunnels: &Arc<RwLock<HashMap<String, TunnelInfo>>>,
        ssh_client: &SshClient,
        health_checker: &HealthChecker,
        max_failures: u32,
        backoff_max_secs: u64,
        is_local_mode: bool,
    ) {
        // Snapshot keys so we can process each tunnel without holding the lock
        let keys: Vec<String> = {
            let map = tunnels.read().await;
            map.keys().cloned().collect()
        };

        for key in keys {
            // Take process and clone spec under a short lock
            let (
                mut process_opt,
                spec,
                mut failure_count,
                prev_health,
                mut next_restart_at,
                mut backoff_secs,
            ) = {
                let mut map = tunnels.write().await;
                if let Some(info) = map.get_mut(&key) {
                    (
                        info.process.take(),
                        info.spec.clone(),
                        info.failure_count,
                        info.health.clone(),
                        info.next_restart_at,
                        info.backoff_secs,
                    )
                } else {
                    continue;
                }
            };

            // Check liveness without holding the lock
            let process_alive = if let Some(ref mut process) = process_opt {
                health_checker.check_ssh_process(process).await
            } else {
                false
            };

            // Only perform local TCP probe for local mode
            let forwarding_healthy = if process_alive && is_local_mode {
                // Give some time for port forwarding to become available
                sleep(Duration::from_millis(500)).await;
                health_checker.check_forwarding(&spec).await
            } else {
                // For remote mode, optionally run a remote TCP probe if configured
                if process_alive && !is_local_mode {
                    // Look up probe target by the exact spec string key
                    let probe_target = ssh_client.remote_probe_target(&spec);
                    if let Some((host, port)) = probe_target {
                        match ssh_client.remote_tcp_probe(&host, port).await {
                            Ok(true) => true,
                            Ok(false) => false,
                            Err(e) => {
                                warn!("Remote probe failed: {}", e);
                                false
                            }
                        }
                    } else {
                        // No remote probe configured; rely on process liveness only
                        true
                    }
                } else {
                    false
                }
            };

            let is_healthy = if is_local_mode {
                process_alive && forwarding_healthy
            } else {
                process_alive
            };

            // Apply updates and possible restarts with exponential backoff
            if is_healthy {
                let mut map = tunnels.write().await;
                if let Some(info) = map.get_mut(&key) {
                    if !prev_health.is_healthy() {
                        info!("Tunnel {} is now healthy", key);
                    }
                    // Put process back
                    info.process = process_opt;
                    info.health = TunnelHealth::Healthy;
                    info.failure_count = 0;
                    info.next_restart_at = None;
                    info.backoff_secs = 1;
                }
            } else {
                failure_count += 1;
                let now = Instant::now();
                if failure_count >= max_failures {
                    // Schedule or attempt restart based on backoff
                    if let Some(at) = next_restart_at {
                        if now < at {
                            // Not yet time to restart; update state and continue
                            let mut map = tunnels.write().await;
                            if let Some(info) = map.get_mut(&key) {
                                debug!(
                                    "Tunnel {} waiting for backoff {:?}",
                                    key,
                                    at.saturating_duration_since(now)
                                );
                                info.process = process_opt;
                                info.health = TunnelHealth::Down;
                                info.failure_count = failure_count;
                                info.next_restart_at = Some(at);
                                info.backoff_secs = backoff_secs;
                            }
                            continue;
                        }
                        // time to restart now
                    } else {
                        // First time exceeding threshold: compute next_restart_at and kill process once
                        if let Some(proc_to_kill) = process_opt.take()
                            && let Err(e) = SshClient::kill_process(proc_to_kill).await
                        {
                            error!("Error killing failed tunnel process: {}", e);
                        }
                        // compute jittered backoff
                        backoff_secs = backoff_secs.max(1);
                        let jittered = jitter_secs(backoff_secs, &spec);
                        next_restart_at = Some(now + Duration::from_secs(jittered));

                        let mut map = tunnels.write().await;
                        if let Some(info) = map.get_mut(&key) {
                            warn!(
                                "Tunnel {} failed {} times, scheduling restart in {}s",
                                key, failure_count, jittered
                            );
                            info.process = None;
                            info.health = TunnelHealth::Down;
                            info.failure_count = failure_count;
                            info.next_restart_at = next_restart_at;
                            info.backoff_secs = backoff_secs;
                        }
                        continue;
                    }

                    // Try to restart now
                    match ssh_client.start_forwarding(&spec).await {
                        Ok(new_proc) => {
                            let mut map = tunnels.write().await;
                            if let Some(info) = map.get_mut(&key) {
                                info!("Restarted tunnel: {}", key);
                                info.process = Some(new_proc);
                                info.health = TunnelHealth::Unknown;
                                info.failure_count = 0;
                                info.next_restart_at = None;
                                info.backoff_secs = 1;
                            }
                        }
                        Err(e) => {
                            // Increase backoff and schedule again
                            backoff_secs = (backoff_secs.saturating_mul(2)).min(backoff_max_secs);
                            let delay = jitter_secs(backoff_secs, &spec);
                            let when = now + Duration::from_secs(delay);

                            let mut map = tunnels.write().await;
                            if let Some(info) = map.get_mut(&key) {
                                error!("Failed to restart tunnel {}: {}", key, e);
                                info.process = None;
                                info.health = TunnelHealth::Down;
                                info.failure_count = failure_count;
                                info.next_restart_at = Some(when);
                                info.backoff_secs = backoff_secs;
                            }
                        }
                    }
                } else {
                    let mut map = tunnels.write().await;
                    if let Some(info) = map.get_mut(&key) {
                        debug!(
                            "Tunnel {} health check failed ({}/{})",
                            key, failure_count, max_failures
                        );
                        // Put process back and update counters
                        info.process = process_opt;
                        info.health = TunnelHealth::Down;
                        info.failure_count = failure_count;
                        // retain any existing backoff scheduling
                        info.next_restart_at = next_restart_at;
                        info.backoff_secs = backoff_secs;
                    }
                }
            }
        }
    }

    /// Get the status of all tunnels
    pub async fn get_status(&self) -> HashMap<String, TunnelHealth> {
        let tunnels = self.tunnels.read().await;
        tunnels
            .iter()
            .map(|(key, info)| (key.clone(), info.health.clone()))
            .collect()
    }
}

/// Compute a deterministic jittered delay in seconds for backoff (80%-120%)
fn jitter_secs(base_secs: u64, spec: &ForwardingSpec) -> u64 {
    let seed = (spec.bind_port as u32) ^ (spec.remote_port as u32);
    let jitter_pct = 80 + (seed % 41); // 80..120
    base_secs.saturating_mul(jitter_pct as u64).div_ceil(100)
}

impl Drop for TunnelManager {
    fn drop(&mut self) {
        // Try to send shutdown signal, but don't wait
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.try_send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ForwardingMode, RemoteConfig};

    fn create_test_config() -> Config {
        Config {
            mode: ForwardingMode::Local,
            remote: RemoteConfig {
                host: "127.0.0.1".to_string(),
                port: 22,
                user: "testuser".to_string(),
                key: None,
            },
            forwarding_list: vec![
                "18080:127.0.0.1:8080".to_string(),
                "19000:127.0.0.1:9000".to_string(),
            ],
            timeout: Some(1),
            remote_probes: None,
            backoff_base_secs: None,
            backoff_max_secs: None,
        }
    }

    #[tokio::test]
    async fn test_tunnel_manager_creation() {
        let config = create_test_config();
        let manager = TunnelManager::new(config).unwrap();

        assert_eq!(manager.config.forwarding_list.len(), 2);
    }

    #[tokio::test]
    async fn test_invalid_config() {
        let mut config = create_test_config();
        config.forwarding_list = vec!["invalid".to_string()];

        let result = TunnelManager::new(config);
        assert!(result.is_err());
    }
}
