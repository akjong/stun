//! STUN - SSH Tunneling Library
//!
//! A Rust library and CLI tool for SSH port forwarding and tunneling,
//! inspired by the Python script functionality but built with modern Rust patterns.
//!
//! # Features
//!
//! - Local and remote SSH port forwarding
//! - Connection health monitoring
//! - Automatic reconnection on failure
//! - JSON configuration support
//! - Structured logging with tracing
//!
//! # Example
//!
//! ```rust,no_run
//! use stun::{Config, ForwardingMode, TunnelManager};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Config {
//!         mode: ForwardingMode::Local,
//!         remote: stun::RemoteConfig {
//!             host: "192.168.1.100".to_string(),
//!             port: 22,
//!             user: "username".to_string(),
//!             key: None,
//!         },
//!         forwarding_list: vec![
//!             "8080:127.0.0.1:8080".to_string(),
//!             "9000:127.0.0.1:9000".to_string(),
//!         ],
//!         timeout: Some(2),
//!     };
//!
//!     let mut manager = TunnelManager::new(config)?;
//!     manager.start().await?;
//!
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod error;
pub mod forwarding;
pub mod health;
pub mod manager;
pub mod ssh;

pub use config::{Config, ForwardingMode, RemoteConfig};
pub use error::{StunError, StunResult};
pub use manager::TunnelManager;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize logging with tracing
pub fn init_logging() -> StunResult<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "stun=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .try_init()
        .map_err(|e| StunError::Config(e.to_string()))?;

    Ok(())
}
