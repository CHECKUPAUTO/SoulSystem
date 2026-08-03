# Gateway Parity — soullink-gateway ⇄ soulsystem-gateway

Status: worklog, 2026-06. Goal: make `soullink-brain/soullink-gateway` a strict
functional drop-in for the npm-distributed `soulsystem-gateway` so we can retire
the old gateway binary we still run in production.

The "npm gateway" is actually a Rust binary distributed via an npm
installer wrapper (`soulsystem-gateway/npm/cli.js` installs the binary built from
`soulsystem-gateway/src/*.rs`). The reference contract is therefore those Rust
sources, not JavaScript.

---

## 1. Root cause of the incompatibility

`soullink-gateway/src/ws/protocol.rs` *claimed* to "match the reference gateway
protocol" but did not. The divergences that prevented cutover:

| Aspect | Reference gateway | soullink (before) | Status |
|--------|--------------------|--------------------|--------|
| Frame `type` tags | `req` / `res` / `event` (lowercase, `#[serde(rename)]`) | `Connect`/`HelloOk`/`Req`/`Res`/`Event` (PascalCase variant names) | **fixed** |
| Handshake | `req{method:"connect"}` → `res{payload: hello-ok}` | top-level `Connect` frame → `HelloOk` frame | **fixed** |
| `hello-ok` payload | `{type:"hello-ok", protocol:3, session_id, device_token, policy{…}}` | `{session_id, version, methods, events}` | **fixed** |
| Protocol version | `PROTOCOL_VERSION = 3`, negotiated via `min/max_protocol` | none | **fixed** |
| `ConnectRequest` | `min_protocol, max_protocol, client{id,version,platform}, role, scopes, caps, auth{token}` | `version, auth, client{id,name,platform}` | **fixed** |
| Response shape | `{id, ok, …payload \| …error}` (flattened, no null fields) | `{id, ok, payload:null, error:null}` (both always present) | **fixed** |
| Error codes | `INVALID_PARAMS, UNSUPPORTED_PROTOCOL, AUTH_REQUIRED, AUTH_FAILED, UNKNOWN_METHOD` | ad-hoc (`NOT_AUTHENTICATED`, `HANDLER_ERROR`, …) | **fixed** |
| Auth | static operator token + 30-day device tokens (`AuthManager`) | none — every client auto-authenticated | **fixed** |
| `policy` | `heartbeat_interval_ms:30000, max_message_size:65536, idle_timeout_ms:300000` | none | **fixed** |
| `/status` endpoint | `{version, sessions, port}` | missing | **fixed** |
| `PORT` env / default | `PORT` env, default `18889` | clap `--port` default `9092`, no env | **`PORT` env honored** |
| `GATEWAY_TOKEN` env | operator token from `GATEWAY_TOKEN` | none | **fixed** |

---

## 2. What this change implements (strict parity)

- **`ws/protocol.rs`** — rewritten as a strict replica of
  `soulsystem-gateway/src/protocol.rs`: `req`/`res`/`event` frames, flattened
  `res`, `PROTOCOL_VERSION = 3`, `ConnectRequest`/`HelloOk`/`PolicyInfo`/`Role`
  with the exact reference fields, error-code constants, and `error_response`/
  `success_response`/`event` builders. Wire-format unit tests assert the
  serialized JSON matches the reference (tags, no null fields, policy values).
- **`ws/auth.rs`** (new) — strict replica of the reference `AuthManager`:
  static operator token validation + 30-day `oc_dev_*` device tokens.
- **`ws/handler.rs`** — rewritten handshake: a `req{method:"connect"}` is
  validated (protocol range, token) and answered with a `res` carrying
  `hello-ok`; pre-auth non-connect methods get `AUTH_REQUIRED`. The richer RPC
  method set (chat, completion, providers.list, models.list, …) is preserved as
  a **superset** — reference clients never call those, so compatibility holds.
- **`cli/run.rs`** — honors `PORT` (overrides `--port`) and `GATEWAY_TOKEN`
  (operator token) for reference-style env invocation, adds the `/status`
  endpoint (`{version, sessions, port}`), and threads the `AuthManager` into the
  WS handler.

The frames, handshake, error codes, policy, auth and `/health`+`/status`+`/ws`
endpoints now match the reference; a superset of RPC methods remains available.

---

## 3. Remaining for full production cutover

These are additive and do not block the protocol drop-in:

1. **Channel providers** *(remaining)* — the reference exposes `ENABLE_TELEGRAM`
   / `ENABLE_WHATSAPP` with `TELEGRAM_TOKEN` / `WHATSAPP_SESSION`; the WhatsApp
   and webhook providers (`soulsystem-gateway/src/providers/{whatsapp,webhook}.rs`)
   need equivalents alongside the existing Telegram long-poll loop.
2. **Bind host** — **done.** `--bind` (and the `GATEWAY_BIND` env, which wins)
   resolves `loopback`/`local` → `127.0.0.1` and `all`/`any`/`public`/`0.0.0.0`
   → `0.0.0.0` (the reference's bind), any other value used verbatim. Default
   stays loopback for safety; `--bind all` matches the reference.
3. **`WORKERS` env** — **done.** `main` builds the tokio runtime manually,
   honoring `WORKERS` clamped to `1..=64` (default 4) — the reference reads the
   same env; here it is actually applied to the runtime.
4. **Heartbeat / idle enforcement** *(remaining)* — the `policy` values are
   advertised; the server should also *enforce* idle-timeout and heartbeat per
   the policy.

Once (1) and (4) land, the old gateway binary can be retired. The routing upgrades
in `docs/RESEARCH_FRONTIER_2026.md` §2 then make the Rust gateway *better* than
the reference, not merely equal.
