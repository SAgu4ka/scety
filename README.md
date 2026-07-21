# scety

[![CI](https://github.com/SAgu4ka/scety/actions/workflows/ci.yml/badge.svg)](https://github.com/SAgu4ka/scety/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**scety** is a reverse proxy written in Rust, inspired by nginx. It's a personal project — not
yet production-hardened everywhere — but it already implements a fair amount of the boring, easy
to get wrong parts of running a reverse proxy safely: ambiguous request framing, spoofable
headers, certificate validation, and a locked-down systemd service by default.

## What it does

- Routes incoming HTTP requests to upstream services based on the `Host` header
- Supports multiple virtual hosts on the same port, with flexible wildcard patterns
- Adds a trustworthy `X-Forwarded-For` header to proxied requests (any client-supplied value is
  stripped first, so it can't be spoofed)
- Proxies WebSocket upgrades
- Serves a fallback page if no service configs are found
- Installs and runs itself as a systemd service, sandboxed and running as an unprivileged user
- Ships a `check` / `check-certs` command pair so you can validate configuration and TLS
  certificates before (re)starting the service

## Security-relevant design

A few things worth calling out explicitly, since they're the parts that are easy to get wrong in
a hand-rolled proxy:

- **Request smuggling guards.** Requests with both `Content-Length` and `Transfer-Encoding`, with
  conflicting duplicate `Content-Length` values, or with a `Transfer-Encoding` other than exactly
  `chunked` are rejected with `400` rather than forwarded ambiguously.
- **`X-Forwarded-For` can't be spoofed by the client.** Any incoming `X-Forwarded-For` (or other
  overridden header) is stripped from the request before scety's own value is inserted.
- **TLS certificate validation as a first-class command.** `scety check-certs` parses each
  configured certificate, checks expiry, self-signed status, SAN coverage of the configured
  host(s), and validates the trust chain against Mozilla's root store (plus an optional custom CA
  bundle) using rustls — independent of whatever TLS termination is actually doing at runtime.
- **Least privilege by default.** `scety install` creates a dedicated, shell-less `scety` system
  user, binds privileged ports via `CAP_NET_BIND_SERVICE` instead of running as root, and installs
  a systemd unit with `NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome`, `PrivateTmp`,
  `RestrictAddressFamilies`, `MemoryDenyWriteExecute`, and friends already turned on.
- **Per-IP connection limiting**, request header/body timeouts, and a configurable limit on the
  number of labels in a `Host` header, all to bound resource usage from a single misbehaving or
  hostile client.

See [SECURITY.md](SECURITY.md) for the full threat model, known limitations, and how to report a
vulnerability.

## Installation

```bash
cargo build --release
sudo ./target/release/scety run
```

On first run outside of systemd, scety installs itself as a systemd service (creating the `scety`
system user, the service unit, and `/etc/scety`) and starts it. From then on, manage it with the
commands below.

## Commands

| Command | Description |
|---|---|
| `scety run` | Start the server (or install as a systemd service, on first run) |
| `scety install` | Install scety as a systemd service without starting it via `run` |
| `scety stop` | Stop the service |
| `scety reload` | Restart the service to pick up configuration changes |
| `scety status` | Show `systemctl status scety` |
| `scety check` | Validate all service configuration files without starting anything |
| `scety check-certs` | Validate configured TLS certificates (expiry, chain, hostname match) |
| `scety uninstall` | Stop, disable, and remove the systemd service |

## Configuration

scety reads two kinds of configuration, both under `/etc/scety` by default:

- **`/etc/scety/scety.toml`** — global settings: connection/timeout limits, buffer sizes, an
  optional extra trusted CA bundle, and headers applied globally to upstream requests / client
  responses.
- **`/etc/scety/services/*.toml`** — one file per proxied service. Every `.toml` file in this
  directory (searched recursively) is loaded and validated independently; a broken file is
  skipped with a logged error rather than blocking the others.

Minimal service config:

```toml
mode = "reverse_proxy"
host = "example.com"
listen_port = 80

[upstream]
port = 3000
```

With TLS and custom headers:

```toml
mode = "reverse_proxy"
host = "*.example.com"
listen_port = 443

[upstream]
port = 3000
service_timeout = "30s"

[ssl]
cert = "/etc/ssl/cert.pem"
key = "/etc/ssl/key.pem"
# or: acme = true, acme_email = "...", acme_domains = ["example.com"]

[headers.upstream]
X-Custom-Header = "value"

[headers.response]
X-Frame-Options = "SAMEORIGIN"
```

Global `/etc/scety/scety.toml`:

```toml
[limitation]
ip_limitation = 20          # max concurrent connections per client IP, -1 disables
client_headers_timeout = "5s"
client_body_timeout = "5s"
max_host_labels = 20         # reject Host headers with more labels than this, -1 disables

[limit_buffers]
client_header = 16384        # bytes

# [tls]
# trusted_ca_bundle = "/etc/scety/extra-ca-bundle.pem"

# [headers.upstream]
# X-Proxy-By = "scety"

# [headers.response]
# X-Content-Type-Options = "nosniff"
```

> **Multi-port services (`listens_port` / per-port `[upstream_<port>]` / `ports` load-balancing)
> are parsed and validated but not yet wired into the runtime router.** Right now only the single
> `listen_port` / `[upstream]` form actually drives traffic; the multi-port fields are groundwork
> for a future release. Don't rely on them yet.

## Wildcards

Scety features a flexible routing engine that supports both single-level and multi-level
wildcards:

* **`*` (Single-level):** Matches exactly one domain segment. For example, `*.example.com` matches
  `api.example.com`, but will **not** match `v1.api.example.com`.
* **`**` (Multi-level):** Matches one or more domain segments recursively. For example,
  `**.example.com` successfully matches both `api.example.com` and `v1.dev.api.example.com`.

> 🚀 **What makes Scety different:** Unlike traditional reverse proxies which restrict wildcards to
> the edges of a host string (e.g., only at the very beginning or end), Scety's routing engine can
> handle complex masks with `*` and `**` embedded **anywhere** within the pattern (e.g.,
> `api.*.internal.**`).

### Lookup performance

Not all wildcard patterns cost the same. Scety classifies every pattern once, at config-load time,
and routes lookups through the cheapest applicable strategy:

| Pattern type | Example | Lookup cost |
|---|---|---|
| Exact host | `example.com` | `O(1)` — hash map lookup |
| Edge wildcard (`*`) | `*.example.com`, `www.example.*` | `O(1)` — hash map lookup |
| Edge wildcard (`**`) | `**.example.com`, `foo.styles.**` | `O(k)` — k = number of distinct known-label counts registered |
| Catch-all | `*`, `**` | `O(1)` — single fallback lookup |
| Mixed / mid-string wildcard | `api.*.internal.**` | `O(P × H)` — dynamic-programming match against pattern length P and host length H |

In practice, almost every real-world config falls into the first three rows, all of which resolve
in constant time. The dynamic-programming path only runs for patterns that mix `*`/`**` away from
the edges — flexible, but the most expensive option — and results are cached per resolved `Host`
value, so repeat lookups skip it entirely.

## Requirements

- Linux with systemd
- Rust 2024 edition (MSRV roughly 1.85+ — the code uses let-chains, which need a recent stable
  compiler)

## Contributing

Issues and PRs are welcome. CI runs `cargo build`, `cargo test`, `cargo clippy -- -D warnings`,
`cargo fmt --check`, and a weekly dependency vulnerability scan against the RustSec advisory
database.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).