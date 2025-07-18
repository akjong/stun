use stun::{Config, TunnelManager};

/// Example: Load configuration from file and start tunneling
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    stun::init_logging()?;

    // Load configuration from file
    let config = Config::from_file("config.json")?;

    println!("Loaded configuration:");
    println!("  Mode: {:?}", config.mode);
    println!(
        "  Remote: {}@{}:{}",
        config.remote.user, config.remote.host, config.remote.port
    );
    println!("  Forwarding {} tunnels", config.forwarding_list.len());

    // Create tunnel manager
    let mut manager = TunnelManager::new(config)?;

    println!("Starting tunnels... (Press Ctrl+C to stop)");

    // This will run until the program is terminated or an error occurs
    manager.start().await?;

    Ok(())
}
