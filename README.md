# SoulSystem

Framework d'entités numériques autonomes avec support multi-fournisseurs LLM.

## Installation

```bash
cargo build --release -p souls
sudo cp target/release/souls /usr/local/bin/
```

## Utilisation

```bash
# Menu interactif de configuration
souls config

# TUI interactif (REPL riche)
souls tui

# Lancement autonome avec gateway HTTP/WS
souls run
```

## Architecture

| Couche | Crates | Rôle |
|--------|--------|------|
| **Runtime** | `soul_scheduler`, `soul_ipc`, `soul_storage`, `soul_matrix_engine` | Ordonnancement temps-réel, IPC, stockage vectoriel, GEMM SIMD |
| **Cognitive** | `soul_llm`, `soul_planner`, `soul_tools`, `soul_sandbox` | Multi-provider LLM, planification, outils, sandbox |
| **Entity** | `soul_entity`, `soul_gateway`, `soul_repl` | Entité autonome, API HTTP/WS, TUI |
| **Neuro** | `neural_*`, `semantic_*` | Métacognition, CRDT, chaos monkey, auto-guérison |
| **Persistence** | `soul_journal`, `soul_persistence` | WAL mmap, KV store Sled |
| **Telemetry** | `soul_telemetry` | Métriques Prometheus, profiling thermique |

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
