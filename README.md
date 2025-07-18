# STUN - SSH Tunneling Library and CLI

A modern Rust implementation of SSH port forwarding and tunneling, inspired by Python automation scripts but built with Rust's performance and safety guarantees.

## Features

- **Local and Remote Port Forwarding**: Support for both `-L` (local) and `-R` (remote) SSH forwarding modes
- **Connection Health Monitoring**: Automatic detection of failed connections with configurable health checks
- **Automatic Reconnection**: Failed tunnels are automatically restarted with exponential backoff
- **JSON Configuration**: Easy-to-read configuration files with validation
- **Structured Logging**: Comprehensive logging with `tracing` for debugging and monitoring
- **Library and CLI**: Use as a Rust library in your projects or as a standalone CLI tool
- **Cross-platform**: Works on Linux, macOS, and Windows (where SSH is available)

## Installation

### From Source

```bash
git clone https://github.com/akjong/stun
cd stun
cargo build --release
```

The binary will be available at `target/release/stun`.

### As a Library

Add this to your `Cargo.toml`:

```toml
[dependencies]
stun = { git = "https://github.com/akjong/stun" }
```

## Quick Start

### CLI Usage

1. Create a configuration file:

```json
{
  "mode": "local",
  "remote": {
    "host": "192.168.1.100",
    "port": 22,
    "user": "username"
  },
  "forwarding_list": [
    "8080:127.0.0.1:8080",
    "9000:127.0.0.1:9000",
    "3306:127.0.0.1:3306"
  ],
  "timeout": 5
}
```

2. Run the CLI:

```bash
stun -c config.json
```

### Library Usage

```rust
use stun::{Config, TunnelManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from file
    let config = Config::from_file("config.json")?;
    
    // Create and start tunnel manager
    let mut manager = TunnelManager::new(config)?;
    manager.start().await?;
    
    Ok(())
}
```

## Configuration

### Configuration File Format

The configuration file is in JSON format with the following structure:

```json
{
  "mode": "local|remote",
  "remote": {
    "host": "hostname or IP",
    "port": 22,
    "user": "username",
    "key": "/path/to/private/key"
  },
  "forwarding_list": [
    "local_port:remote_host:remote_port",
    "bind_addr:local_port:remote_host:remote_port"
  ],
  "timeout": 5
}
```

### Configuration Options

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `mode` | string | Yes | - | Forwarding mode: `"local"` or `"remote"` |
| `remote.host` | string | Yes | - | SSH server hostname or IP address |
| `remote.port` | number | No | 22 | SSH server port |
| `remote.user` | string | Yes | - | SSH username |
| `remote.key` | string | No | - | Path to SSH private key file |
| `forwarding_list` | array | Yes | - | List of port forwarding specifications |
| `timeout` | number | No | 2 | Connection timeout in seconds |

### Port Forwarding Specifications

Two formats are supported:

1. **Three-part format**: `"local_port:remote_host:remote_port"`
   - Example: `"8080:127.0.0.1:8080"`
   - Binds to `127.0.0.1:8080` locally

2. **Four-part format**: `"bind_address:local_port:remote_host:remote_port"`
   - Example: `"0.0.0.0:8080:127.0.0.1:8080"`
   - Binds to `0.0.0.0:8080` locally

## CLI Options

```
stun [OPTIONS] --config <FILE>

OPTIONS:
    -c, --config <FILE>    Configuration file path
    -v, --verbose          Increase logging verbosity (can be used multiple times)
    -h, --help             Print help information
    -V, --version          Print version information
```

## Logging

The application uses structured logging with different levels:

- `ERROR`: Critical errors that prevent operation
- `WARN`: Warning conditions, such as connection failures
- `INFO`: General information about tunnel status
- `DEBUG`: Detailed debugging information

### Environment Variables

Control logging with the `RUST_LOG` environment variable:

```bash
# Show all logs
RUST_LOG=debug stun -c config.json

# Show only error and warning logs
RUST_LOG=warn stun -c config.json

# Show only stun library logs
RUST_LOG=stun=debug stun -c config.json
```

## How It Works

1. **Initialization**: The tunnel manager parses the configuration and validates all forwarding specifications
2. **SSH Process Management**: For each forwarding specification, an SSH process is spawned with appropriate flags
3. **Health Monitoring**: Every 5 seconds, the manager checks:
   - SSH process status
   - Port connectivity (attempts to connect to forwarded ports)
