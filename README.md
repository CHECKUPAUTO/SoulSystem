# SoulSystem

Framework d'entités numériques autonomes avec support multi-fournisseurs LLM.

This repository is a unified Rust workspace that merges the original SoulSystem monolith, the autonomous-agent monolith (`soul_agent_core`, `soul_entity`, `souls`, ...), the SoulLink Neural Mesh, SciRust core, and CCOS (Causal Context Operating System).

## Build

**One-liner (recommended):**

```bash
curl -fsSL https://raw.githubusercontent.com/CHECKUPAUTO/SoulSystem/main/install.sh | sh
```

This downloads a prebuilt `soulsystem` binary for your platform (Linux/macOS,
x86_64/arm64), or builds from source if no release matches — installing the Rust
toolchain automatically if needed. Override the target dir with
`SOULSYSTEM_INSTALL_DIR` or pin a version with `SOULSYSTEM_VERSION`.

**npm:**

```bash
npm install -g soulsystem
```

**Cargo (from source):**

```bash
cargo install --git https://github.com/CHECKUPAUTO/SoulSystem soulsystem
# or, in a clone:
cargo build --release --bin soulsystem
```

<details>
<summary>Legacy <code>souls</code> TUI binary</summary>

```bash
# Fast workspace check
cargo check --workspace

# Run the main binary
cargo run --bin soulsystem -- [--dev] [--repl] [--daemon]

# Release build
cargo build --release
```
</details>

## Utilisation

```bash
# Main binary
cargo run --bin soulsystem -- --help

# Autonomous REPL
cargo run -p soul_repl --release

# Legacy `souls` TUI binary (when available)
cargo build --release -p souls
sudo cp target/release/souls /usr/local/bin/
```

## Architecture

| Couche | Crates | Rôle |
|--------|--------|------|
| **Runtime** | `soul_scheduler`, `soul_ipc`, `soul_storage`, `soul_matrix_engine` | Ordonnancement temps-réel, IPC, stockage vectoriel, GEMM SIMD |
| **Cognitive** | `soul_llm`, `soul_planner`, `soul_tools`, `soul_sandbox` | Multi-provider LLM, planification, outils, sandbox |
| **Entity** | `soul_entity`, `soul_gateway`, `soul_repl`, `soul_agent_core` | Entité autonome, API HTTP/WS, TUI |
| **Neuro** | `neural_*`, `semantic_*`, `soullink-*` | Métacognition, CRDT, chaos monkey, auto-guérison, HNN mesh |
| **Persistence** | `soul_journal`, `soul_persistence` | WAL mmap, KV store Sled |
| **Telemetry** | `soul_telemetry` | Métriques Prometheus, profiling thermique |
| **CCOS** | `ccos` | Causal Context Operating System — merged workspace member |

## Providers LLM

| Provider | Streaming | Embeddings |
|----------|-----------|------------|
| Ollama | NDJSON | Batch natif |
| OpenAI | SSE | API `/embeddings` |
| Anthropic | SSE | Non supporté |

## Commandes TUI

| Raccourci | Action |
|-----------|--------|
| `Ctrl+Shift+P` | Palette de commandes |
| `Ctrl+F` | Navigateur de fichiers |
| `Ctrl+R` | Recherche historique |
| `Ctrl+O` | Gestionnaire de sessions |
| `Ctrl+Y` | Copier dans le presse-papier |
| `Ctrl+E` | Exporter le chat |
| `Shift+Enter` | Saisie multi-lignes |

## Commandes CLI

```
/ask, /help, /models, /status, /plan, /run, /observe,
/clear, /save, /export, /files, /search
```

## Licence

MIT
