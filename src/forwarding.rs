use crate::error::{StunError, StunResult};

/// Represents a port forwarding specification
#[derive(Debug, Clone, PartialEq)]
pub struct ForwardingSpec {
    /// Local/bind address (optional)
    pub bind_address: Option<String>,
    /// Local/bind port
    pub bind_port: u16,
    /// Remote host
    pub remote_host: String,
    /// Remote port
    pub remote_port: u16,
}

impl ForwardingSpec {
    /// Parse a forwarding specification string
    ///
    /// Supported formats:
    /// - "port:host:port" (e.g., "8080:127.0.0.1:8080")
    /// - "address:port:host:port" (e.g., "0.0.0.0:8080:127.0.0.1:8080")
    /// - IPv6 addresses must be enclosed in brackets, e.g., "[::1]:80:localhost:80"
    pub fn parse(spec: &str) -> StunResult<Self> {
        // Parse from right to left to handle IPv6 addresses correctly
        // Format is always: [bind_addr:]bind_port:remote_host:remote_port

        let last_colon = spec.rfind(':').ok_or_else(|| {
            StunError::Config(format!("Invalid forwarding specification: {spec}"))
        })?;

        let remote_port_str = &spec[last_colon + 1..];
        let remote_port = remote_port_str
            .parse::<u16>()
            .map_err(|_| StunError::Config(format!("Invalid remote port: {remote_port_str}")))?;

        let rest = &spec[..last_colon];
        // rest is now "[bind_addr:]bind_port:remote_host"

        let second_last_colon = rest.rfind(':').ok_or_else(|| {
            StunError::Config(format!("Invalid forwarding specification: {spec}"))
        })?;

        let remote_host = &rest[second_last_colon + 1..];
        let rest2 = &rest[..second_last_colon];
        // rest2 is now "[bind_addr:]bind_port"

        let (bind_addr, bind_port_str) = if let Some(idx) = rest2.rfind(':') {
            // Has bind address
            (Some(rest2[..idx].to_string()), &rest2[idx + 1..])
        } else {
            // No bind address
            (None, rest2)
        };

        let bind_port = bind_port_str
            .parse::<u16>()
            .map_err(|_| StunError::Config(format!("Invalid bind port: {bind_port_str}")))?;

        Ok(ForwardingSpec {
            bind_address: bind_addr,
            bind_port,
            remote_host: remote_host.to_string(),
            remote_port,
        })
    }

    /// Convert to SSH forwarding argument format
    pub fn to_ssh_arg(&self) -> String {
        match &self.bind_address {
            Some(addr) => format!(
                "{}:{}:{}:{}",
                addr, self.bind_port, self.remote_host, self.remote_port
            ),
            None => format!(
                "{}:{}:{}",
                self.bind_port, self.remote_host, self.remote_port
            ),
        }
    }

    /// Get the effective bind address (default to 127.0.0.1 if not specified)
    pub fn effective_bind_address(&self) -> &str {
        self.bind_address.as_deref().unwrap_or("127.0.0.1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_three_part_spec() {
        let spec = ForwardingSpec::parse("8080:127.0.0.1:9000").unwrap();
        assert_eq!(spec.bind_address, None);
        assert_eq!(spec.bind_port, 8080);
        assert_eq!(spec.remote_host, "127.0.0.1");
        assert_eq!(spec.remote_port, 9000);
    }

    #[test]
    fn test_parse_four_part_spec() {
        let spec = ForwardingSpec::parse("0.0.0.0:8080:192.168.1.10:9000").unwrap();
        assert_eq!(spec.bind_address, Some("0.0.0.0".to_string()));
        assert_eq!(spec.bind_port, 8080);
        assert_eq!(spec.remote_host, "192.168.1.10");
        assert_eq!(spec.remote_port, 9000);
    }

    #[test]
    fn test_to_ssh_arg() {
        let spec1 = ForwardingSpec::parse("8080:127.0.0.1:9000").unwrap();
        assert_eq!(spec1.to_ssh_arg(), "8080:127.0.0.1:9000");

        let spec2 = ForwardingSpec::parse("0.0.0.0:8080:192.168.1.10:9000").unwrap();
        assert_eq!(spec2.to_ssh_arg(), "0.0.0.0:8080:192.168.1.10:9000");
    }

    #[test]
    fn test_effective_bind_address() {
        let spec1 = ForwardingSpec::parse("8080:127.0.0.1:9000").unwrap();
        assert_eq!(spec1.effective_bind_address(), "127.0.0.1");

        let spec2 = ForwardingSpec::parse("0.0.0.0:8080:127.0.0.1:9000").unwrap();
        assert_eq!(spec2.effective_bind_address(), "0.0.0.0");
    }

    #[test]
    fn test_invalid_specs() {
        assert!(ForwardingSpec::parse("invalid").is_err());
        assert!(ForwardingSpec::parse("8080:host").is_err());
        assert!(ForwardingSpec::parse("8080:host:port:extra:part").is_err());
        assert!(ForwardingSpec::parse("invalid_port:host:9000").is_err());
    }
}
