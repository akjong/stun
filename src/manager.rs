use std::{collections::HashMap, sync::Arc, time::Duration};

use tokio::{
    process::Child,
    sync::{RwLock, mpsc},
    time::{interval, sleep},
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
}

impl TunnelManager {
    /// Create a new tunnel manager with the given configuration
    pub fn new(config: Config) -> StunResult<Self> {
        config.validate()?;

        let timeout = config.timeout.unwrap_or(2);
        let ssh_client = SshClient::new(config.clone());
        let health_checker = HealthChecker::new(timeout);

        Ok(Self {
            config,
            ssh_client,
            health_checker,
            tunnels: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            health_check_interval: Duration::from_secs(5), // Health check every 5 seconds
            max_failures: 3, // Max consecutive failures before restart
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

        let management_task = tokio::spawn(async move {
            Self::management_loop(
                tunnels,
                ssh_client,
                health_checker,
                health_check_interval,
                max_failures,
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
        let mut tunnels = self.tunnels.write().await;

        for (key, tunnel_info) in tunnels.iter_mut() {
            if tunnel_info.process.is_none() {
                match self.ssh_client.start_forwarding(&tunnel_info.spec).await {
                    Ok(process) => {
                        info!("Started tunnel: {}", key);
                        tunnel_info.process = Some(process);
                        tunnel_info.health = TunnelHealth::Unknown;
                        tunnel_info.failure_count = 0;
                    }
                    Err(e) => {
                        error!("Failed to start tunnel {}: {}", key, e);
                        tunnel_info.health = TunnelHealth::Down;
                    }
                }
            }
        }

        Ok(())
    }

    /// Stop all tunnels
    async fn stop_all_tunnels(&self) -> StunResult<()> {
        let mut tunnels = self.tunnels.write().await;

        for (key, tunnel_info) in tunnels.iter_mut() {
            if let Some(process) = tunnel_info.process.take() {
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
        mut shutdown_rx: mpsc::Receiver<()>,
    ) {
        let mut interval = interval(health_check_interval);
        interval.tick().await; // Skip first tick

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    Self::perform_health_checks(&tunnels, &ssh_client, &health_checker, max_failures).await;
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
    ) {
        let mut tunnels = tunnels.write().await;

        for (key, tunnel_info) in tunnels.iter_mut() {
            // Check if process is still running
            let process_alive = if let Some(ref mut process) = tunnel_info.process {
                health_checker.check_ssh_process(process).await
            } else {
                false
            };

            // Check if port forwarding is working
            let forwarding_healthy = if process_alive {
                // Give some time for port forwarding to become available
                sleep(Duration::from_millis(500)).await;
                health_checker.check_forwarding(&tunnel_info.spec).await
            } else {
                false
            };

            // Update health status
            let is_healthy = process_alive && forwarding_healthy;

            if is_healthy {
                if !tunnel_info.health.is_healthy() {
                    info!("Tunnel {} is now healthy", key);
                }
                tunnel_info.health = TunnelHealth::Healthy;
                tunnel_info.failure_count = 0;
            } else {
                tunnel_info.failure_count += 1;

                if tunnel_info.failure_count >= max_failures {
                    warn!(
                        "Tunnel {} failed {} times, restarting",
                        key, tunnel_info.failure_count
                    );
                    tunnel_info.health = TunnelHealth::Down;

                    // Kill the existing process if it exists
                    if let Some(process) = tunnel_info.process.take() {
                        if let Err(e) = SshClient::kill_process(process).await {
                            error!("Error killing failed tunnel process: {}", e);
                        }
                    }

                    // Try to restart the tunnel
                    match ssh_client.start_forwarding(&tunnel_info.spec).await {
                        Ok(process) => {
                            info!("Restarted tunnel: {}", key);
                            tunnel_info.process = Some(process);
                            tunnel_info.health = TunnelHealth::Unknown;
                            tunnel_info.failure_count = 0;
                        }
                        Err(e) => {
                            error!("Failed to restart tunnel {}: {}", key, e);
                        }
                    }
                } else {
                    debug!(
                        "Tunnel {} health check failed ({}/{})",
                        key, tunnel_info.failure_count, max_failures
                    );
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
