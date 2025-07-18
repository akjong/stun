use std::path::PathBuf;

use clap::{Arg, Command};
use stun::{Config, TunnelManager};
use tokio::signal;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    stun::init_logging()?;

    let matches = Command::new("stun")
        .version("0.1.0")
        .author("akagi201")
        .about("SSH port forwarding and tunneling tool")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file path")
                .required(true),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(clap::ArgAction::Count)
                .help("Increase logging verbosity"),
        )
        .get_matches();

    let config_path = matches
        .get_one::<String>("config")
        .expect("config argument is required");
    let config_path = PathBuf::from(config_path);

    // Load configuration
    let config = Config::from_file(&config_path)?;

    info!("Loaded configuration from {}", config_path.display());
    info!("Mode: {:?}", config.mode);
    info!(
        "Remote: {}@{}:{}",
        config.remote.user, config.remote.host, config.remote.port
    );
    info!("Forwarding {} tunnels", config.forwarding_list.len());

    // Create and start tunnel manager
    let mut manager = TunnelManager::new(config)?;

    // Set up graceful shutdown
    let manager_task = tokio::spawn(async move {
        if let Err(e) = manager.start().await {
            error!("Tunnel manager error: {}", e);
        }
    });

    info!("Starting tunnel manager. Press Ctrl+C to stop.");

    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C, shutting down...");
        }
        result = manager_task => {
            if let Err(e) = result {
                error!("Manager task panicked: {}", e);
            }
        }
    }

    info!("Shutdown complete");
    Ok(())
}
