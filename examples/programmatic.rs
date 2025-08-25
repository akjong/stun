use stun::{Config, ForwardingMode, RemoteConfig, TunnelManager};

/// Example: Create configuration programmatically and start tunneling
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    stun::init_logging()?;

    // Create configuration programmatically
    let config = Config {
        mode: ForwardingMode::Local,
        remote: RemoteConfig {
            host: "example.com".to_string(),
            port: 22,
            user: "username".to_string(),
            key: Some("~/.ssh/id_rsa".to_string()),
        },
        forwarding_list: vec![
            "8080:127.0.0.1:8080".to_string(),
            "3306:database.internal:3306".to_string(),
            "5432:postgres.internal:5432".to_string(),
        ],
        timeout: Some(5),
        remote_probes: None,
        backoff_base_secs: None,
        backoff_max_secs: None,
    };

    println!("Creating tunnel manager...");
    let mut manager = TunnelManager::new(config)?;

    println!("Starting tunnels in background...");
    let handle = manager.start_background().await?;

    println!("Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    println!("Stopping...");
    manager.stop().await?;
    let _ = handle.await;

    Ok(())
}