4. **Automatic Recovery**: If a tunnel fails health checks 3 times consecutively, it's automatically restarted
5. **Graceful Shutdown**: On SIGINT (Ctrl+C), all SSH processes are terminated gracefully

## Examples

### Local Port Forwarding

Forward local ports to remote services through SSH:

```json
{
  "mode": "local",
  "remote": {
    "host": "bastion.example.com",
    "user": "admin",
    "key": "~/.ssh/id_rsa"
  },
  "forwarding_list": [
    "3306:database.internal:3306",
    "5432:postgres.internal:5432",
    "6379:redis.internal:6379"
  ]
}
```

This creates local ports:
- `localhost:3306` → `database.internal:3306` (through bastion)
- `localhost:5432` → `postgres.internal:5432` (through bastion)
- `localhost:6379` → `redis.internal:6379` (through bastion)

### Remote Port Forwarding

Expose local services to remote networks:

```json
{
  "mode": "remote",
  "remote": {
    "host": "public-server.example.com",
    "user": "deploy"
  },
  "forwarding_list": [
    "8080:127.0.0.1:3000",
    "8443:127.0.0.1:8443"
  ]
}
```

This makes local services available on the remote server:
- `public-server.example.com:8080` → `localhost:3000`
- `public-server.example.com:8443` → `localhost:8443`

### Development Environment

Connect to development databases and services:

```json
{
  "mode": "local",
  "remote": {
    "host": "dev.example.com",
    "port": 2222,
    "user": "developer",
    "key": "~/.ssh/dev_key"
  },
  "forwarding_list": [
    "5432:localhost:5432",
    "3306:localhost:3306",
    "6379:localhost:6379",
    "9200:elasticsearch:9200",
    "5672:rabbitmq:5672"
  ],
  "timeout": 10
}
```

## Library API

### Core Types

```rust
use stun::{Config, TunnelManager, ForwardingMode, RemoteConfig};

// Configuration
let config = Config {
    mode: ForwardingMode::Local,
    remote: RemoteConfig {
        host: "example.com".to_string(),
        port: 22,
        user: "user".to_string(),
        key: Some("~/.ssh/id_rsa".to_string()),
    },
    forwarding_list: vec![
        "8080:127.0.0.1:8080".to_string(),
    ],
    timeout: Some(5),
};

// Create manager
let mut manager = TunnelManager::new(config)?;

// Start tunneling (blocks until shutdown)
manager.start().await?;
```

### Error Handling

```rust
use stun::{StunError, StunResult};

match TunnelManager::new(config) {
    Ok(manager) => { /* handle success */ }
    Err(StunError::Config(msg)) => { /* configuration error */ }
    Err(StunError::Ssh(msg)) => { /* SSH error */ }
    Err(e) => { /* other errors */ }
}
```

## Comparison with Original Python Script

| Feature | Python Script | Rust STUN |
|---------|---------------|-----------|
| **Performance** | Interpreted, slower startup | Compiled, fast startup |
| **Memory Usage** | Higher baseline memory | Lower memory footprint |
| **Error Handling** | Basic exception handling | Comprehensive error types |
| **Configuration** | JSON + manual parsing | Structured config with validation |
| **Logging** | Print statements | Structured logging with levels |
| **Testing** | Manual testing | Unit tests and integration tests |
| **Type Safety** | Runtime type errors | Compile-time type checking |
| **Concurrency** | Threading with GIL | Async/await with Tokio |
| **Distribution** | Requires Python interpreter | Single binary executable |

## Requirements

- Rust 1.70+ (for building from source)
- SSH client installed and available in PATH
- Network access to SSH server
- Appropriate SSH keys or password authentication configured

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass: `cargo test`
5. Submit a pull request

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE) for details.

## Acknowledgments

- Inspired by SSH tunneling automation scripts
- Built with the excellent [russh](https://github.com/warp-tech/russh) library
- Uses [tokio](https://tokio.rs/) for async runtime
- Configuration handling with [serde](https://serde.rs/)
- CLI interface with [clap](https://clap.rs/)
- Logging with [tracing](https://tracing.rs/)
