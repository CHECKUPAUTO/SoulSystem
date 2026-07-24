# Getting started with SoulSystem

SoulSystem is a local-first autonomous-agent runtime. The recommended first
run keeps every listener on loopback and uses a local Ollama model.

## Install

Linux and macOS (x86-64 and arm64):

```sh
curl -fsSL https://raw.githubusercontent.com/Memorithm/SoulSystem/main/install.sh | sh
```

The installer verifies the SHA-256 checksum published with the GitHub release.
If no binary exists for the platform, it installs Rust and builds SoulSystem
from source. The default destination is `$HOME/.local/bin`; override it with
`SOULSYSTEM_INSTALL_DIR`.

Windows PowerShell (x86-64):

```powershell
irm https://raw.githubusercontent.com/Memorithm/SoulSystem/main/install.ps1 | iex
```

Alternative installation methods:

```sh
npm install -g soulsystem
cargo install --git https://github.com/Memorithm/SoulSystem soulsystem
```

## Configure and verify

Install and start [Ollama](https://ollama.com), then pull the default model:

```sh
ollama pull qwen3:8b
soulsystem --setup
soulsystem --doctor
```

`--setup` writes the provider, model, entity name, and gateway settings.
Provider keys are entered without echo and stored in macOS Keychain, Windows
Credential Manager, or Linux Secret Service; they are not written to the TOML
file. They can also be managed directly:

```sh
soulsystem secrets set llm/openai
soulsystem secrets status llm/openai
soulsystem secrets delete llm/openai
```

`--doctor` checks configuration directories, the LLM endpoint, bubblewrap
sandboxing, and gateway authentication without starting the agent.

Start the interactive local agent:

```sh
soulsystem --entity --repl
```

One-shot and planning modes:

```sh
soulsystem --ask "Summarize this workspace"
soulsystem --plan "Add tests for the parser"
```

Run `soulsystem --help` for the complete CLI.

## Friendly automations

Scheduled goals are stored in `automations.json` under the configured
configuration directory and loaded whenever `soulsystem --entity` starts.

```sh
soulsystem automation add morning-brief \
  --schedule daily@08:30 \
  --goal "Prepare my daily brief" \
  --priority 6
soulsystem automation add inbox-check \
  --schedule every-15m \
  --goal "Review new inbox items"
soulsystem automation list
soulsystem automation disable inbox-check
```

Five-field cron expressions are accepted too. Times are currently interpreted
in UTC.

## Browser, MCP, skills, and subagents

- Start Chrome with a local CDP endpoint, for example
  `google-chrome --headless --remote-debugging-port=9222`. The agent can then
  use the typed `browser_read` tool. CDP endpoints must be loopback addresses.
- `mcp_call` connects to an explicitly supplied `ws://` or `wss://` MCP
  endpoint. It is classified as a state-changing network operation and passes
  through the approval gate.
- Put Markdown skill files in `<config_dir>/skills`. They are loaded at entity
  startup, matched by trigger, and included in the actual LLM prompt path.
- Authenticated gateway clients can create and inspect subagents with
  `POST /v1/subagents` and `GET /v1/subagents`.

## Signal and iMessage

Signal uses the unofficial `signal-cli` JSON-RPC daemon:

```sh
signal-cli daemon --http=127.0.0.1:8080
export SIGNAL_CLI_HTTP_URL=http://127.0.0.1:8080
export SIGNAL_ACCOUNT=+33123456789  # optional in single-account mode
soulsystem --entity
```

The provider consumes incoming SSE events, sends their text to the entity, and
replies through JSON-RPC. Keep `signal-cli` current because it is not an
official Signal client.

iMessage is outbound-only, macOS-only, and opt-in:

```sh
export SOULSYSTEM_IMESSAGE_ENABLED=true
```

It automates the local Messages app and requires Apple Events permission.
Apple does not expose a general headless iMessage bot API, so inbound iMessage
support is not claimed.

## Remote and multi-user gateway

Create a certificate/key pair (or use certificates issued by your internal or
public CA), then configure named bearer tokens:

```sh
export SOULSYSTEM_ENV=production
export SOULSYSTEM_GATEWAY_TOKENS='alice=replace-with-a-long-token,bob=another-long-token'
soulsystem --entity \
  --gateway-addr 0.0.0.0:7878 \
  --tls-cert /etc/soulsystem/tls/fullchain.pem \
  --tls-key /etc/soulsystem/tls/private-key.pem
```

`--tls-cert` and `--tls-key` must be supplied together. The listener uses
native Rustls TLS. The legacy single-user `SOULSYSTEM_GATEWAY_TOKEN` remains
supported. Tokens identify operators but do not yet implement per-route RBAC;
use separate deployments where strong tenant isolation is required.

## Security defaults

- Keep the gateway on `127.0.0.1` unless remote access is required.
- Set `SOULSYSTEM_GATEWAY_TOKENS` (or the legacy singular variable) and native
  TLS before any non-loopback deployment.
- Use `SOULSYSTEM_ENV=production` to enable fail-closed startup checks.
- Install `bubblewrap` on Linux for strong process isolation.
- Prefer `soulsystem secrets set` or provider environment variables. CLI
  arguments can be visible in process listings.

Review [Security](SECURITY.md), [security invariants](security/SECURITY_INVARIANTS.md),
and the [latest framework audit](audit/SOULSYSTEM_FULL_AUDIT_2026-07-24.md)
before exposing a service. The deployment-specific threat model is pending
owner confirmation of exposure, tenancy, and data sensitivity.

## Build from a clone

```sh
git clone https://github.com/Memorithm/SoulSystem.git
cd SoulSystem
cargo check -p soulsystem
cargo test -p soul_agent_core -p soul_tools -p soul_gateway -p ccos
cargo build --release --bin soulsystem
./target/release/soulsystem --doctor
```

GPU crates use standalone manifests and are not needed
for the default CPU/local installation.

## Troubleshooting

- `LLM unavailable`: start Ollama or pass the correct `--llm-url`.
- `bubblewrap not found`: install the `bubblewrap` package; development can
  continue with reduced isolation, but production should not.
- Gateway auth/TLS warning: keep the gateway on loopback or configure strong
  named tokens and both PEM files.
- Secret Service unavailable on headless Linux: supply the provider-specific
  environment variable or start a Secret Service implementation for the user.
- Build failures in CUDA crates: build the root workspace only; use
  each GPU manifest explicitly when the matching GPU toolchain is installed.
