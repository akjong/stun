use std::{path::Path, process::Stdio};

use tokio::process::{Child, Command};
use tracing::{debug, error, info, warn};

use crate::{
    config::Config,
    error::{StunError, StunResult},
    forwarding::ForwardingSpec,
};

/// SSH client wrapper for port forwarding
pub struct SshClient {
    config: Config,
}

impl SshClient {
    /// Create a new SSH client with the given configuration
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Start an SSH process with port forwarding
    pub async fn start_forwarding(&self, spec: &ForwardingSpec) -> StunResult<Child> {
        let mut cmd = Command::new("ssh");

        // Base SSH options
        cmd.args([
            "-o",
            "ServerAliveInterval=30",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "ExitOnForwardFailure=yes",
        ]);

        // Add forwarding flag and specification
        cmd.arg(self.config.mode.to_ssh_flag());
        cmd.arg(spec.to_ssh_arg());

        // Add private key if specified
        if let Some(key_path) = &self.config.remote.key {
            if Path::new(key_path).exists() {
                cmd.args(["-i", key_path]);
            } else {
                warn!("Private key file does not exist: {}", key_path);
            }
        }

        // Add port if not default
        if self.config.remote.port != 22 {
            cmd.args(["-p", &self.config.remote.port.to_string()]);
        }

        // SSH connection target
        let target = format!("{}@{}", self.config.remote.user, self.config.remote.host);
        cmd.arg(target);

        // Configure stdio
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Starting SSH command: {:?}", cmd);

        let child = cmd
            .spawn()
            .map_err(|e| StunError::Ssh(format!("Failed to start SSH process: {e}")))?;

        info!("Started SSH forwarding: {}", spec.to_ssh_arg());

        Ok(child)
    }

    /// Kill an SSH process gracefully
    pub async fn kill_process(mut process: Child) -> StunResult<()> {
        debug!("Terminating SSH process");

        // Try graceful termination first
        if let Err(e) = process.kill().await {
            warn!("Error killing SSH process: {}", e);
        }

        // Wait for the process to exit
        match process.wait().await {
            Ok(status) => {
                debug!("SSH process exited with status: {}", status);
            }
            Err(e) => {
                error!("Error waiting for SSH process to exit: {}", e);
            }
        }

        Ok(())
    }

    /// Build SSH command string for debugging/logging
    pub fn build_command_string(&self, spec: &ForwardingSpec) -> String {
        let mut parts = vec!["ssh".to_string()];

        parts.extend([
            "-o".to_string(),
            "ServerAliveInterval=30".to_string(),
            "-o".to_string(),
            "StrictHostKeyChecking=no".to_string(),
            "-o".to_string(),
            "ExitOnForwardFailure=yes".to_string(),
        ]);

        parts.push(self.config.mode.to_ssh_flag().to_string());
        parts.push(spec.to_ssh_arg());

        if let Some(key_path) = &self.config.remote.key {
            parts.push("-i".to_string());
            parts.push(key_path.clone());
        }

        if self.config.remote.port != 22 {
            parts.push("-p".to_string());
            parts.push(self.config.remote.port.to_string());
        }

        let target = format!("{}@{}", self.config.remote.user, self.config.remote.host);
        parts.push(target);

        parts.join(" ")
    }

    /// Returns true if the client is configured for local (-L) forwarding
    pub fn is_local_mode(&self) -> bool {
        matches!(self.config.mode, crate::config::ForwardingMode::Local)
    }

    /// Attempt a remote TCP connection to host:port via the SSH server.
    /// This runs a small shell test remotely. Returns true on success.
    pub async fn remote_tcp_probe(&self, host: &str, port: u16) -> StunResult<bool> {
        // Build: ssh [opts] user@host sh -lc 'nc -z -w <timeout> <host> <port> || /dev/tcp'
        // We try netcat first; if unavailable, try bash /dev/tcp if available.
        let timeout_secs = self.config.timeout.unwrap_or(2);

        let mut cmd = Command::new("ssh");
        // base options similar to start_forwarding
        cmd.args([
            "-o",
            "ServerAliveInterval=30",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "ExitOnForwardFailure=yes",
        ]);
        if let Some(key_path) = &self.config.remote.key
            && Path::new(key_path).exists()
        {
            cmd.args(["-i", key_path]);
        }
        if self.config.remote.port != 22 {
            cmd.args(["-p", &self.config.remote.port.to_string()]);
        }
        let target = format!("{}@{}", self.config.remote.user, self.config.remote.host);
        cmd.arg(target);

        // Remote shell script: try nc, else bash tcp
        let script = format!(
            "sh -lc 'nc -z -w {timeout_secs} {host} {port} >/dev/null 2>&1 || (bash -lc \"echo > /dev/tcp/{host}/{port}\")'"
        );
        cmd.arg(script);

        let status = cmd
            .status()
            .await
            .map_err(|e| StunError::Ssh(format!("Failed to run remote probe: {e}")))?;

        Ok(status.success())
    }

    /// Lookup a configured remote probe target for the given spec (by its to_ssh_arg() string)
    pub fn remote_probe_target(&self, spec: &ForwardingSpec) -> Option<(String, u16)> {
        let key = spec.to_ssh_arg();
        if let Some(map) = &self.config.remote_probes
            && let Some(target) = map.get(&key)
        {
            // Split last ':'
            let (host, port_str) = target.rsplit_once(':')?;
            if let Ok(port) = port_str.parse::<u16>() {
                return Some((host.to_string(), port));
            }
        }
        None
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
                host: "example.com".to_string(),
                port: 22,
                user: "testuser".to_string(),
                key: Some("/path/to/key".to_string()),
            },
            forwarding_list: vec![],
            timeout: Some(5),
            remote_probes: None,
            backoff_base_secs: None,
            backoff_max_secs: None,
        }
    }

    #[test]
    fn test_build_command_string() {
        let config = create_test_config();
        let client = SshClient::new(config);
        let spec = ForwardingSpec::parse("8080:127.0.0.1:9000").unwrap();

        let cmd = client.build_command_string(&spec);

        assert!(cmd.contains("ssh"));
        assert!(cmd.contains("-L"));
        assert!(cmd.contains("8080:127.0.0.1:9000"));
        assert!(cmd.contains("-i /path/to/key"));
        assert!(cmd.contains("testuser@example.com"));
    }

    #[test]
    fn test_build_command_string_with_port() {
        let mut config = create_test_config();
        config.remote.port = 2222;
        config.mode = ForwardingMode::Remote;

        let client = SshClient::new(config);
        let spec = ForwardingSpec::parse("0.0.0.0:8080:192.168.1.10:9000").unwrap();

        let cmd = client.build_command_string(&spec);

        assert!(cmd.contains("-R"));
        assert!(cmd.contains("-p 2222"));
        assert!(cmd.contains("0.0.0.0:8080:192.168.1.10:9000"));
    }
}
