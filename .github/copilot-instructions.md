# STUN - SSH Tunneling Tool AI Development Guide

## Architecture Overview

STUN is a Rust SSH tunneling library and CLI that manages multiple SSH port forwarding connections with health monitoring and automatic reconnection. The architecture follows a layered approach:

- **`manager.rs`**: Core orchestrator managing tunnel lifecycle, health checks, and reconnections
- **`ssh.rs`**: SSH process wrapper that builds and executes SSH commands
- **`forwarding.rs`**: Parser for port forwarding specifications (`port:host:port` or `bind_addr:port:host:port`)
- **`config.rs`**: JSON configuration with validation (modes: `local`/`remote`)
- **`health.rs`**: Connection health monitoring with configurable timeouts
- **`error.rs`**: Comprehensive error handling using `thiserror`

## Key Development Patterns

### Configuration System
- Always use `Config::from_file()` for JSON configs, `Config::validate()` is called automatically
- Support both programmatic config creation (see `examples/programmatic.rs`) and file-based (see `examples/file_config.rs`)
- ForwardingSpec parsing handles two formats: `"8080:127.0.0.1:8080"` and `"0.0.0.0:8080:127.0.0.1:8080"`

### Error Handling Convention
```rust
// Use specific error types, not generic strings
StunError::Config(format!("Invalid bind port: {port}"))
StunError::Ssh(format!("Failed to start SSH process: {e}"))
```

### Async Architecture
- `TunnelManager::start()` runs indefinitely with background health checking
- Uses `tokio::process::Child` for SSH process management
- Health checks run on `Duration::from_secs(5)` intervals
- Exponential backoff for reconnection attempts

### Testing Strategy
- Each module has comprehensive unit tests under `#[cfg(test)]`
- Focus on spec parsing (`forwarding.rs`), config validation (`config.rs`), and command building (`ssh.rs`)
- Use `StunResult<T>` return type consistently

## Development Workflow

### Essential Commands
```bash
# Use Just for standardized commands (preferred)
just format    # taplo fmt + cargo +nightly fmt
just lint      # format check + clippy with strict rules
just test      # cargo test

# Alternative Make commands
make check     # cargo check
make build     # cargo build --release
```

### Code Quality Standards
- No `unwrap()` calls (use `expect()` with meaningful messages)
- All format strings use inline syntax: `format!("text {variable}")`
- Remove unused dependencies (checked by `cargo machete`)
- Use `tracing` for logging, not `println!`

### SSH Integration Points
- SSH commands built in `ssh.rs::build_command_string()`
- Forwarding mode determines SSH flags: `ForwardingMode::Local` → `-L`, `Remote` → `-R`
- Process lifecycle: spawn → monitor → kill on failure → restart
- Health checks test actual port connectivity, not just process status

## Library vs CLI Usage

### Library Integration
```rust
use stun::{Config, TunnelManager};

let config = Config::from_file("config.json")?;
let mut manager = TunnelManager::new(config)?;
manager.start().await?; // Runs until terminated
```

### CLI Entry Point
- `main.rs` uses `clap` for argument parsing
- Required `-c/--config` parameter for JSON config file
- Verbose logging via `-v` flags
- Graceful shutdown handling (Ctrl+C)

## Configuration Schema
```json
{
  "mode": "local|remote",
  "remote": {
    "host": "hostname",
    "port": 22,
    "user": "username", 
    "key": "/path/to/key"  // optional
  },
  "forwarding_list": [
    "8080:127.0.0.1:8080",
    "0.0.0.0:9000:database:3306"
  ],
  "timeout": 5  // optional, defaults to 2
}
```

## Common Pitfalls
- Always validate forwarding specs through `ForwardingSpec::parse()` before using
- SSH key path resolution handled by SSH client, not the library
- Health check failures accumulate; 3 consecutive failures trigger restart
- Bind address defaults to `127.0.0.1` if not specified in 3-part format
