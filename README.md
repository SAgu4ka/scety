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
## Wildcards

Scety features a flexible routing engine that supports both single-level and multi-level wildcards:

* **`*` (Single-level):** Matches exactly one domain segment. For example, `*.example.com` matches `api.example.com`, but will **not** match `v1.api.example.com`.
* **`**` (Multi-level):** Matches one or more domain segments recursively. For example, `**.example.com` successfully matches both `api.example.com` and `v1.dev.api.example.com`.

> 🚀 **What makes Scety different:** Unlike traditional reverse proxies which restrict wildcards to the edges of a host string (e.g., only at the very beginning or end), Scety's routing engine can handle complex masks with `*` and `**` embedded **anywhere** within the pattern (e.g., `api.*.internal.**`).

### Lookup performance

Not all wildcard patterns cost the same. Scety classifies every pattern once, at config-load time, and routes lookups through the cheapest applicable strategy:

| Pattern type | Example | Lookup cost |
|---|---|---|
| Exact host | `example.com` | `O(1)` — hash map lookup |
| Edge wildcard (`*`) | `*.example.com`, `www.example.*` | `O(1)` — hash map lookup |
| Edge wildcard (`**`) | `**.example.com`, `foo.styles.**` | `O(k)` — k = number of distinct known-label counts registered |
| Catch-all | `*`, `**` | `O(1)` — single fallback lookup |
| Mixed / mid-string wildcard | `api.*.internal.**` | `O(P × H)` — dynamic-programming match against pattern length P and host length H |

In practice, almost every real-world config falls into the first three rows, all of which resolve in constant time. The dynamic-programming path only runs for patterns that mix `*`/`**` away from the edges — flexible, but the most expensive option, so use it sparingly if you're routing high request volumes.

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