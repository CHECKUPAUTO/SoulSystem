# OpenEvolve v4.3 — 100% Rust

[![CI](https://github.com/CHECKUPAUTO/openevolve/actions/workflows/ci.yml/badge.svg)](https://github.com/CHECKUPAUTO/openevolve/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Moteur d'optimisation de code par évolution guidée par LLM — entièrement Rust, zéro dépendance Python. Persistance SQLite, embeddings sémantiques (TRIBE-compatible), Map-Elites, pre-filter nCPU, sandbox Docker, dashboard live, métriques Prometheus, suivi de coûts par provider.

## Nouveautés v4.3

Wave D+E preview :

- **`cache.rs`** — Cache LLM clé-SHA avec TTL et LRU. Économies immédiates sur les prompts répétés.
- **`distributed.rs`** — Scaffolding trait `MessageBus` + `LocalBus` + `DistributionManager`. Prêt pour NATS/Redis lundi avec le Thor.
- **`evolve_tag.rs`** + sous-commande CLI — Parser `// @evolve fitness=... iterations=...` (Rust / Python / JS / Go).
- **`auto_pr.rs`** — `create_pr_on_target()` : PR sur un repo *cible* (pas openevolve), avec body Markdown riche.
- **`mutation_engine`** — `structured_rate` **adaptatif** via `note_outcome()`. Le moteur dérive vers la voie qui marche.
- **`.github/workflows/evolve-on-tag.yml`** — Workflow GitHub Actions qui scanne `@evolve`, lance une évolution par tag en matrix, post les résultats en commentaire de PR.

## Nouveautés v4.2

| v4.1 | v4.3 |
|---|---|
| Population en RAM (`Arc<RwLock<Vec<Vec<Program>>>>`) | **SQLite + r2d2 pool** persistent, crash-safe, queries historiques |
| Aucun WAL | **JSONL append-only WAL** + recovery torn-write pour replay déterministe |
| Pas d'embeddings | **Client TRIBE-compatible** (HTTP `:7440/embed`) + fallback feature-hash 256d |
| NSGA-II seul | **NSGA-II + Map-Elites** (grille complexity_class × loc × fn_count) — sort des optima locaux |
| Crossover LLM-only | **+ AST crossover** réel (swap subtree tree-sitter) |
| Pas de pre-filter | **Client nCPU** optionnel (`:7450`) pour reject sub-ms des candidats foireux |
| Pas de métriques | **Exporter Prometheus** (`/metrics`) — counters, gauges, histogrammes |
| Pas de tracking coût | **CostTracker** par provider + pricing OpenAI/Anthropic/Gemini + hard cap `max_usd` |
| Dashboard agrégé | **+ Per-island chart + cost panel** dans le dashboard live |
| `Program` minimal | **`Program.language`** + **`Program.provenance`** (rétro-compat `#[serde(default)]`) |
| 41 tests | **60 tests** |

## Architecture

```
src/
├── main.rs              CLI : evolve / server / diagnose / analyze / library
│                              + runs / replay-wal / db-export (v4.3)
├── lib.rs               Re-exports
│
├── persistence.rs       SQLite (rusqlite + r2d2 pool, schema migrations)
├── wal.rs               JSONL WAL append-only + recovery
├── embedding.rs         Client TRIBE HTTP + fallback feature-hash
├── map_elites.rs        Quality-diversity grid
├── ncpu_filter.rs       Sub-ms pre-filter HTTP nCPU
├── metrics.rs           Prometheus exporter + CostTracker + pricing
│
├── analysis.rs          AST tree-sitter (Py/Rs/JS/Go)
├── auto_pr.rs           Génération de PR GitHub
├── checkpoint.rs        Sauvegarde / reprise
├── config.rs            TOML config (10 sections)
├── database.rs          ProgramDatabase — îles + migration
├── diagnostic.rs        Scan journalctl + source
├── evaluator.rs         Exécute le binaire évaluateur
├── evolution.rs         Boucle principale (wire les 28 modules)
├── fuzzer.rs            Inputs aléatoires
├── llm.rs               Client Ollama natif
├── meta_evolution.rs    Évolution des hyperparamètres
├── mutation_engine.rs   Mutations AST + créatives + ast_crossover()
├── pareto.rs            NSGA-II (perf / lisibilité / taille / sécurité)
├── program.rs           Type Program (id, code, score, metrics, language, provenance)
├── prompt.rs            PromptBuilder (top-N + diverse)
├── prompt_memory.rs     Mémorisation prompts performants
├── providers.rs         Trait LlmProvider (Ollama, OpenAI, Anthropic, Gemini)
├── repair.rs            Auto-réparation syntaxique
├── sandbox.rs           Docker + fallback process + escape detection
├── semantic_diff.rs     Diff AST-aware
├── server.rs            REST + WS + dashboard + /metrics + /islands + /costs
├── test_generator.rs    Tests auto-générés
└── transfer.rs          Transfer learning entre tâches

configs/
├── default.toml
├── openclaw_t430.toml
└── openclaw_t430_quick.toml

examples/my_task/
├── Cargo.toml
└── src/
    ├── solution.rs       Programme initial à évoluer
    └── evaluator.rs      Évaluateur Rust (stdin → JSON)
```

## Prérequis

- Rust ≥ 1.82 stable (édition 2021)
- [Ollama](https://ollama.ai) local (port 11434) — modèles `qwen3-coder-next:cloud` (mutations) et `glm-5:cloud` (feedback)
- Optionnel : Docker (sandbox), TRIBE (`:7440/embed`), nCPU (`:7450/classify`)

## Build

```bash
cargo build --release
cargo build --release --manifest-path examples/my_task/Cargo.toml
```

Binaire produit : `target/release/openevolve` (~9 Mo strippé).

## Usage

```bash
openevolve evolve   [opts]                      # défaut
openevolve server   [--bind 127.0.0.1:8460]
openevolve diagnose [--source-dir src]
openevolve analyze  <file.rs> [--language rust]
openevolve library  [--limit 20]

# v4.3 — accès à la base persistante
openevolve runs       [--limit 20]              # liste les runs SQLite
openevolve replay-wal <path>  [--limit 1000]    # replay déterministe du WAL
openevolve db-export  <run-id> [--out export.json --limit 50]

# Options globales
openevolve --check          # vérifie les prérequis
openevolve --quick
openevolve --print-config
openevolve --list-checkpoints
openevolve -c configs/openclaw_t430.toml -i 500
```

### Dashboard live

```bash
openevolve server --bind 127.0.0.1:8460
# Ouvrir http://localhost:8460/
```

Affiche en live : best score, total programs, iteration, dernier delta, **USD dépensés**, **bars par île** (taille + best score), **coût par provider**, flux d'événements WebSocket.

### Configuration v4.3

```toml
[persistence]
enabled     = true
sqlite_path = "openevolve_output/openevolve.db"
wal_path    = ""                                  # vide = <output_dir>/wal.jsonl

[embedding]                                       # TRIBE-compatible
endpoint    = "http://localhost:7440/embed"       # vide = fallback local
timeout_secs = 30
max_batch   = 32

[ncpu]
endpoint        = "http://localhost:7450/classify"  # vide = désactivé
timeout_ms      = 50
reject_threshold = 0.85

[map_elites]
enabled            = true
sample_probability = 0.20

[cost]
max_usd        = 5.00     # 0 = pas de plafond
warn_fraction  = 0.80     # warn à 80% du plafond
```

### Endpoints REST (v4.3)

| Route | Description |
|---|---|
| `GET /` | Dashboard HTML live |
| `GET /health` | Liveness probe |
| `GET /status` | Aggregate (best score, total programs, config) |
| `GET /library?limit=N` | Top programmes |
| `GET /islands` | Per-island summary (size, best, mean) |
| `GET /costs` | Coût par provider + total USD |
| `GET /metrics` | Prometheus exposition format |
| `WS  /ws` | Stream temps réel des iterations |

### Métriques Prometheus exposées

```
openevolve_iterations_total{status="ok|failed|skipped"}
openevolve_mutations_total{op="RenameVariable|...|Creative"}
openevolve_repair_attempts_total
openevolve_repair_successes_total
openevolve_evaluations_total{outcome="ok|zero|ncpu_reject"}
openevolve_best_score
openevolve_population_size{island="0|1|2|3"}
openevolve_breaker_open{provider="ollama|openai|..."}
openevolve_iteration_duration_seconds (histogram)
openevolve_llm_tokens_in_total{provider, model}
openevolve_llm_tokens_out_total{provider, model}
openevolve_llm_usd_total{provider, model}  (micro-dollars)
```

### Multi-provider LLM

```rust
use openevolve::providers::{create_provider, ProviderConfig, ChatMessage, chat_with_retry};

let cfg = ProviderConfig {
    provider: "anthropic".into(),
    api_key: Some(std::env::var("ANTHROPIC_API_KEY")?),
    model: "claude-opus-4-7".into(),
    ..Default::default()
};
let p = create_provider(&cfg);
let msgs = [ChatMessage { role: "user".into(), content: "Optimize this".into() }];
let answer = chat_with_retry(p.as_ref(), &msgs, 0.7, 1024, 3, 500).await?;
```

## Tests

```bash
cargo test --lib                          # 60 tests
cargo clippy --all-targets -- -D warnings # clean
cargo fmt --check                         # clean
```

## Licence

MIT — voir [CHANGELOG.md](CHANGELOG.md).
