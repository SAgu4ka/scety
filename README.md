# scety

Hi everyone! scety is my own reverse proxy written in Rust and inspired by nginx. I plan to continue developing it and hopefully eventually make it production-ready.

## What it does

- Routes incoming HTTP requests to upstream services based on the `Host` header
- Supports multiple virtual hosts and ports
- Adds `X-Forwarded-For` header to proxied requests
- Serves a fallback page if no configs are found
- Installs and runs as a systemd service

## Installation

```bash
cargo build --release
sudo ./target/release/scety run
```

On first run outside of systemd, scety will install itself as a systemd service automatically.

## Configuration

Put `.toml` config files into the services config directory. Example:

```toml
mode = "proxy"
host = "example.com"
listen_port = 80

[upstream]
port = 3000
```

## Commands

| Command | Description |
|---|---|
| `scety run` | Start the server (or install as systemd service) |
| `scety install` | Install scety in systemd |
| `scety stop` | Stop the service |
| `scety reload` | Reload configuration |
| `scety status` | Check service status |
| `scety check` | Validate config files |
| `scety uninstall` | Remove the systemd service |

## Requirements

- Linux with systemd
- Rust 2024 edition