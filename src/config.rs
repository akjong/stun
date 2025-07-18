use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{StunError, StunResult};

/// Configuration for the SSH tunneling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Forwarding mode: local or remote
    pub mode: ForwardingMode,
    /// Remote SSH server configuration
    pub remote: RemoteConfig,
    /// List of port forwarding specifications
    pub forwarding_list: Vec<String>,
    /// Connection timeout in seconds
    pub timeout: Option<u64>,
}

/// Forwarding mode enumeration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForwardingMode {
    Local,
    Remote,
}

impl ForwardingMode {
    /// Convert to SSH flag string
    pub fn to_ssh_flag(&self) -> &'static str {
        match self {
            ForwardingMode::Local => "-L",
            ForwardingMode::Remote => "-R",
        }
    }
}

/// Remote SSH server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    /// Hostname or IP address
    pub host: String,
    /// SSH port (default: 22)
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    /// Username for SSH connection
    pub user: String,
    /// Path to private key file (optional)
    pub key: Option<String>,
}

fn default_ssh_port() -> u16 {
    22
}

impl Config {
    /// Load configuration from a JSON file
    pub fn from_file<P: AsRef<Path>>(path: P) -> StunResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| StunError::Config(format!("Failed to read config file: {e}")))?;

        let config: Config = serde_json::from_str(&content)
            .map_err(|e| StunError::Config(format!("Failed to parse config: {e}")))?;

        config.validate()?;
        Ok(config)
    }

    /// Save configuration to a JSON file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> StunResult<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| StunError::Config(format!("Failed to serialize config: {e}")))?;

        std::fs::write(path, content)
            .map_err(|e| StunError::Config(format!("Failed to write config file: {e}")))?;

        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> StunResult<()> {
        if self.remote.host.is_empty() {
            return Err(StunError::Config("Remote host cannot be empty".to_string()));
        }

        if self.remote.user.is_empty() {
            return Err(StunError::Config("Remote user cannot be empty".to_string()));
        }

        if self.forwarding_list.is_empty() {
            return Err(StunError::Config(
                "Forwarding list cannot be empty".to_string(),
            ));
        }

        // Validate forwarding specifications
        for spec in &self.forwarding_list {
            self.validate_forwarding_spec(spec)?;
        }

        Ok(())
    }

    /// Validate a single forwarding specification
    fn validate_forwarding_spec(&self, spec: &str) -> StunResult<()> {
        let parts: Vec<&str> = spec.split(':').collect();

        if parts.len() != 3 && parts.len() != 4 {
            return Err(StunError::Config(format!(
                "Invalid forwarding specification '{spec}'. Expected format: [bind_addr:]port:host:port"
            )));
        }

        // Parse and validate ports
        let port_indices = if parts.len() == 3 {
            vec![0, 2]
        } else {
            vec![1, 3]
        };

        for &idx in &port_indices {
            parts[idx].parse::<u16>().map_err(|_| {
                StunError::Config(format!(
                    "Invalid port '{}' in forwarding specification '{}'",
                    parts[idx], spec
                ))
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let config = Config {
            mode: ForwardingMode::Local,
            remote: RemoteConfig {
                host: "example.com".to_string(),
                port: 22,
                user: "testuser".to_string(),
                key: None,
            },
            forwarding_list: vec!["8080:127.0.0.1:8080".to_string()],
            timeout: Some(5),
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_file_operations() {
        let config = Config {
            mode: ForwardingMode::Remote,
            remote: RemoteConfig {
                host: "192.168.1.100".to_string(),
                port: 2222,
                user: "admin".to_string(),
                key: Some("/path/to/key".to_string()),
            },
            forwarding_list: vec![
                "8080:127.0.0.1:8080".to_string(),
                "9000:localhost:9000".to_string(),
            ],
            timeout: Some(10),
        };

        // Create a temporary file for testing
        let mut temp_file = std::env::temp_dir();
        temp_file.push("test_config.json");

        // Test saving
        config.to_file(&temp_file).unwrap();

        // Test loading
        let loaded_config = Config::from_file(&temp_file).unwrap();

        // Clean up
        let _ = std::fs::remove_file(&temp_file);

        assert_eq!(config.remote.host, loaded_config.remote.host);
        assert_eq!(
            config.forwarding_list.len(),
            loaded_config.forwarding_list.len()
        );
    }
}
