use thiserror::Error;

/// Result type alias for stun operations
pub type StunResult<T> = Result<T, StunError>;

/// Error types for the stun library
#[derive(Error, Debug)]
pub enum StunError {
    /// Configuration related errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// SSH connection errors
    #[error("SSH error: {0}")]
    Ssh(String),

    /// Network connection errors
    #[error("Network error: {0}")]
    Network(String),

    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Tunnel management errors
    #[error("Tunnel error: {0}")]
    Tunnel(String),

    /// Health check errors
    #[error("Health check error: {0}")]
    HealthCheck(String),

    /// Timeout errors
    #[error("Operation timed out")]
    Timeout,

    /// Generic errors
    #[error("Error: {0}")]
    Other(String),
}

impl From<russh::Error> for StunError {
    fn from(err: russh::Error) -> Self {
        StunError::Ssh(err.to_string())
    }
}

impl From<eyre::Error> for StunError {
    fn from(err: eyre::Error) -> Self {
        StunError::Other(err.to_string())
    }
}
