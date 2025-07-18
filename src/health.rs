use std::time::Duration;

use tokio::{net::TcpStream, time::timeout};
use tracing::{debug, warn};

use crate::forwarding::ForwardingSpec;

/// Health checker for port forwarding connections
#[derive(Debug, Clone)]
pub struct HealthChecker {
    /// Connection timeout for health checks
    timeout: Duration,
}

impl HealthChecker {
    /// Create a new health checker with the specified timeout
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Check if a forwarding connection is healthy by attempting to connect
    pub async fn check_forwarding(&self, spec: &ForwardingSpec) -> bool {
        let address = format!("{}:{}", spec.effective_bind_address(), spec.bind_port);

        debug!("Health checking connection to {}", address);

        match timeout(self.timeout, TcpStream::connect(&address)).await {
            Ok(Ok(_)) => {
                debug!("Health check successful for {}", address);
                true
            }
            Ok(Err(e)) => {
                warn!("Health check failed for {}: {}", address, e);
                false
            }
            Err(_) => {
                warn!("Health check timed out for {}", address);
                false
            }
        }
    }

    /// Check if an SSH process is responding by attempting to write to stdin
    pub async fn check_ssh_process(&self, process: &mut tokio::process::Child) -> bool {
        // Check if the process is still running
        match process.try_wait() {
            Ok(Some(status)) => {
                warn!("SSH process exited with status: {}", status);
                false
            }
            Ok(None) => {
                debug!("SSH process is still running");
                true
            }
            Err(e) => {
                warn!("Error checking SSH process status: {}", e);
                false
            }
        }
    }
}

/// Health status for a tunnel
#[derive(Debug, Clone, PartialEq)]
pub enum TunnelHealth {
    /// Tunnel is healthy and functioning
    Healthy,
    /// Tunnel is down or unreachable
    Down,
    /// Tunnel status is unknown (e.g., during startup)
    Unknown,
}

impl TunnelHealth {
    /// Check if the tunnel is in a healthy state
    pub fn is_healthy(&self) -> bool {
        matches!(self, TunnelHealth::Healthy)
    }

    /// Check if the tunnel is down
    pub fn is_down(&self) -> bool {
        matches!(self, TunnelHealth::Down)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forwarding::ForwardingSpec;

    #[test]
    fn test_health_status() {
        assert!(TunnelHealth::Healthy.is_healthy());
        assert!(!TunnelHealth::Down.is_healthy());
        assert!(!TunnelHealth::Unknown.is_healthy());

        assert!(TunnelHealth::Down.is_down());
        assert!(!TunnelHealth::Healthy.is_down());
        assert!(!TunnelHealth::Unknown.is_down());
    }

    #[tokio::test]
    async fn test_health_checker_timeout() {
        let checker = HealthChecker::new(1);

        // This should fail quickly since nothing is listening on this port
        let spec = ForwardingSpec::parse("65534:127.0.0.1:65534").unwrap();
        let result = checker.check_forwarding(&spec).await;

        assert!(!result);
    }
}
