# Architecture — SoulSystem

**Date :** 2026-06-16  
**État :** généré depuis le disque, après unification du runtime autonome.

## Résumé exécutif

SoulSystem est un monorepo Rust de **171 crates Cargo** organisé autour de **deux points d'entrée** :

| Binaire | Rôle | Statut |
|---|---|---|
| `souls` | **Runtime autonome canonique** — entité autonome + gateway HTTP/WS + REPL | actif, recommandé |
| `soulsystem` | **Runtime unifié** — legacy operator + entité autonome (`--entity`) + gateway + skills | actif, recommandé |

Le cœur autonome fonctionnel est constitué d'une dizaine de crates fortement intégrées. Le reste du workspace (SoulLink brain, AVID, SciRust, CCOS) compile mais n'est pas encore activement câblé au runtime autonome.

## Stack autonome opérationnelle

```
┌─────────────────────────────────────────────────────────────┐
│  CLI / binaire : souls                                       │
├─────────────────────────────────────────────────────────────┤
│  soul_gateway  ── HTTP/WS API + providers (Telegram/Discord/Slack/WhatsApp) │
│  soul_entity   ── SoulEntity, boucle cognitive, mémoire     │
│    ├─ soul_agent_core ── ReAct loop (utilisée par ask)      │
│    ├─ soul_llm        ── multi-provider Ollama/OpenAI/Anthro │
│    ├─ soul_planner    ── goal/plan + working memory          │
│    ├─ soul_tools      ── async shell + file ops + registry │
│    ├─ soul_sandbox    ── whitelist + sandbox + timeout       │
│    ├─ soul_persistence── Sled KV + lineage                  │
│    ├─ soul_agent_contracts ── agent contract/skill facade   │
│    └─ subsystems      ── journal, forge, orchestrator, ...  │
│  soul_repl       ── REPL conversationnel                   │
└─────────────────────────────────────────────────────────────┘
```

## Crates orphelins (non câblés par `souls` ou `soulsystem`)

~104 crates workspace ne sont pas dépendus directement par les binaires actifs. Parmi les plus importants :

- **SoulLink brain** (`soullink-core`, `soullink-memory`, `soullink-orchestrator`, `soullink-moe`, `soullink-senate`, `soullink-reasoning`, `soullink-inference`...)
- **AVID** (`avid-core`, `avid-cortex`, `avid-scout`, `avid-model-router`, `avid-orchestrator`, `avid-skills`...)
- **SciRust** (`scirust-core`, `scirust-gpu`, `scirust-autodiff`, `scirust-trading-core`...)
- **CCOS**, `soul_cortex`, `soul_cluster`, `soul_perception`, `soul_scout`, `soul_acoustic`, `soul_attention`, `soul_rag`, `soul_graph_memory`, `soul_conversations`, `soul_webfetch`, `soul_cognitive`, `soul_automation`, `soul_api`...

> Ils sont compilés par `cargo check --workspace` mais n'interviennent pas dans la boucle autonome actuelle.

## Changements récents

1. `Cargo.toml` racine : suppression de la dépendance invalide `souls`.
2. `soul_entity` : intégration de `soul_agent_core::AutonomousAgent` dans la méthode `ask`.
3. `soul_sandbox` : correction d'un bug de sécurité critique (normalisation trop agressive qui masquait redirections/pipes).
4. `soul_gateway` : ajout du module `providers` avec implémentations Telegram (long-poll bot), Discord, Slack et WhatsApp (webhook-based).
5. `soulsystem` : ajout du wizard interactif `--setup` (CLI) et `--setup-tui` (TUI ratatui) pour configurer LLM, entité et gateway.
6. `soul_entity` tests : robustesse au scan automatique d'agents.
7. `soul-kernel` tests : correction d'un test `unwrap_err()` trop strict.
8. `soulsystem` : ajout du mode `--entity` qui lance `SoulEntity` + gateway + REPL/boucle autonome.

## Build & test

```bash
# Vérification rapide
cargo check --workspace

# Binaires
cargo build -p souls
cargo build --bin soulsystem

# Tests du cœur autonome (rapides)
cargo test --lib -p soul_agent_core -p soul_entity -p soul_llm \
           -p soul_tools -p soul_planner -p soul_repl \
           -p soul_persistence -p soul_sandbox -p soul_gateway -p soul_agent_contracts

# Tests workspace complets — certains crates sont lents/temps réel
# (avid-security, turboquant, neural_clinical_console, etc.)
cargo test --workspace --lib
```

## Limites connues

- Deux binaires coexistent ; `soulsystem --entity` unifie le runtime, `souls` reste le binaire canonique minimal.
- Tous les canaux principaux (Telegram, Discord, Slack, WhatsApp) sont implémentés en mode webhook ou long-poll.
- `soul_agent_core` est utilisé pour `ask`, pas encore pour `run_cycle`.
- Les 104 crates orphelins nécessitent un plan d'intégration ou d'exclusion progressive.
