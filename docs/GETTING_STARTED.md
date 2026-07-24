# Getting started with SoulSystem

SoulSystem is a local-first autonomous-agent runtime. The recommended first
run keeps every listener on loopback and uses a local Ollama model.

## Install

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/Memorithm/SoulSystem/main/install.sh | sh
```

The installer verifies the SHA-256 checksum published with the GitHub release.
If no binary exists for the platform, it installs Rust and builds SoulSystem
from source. The default destination is `$HOME/.local/bin`; override it with
`SOULSYSTEM_INSTALL_DIR`.

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

## Security defaults

- Keep the gateway on `127.0.0.1` unless remote access is required.
- Set `SOULSYSTEM_GATEWAY_TOKEN` before any non-loopback deployment.
- Use `SOULSYSTEM_ENV=production` to enable fail-closed startup checks.
- Install `bubblewrap` on Linux for strong process isolation.
- Prefer provider API keys in environment variables or a secret manager. CLI
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
- `SOULSYSTEM_GATEWAY_TOKEN` warning: keep the gateway on loopback or configure
  a strong random token.
- Build failures in CUDA crates: build the root workspace only; use
  each GPU manifest explicitly when the matching GPU toolchain is installed.
