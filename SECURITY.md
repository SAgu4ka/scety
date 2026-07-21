# Security Policy

scety is a reverse proxy, so it sits directly in the request path between untrusted clients and
your upstream services. This document describes what it currently defends against, what it
doesn't (yet), and how to report a vulnerability.

## Supported versions

scety is pre-1.0 (`0.0.x`). There is no separate maintenance branch yet — only the latest release
on `master` is supported. Once a `1.0` is tagged, this section will be updated with a proper
support table.

## Reporting a vulnerability

Please **do not** open a public GitHub issue for a suspected security vulnerability.

Instead, open a [GitHub Security Advisory](https://github.com/SAgu4ka/scety/security/advisories/new)
for this repository (Security tab → "Report a vulnerability"), or contact the maintainer directly
if you don't have GitHub advisory access. Include:

- the affected version/commit,
- a description of the issue and its impact,
- steps to reproduce, if possible.

You should get an initial response within a few days. There's no bug bounty — this is a personal
project — but reports are taken seriously and credited in the fix.

## Threat model — what scety defends against today

- **HTTP request smuggling.** Requests with both `Content-Length` and `Transfer-Encoding`, with
  conflicting duplicate `Content-Length` headers, or with a `Transfer-Encoding` value other than
  exactly `chunked`, are rejected outright (`400`) instead of being forwarded with ambiguous
  framing.
- **`X-Forwarded-For` spoofing.** Any client-supplied `X-Forwarded-For` (or other header scety is
  configured to inject) is stripped from the request before scety's own value is written, so a
  client can't inject a fake origin IP that your upstream then trusts.
- **Resource exhaustion from a single client.** Per-IP concurrent connection limits, header/body
  read timeouts, and a maximum number of labels in the `Host` header are all enforced before a
  request is routed anywhere.
- **Misconfigured or expired TLS certificates going unnoticed.** `scety check-certs` independently
  validates certificate expiry, self-signed status, SAN coverage of the configured host(s), and
  the trust chain against Mozilla's root store (plus an optional custom CA bundle), separately
  from whatever the TLS stack does at handshake time.
- **Privilege escalation via the running service.** The systemd unit installed by `scety install`
  runs as a dedicated, shell-less, unprivileged `scety` user, binds privileged ports via
  `CAP_NET_BIND_SERVICE` instead of root, and sets `NoNewPrivileges=true`,
  `ProtectSystem=strict`, `ProtectHome=true`, `PrivateTmp=true`, `RestrictAddressFamilies`,
  `MemoryDenyWriteExecute=true`, and related systemd sandboxing directives.

## Known limitations / not yet addressed

Being upfront about these matters more here than in most projects, since scety is proxying live
traffic:

- **No global connection cap.** The per-IP connection limit doesn't bound total concurrent
  connections across many distinct source IPs — a distributed flood isn't currently mitigated.
- **Only the first request on a keep-alive connection is routed and validated.** Subsequent
  pipelined requests on the same TCP connection are relayed byte-for-byte to the upstream chosen
  for the first request, without being re-parsed or re-validated individually. This is a deliberate
  simplification (a single connection is assumed to stay on the same virtual host), not an
  oversight, but it's worth knowing if you're reasoning about per-request isolation.
- **Multi-port service configuration (`listens_port`, per-port `[upstream_<port>]` sections,
  `ports` load-balancing) is parsed and validated, but not yet wired into the runtime router.**
  Only the single `listen_port` / `[upstream]` form currently drives traffic.
- **No built-in request rate limiting or WAF-style filtering** beyond the connection/timeout/host
  limits described above.
- **No dependency vulnerability scanning until recently** — CI now runs a scheduled RustSec
  advisory check, but there's no history of continuous coverage before that.

If any of these matter for your deployment, please open an issue (a normal one, not a security
advisory, since these are documented limitations rather than vulnerabilities) so priorities can be
discussed.
