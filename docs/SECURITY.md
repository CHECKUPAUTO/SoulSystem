# SoulSystem security guide

SoulSystem can execute tools, access files, call external model providers, and
open local HTTP/WebSocket listeners. Treat it as privileged developer tooling,
not as an untrusted public service.

## Safe default deployment

- Bind gateways to loopback (`127.0.0.1` or `::1`).
- Use `SOULSYSTEM_ENV=production` outside local development. Production startup
  fails closed when required controls are missing.
- Set a strong `SOULSYSTEM_GATEWAY_TOKEN` before any non-loopback deployment.
- Install bubblewrap on Linux and keep sandbox enforcement enabled.
- Run under a dedicated, unprivileged operating-system account.
- Keep provider credentials in environment variables or a secret manager.

Run `soulsystem --doctor` before the first start and after configuration
changes.

## Security architecture

- Tool calls use a typed registry, capability classification, approval gates,
  and the sandboxed execution path.
- Tool output is screened before it enters planner history or causal memory.
- Gateway state-changing routes require authentication in production.
- CCOS snapshots and runtime state use atomic, fsync-backed writes and verify
  hash-chain integrity when restored.
- Production readiness checks live in `crates/soul-prod-guard`.

The exact requirements and evidence anchors are documented in
[`security/SECURITY_INVARIANTS.md`](security/SECURITY_INVARIANTS.md). The
hardening backlog is in
[`security/PRODUCTION_HARDENING_PLAN.md`](security/PRODUCTION_HARDENING_PLAN.md).

## Exposure warning

Do not expose dashboard, gateway, MCP, PTY, webhook, or metrics endpoints
directly to the Internet. Put remotely accessed endpoints behind authenticated
TLS termination and restrict the network path. Local-only defaults are a risk
reduction, not an authentication mechanism.

## Persistence and logs

Runtime data is stored under the configured SoulSystem data and log
directories; no fixed `/var/log` path is assumed. Audit records and CCOS event
logs provide integrity evidence, but host-level access can still delete them.
Use filesystem permissions and external log shipping where tamper resistance is
required.

## Reporting vulnerabilities

Do not open a public issue containing exploit details or secrets. Contact the
repository maintainers privately through GitHub security reporting when
available.
