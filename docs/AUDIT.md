# SoulSystem — Audit de parité et plan de rattrapage (juin 2026)

Document construit à partir d'une revue manuelle du code et d'une compilation réelle du workspace.

## 1. Santé du build

| Commande | Résultat |
|---|---|
| `cargo check --workspace` | ✅ vert |
| `cargo test --workspace --lib` | ✅ vert, aucun échec |
| Branches distantes | `main` seule |

Crate exclue du workspace racine : `os-agents/` (duplicate sub-workspace).

## 2. Résumé des forces

- **Runtime unifié** : `soulsystem --entity` lance `SoulEntity` + gateway HTTP/WS + providers + REPL/autonomous loop dans un seul binaire.
- **Gateway** : `soul_gateway` expose `/v1/ask`, `/v1/plan`, `/v1/run`, `/v1/cycle`, `/v1/status`, `/v1/goals`, WS `/v1/stream`, plus les webhooks providers.
- **Providers** : Telegram (long-poll), Discord (webhook + REST), Slack (webhook), WhatsApp (webhook).
- **LLM** : `LlmClient` multi-provider (Ollama, OpenAI, Anthropic) avec budget par goal.
- **ReAct loop** : `soul_agent_core::AutonomousAgent` avec mémoire hiérarchique, graphe de connaissances, métacognition, compaction de contexte, critique (`soul_critique`) et cristallisation de skills.
- **Onboarding** : `soulsystem --setup` et `soulsystem --setup-tui` (ratatui/crossterm) persistent la config via `souls::config`.
- **Sécurité** : `BoundSystem`, seccomp-BPF, code signing, circuit breaker, backup signé.

## 3. Gaps prioritaires (avec preuves)

### P1 — Deux systèmes de mémoire non unifiés

- `SoulEntity` utilise `soul_persistence::LongTermMemory` (redb, entrées JSON typées `KIND_GOAL`, `KIND_PLAN`, etc.).
- `AutonomousAgent` utilise `soullink-memory-hierarchy::HierarchicalMemory` (working/episodic/semantic + consolidation).
- Conséquence : les apprentissages de l'agent ReAct ne remontent pas dans le gateway/daemon.
- Fichiers : `soul_entity/src/entity.rs:33`, `soul-agent-core/src/lib.rs:150`.

### P1 — Tool calling dégradé hors Ollama natif

- `soul_llm::legacy::OllamaClient::chat` ignore le paramètre `_tools` et aplatit la conversation en un seul prompt.
- `soul_llm::chat::OllamaClient` fait du vrai tool-calling, mais n'est pas le client utilisé par `AutonomousAgent`.
- Conséquence : `LlmClient` (multi-provider) ne supporte pas le ReAct tool-calling ; seul Ollama via `chat.rs` le fait.
- Fichiers : `soul_llm/src/legacy.rs:179`, `soul_llm/src/chat.rs:316`.

### P1 — Pas de cron scheduler opérationnel

- `soul_scheduler` est un ordonnanceur CPU work-stealing (`AgentScheduler::submit_to`), pas un cron.
- Le bloc cron dans `src/main.rs` est commenté avec la note : « current `soul_scheduler` is a CPU topology work-stealing scheduler, not a cron scheduler ».
- Conséquence : aucune goal périodique ni tâche planifiée.
- Fichiers : `src/main.rs:1390`, `soul_scheduler/src/scheduler.rs:101`.

### P2 — Subagents non câblés à l'entité

- `soul-subagents::SubAgentManager` existe mais n'est importé ni dans `soul_entity` ni dans `soulsystem`.
- Il est utilisé dans `soul-daemon::Daemon` comme worker LLM simple, sans décomposition réelle.
- Conséquence : pas de décomposition parallèle de goals dans le runtime principal.
- Fichiers : `soul-subagents/src/lib.rs`, `soul-daemon/src/lib.rs:15`.

### P2 — Sandbox sans backend container/VM

- `soul_sandbox` = whitelist + timeout + seccomp-BPF.
- Autres runtimes supportent Docker/SSH/Modal et 5 backends de sandbox.
- Conséquence : exécution toujours sur le host, même pour des commandes non sensibles.
- Fichiers : `soul_sandbox/src/policy.rs`, `soul_sandbox/src/lib.rs`.

### P2 — Signal / iMessage absents

- Seuls Telegram, Discord, Slack, WhatsApp sont implémentés.

### P3 — MCP non branché

- `soul-mcp` implémente un client/serveur JSON-RPC sur channel, mais n'est pas branché au registry `soul_tools`.
- Conséquence : pas d'intégration serveur MCP externe.
- Fichiers : `soul-mcp/src/lib.rs`.

### P3 — Browser / webfetch non utilisés par l'entité

- `soul-browser` a des types CDP mais pas d'automation active câblée.
- `soul-webfetch` existe mais n'est pas dans le tool registry.
- Conséquence : pas de navigation web dans la boucle ReAct.
- Fichiers : `soul-browser/src/lib.rs`, `soul-webfetch/src/lib.rs`.

## 4. Plan d'action proposé

1. **Unifier la mémoire** (P1) — remplacer `LongTermMemory` dans `SoulEntity` par `HierarchicalMemory` ; synchroniser les deux avec un `MemoryBridge`.
2. **Tool calling multi-provider** (P1) — ajouter `tools` à l'API `LlmProvider`, implémenter pour OpenAI/Anthropic/Ollama, et parser les `tool_calls` dans `LlmClient`.
3. **Cron scheduler** (P1) — créer un crate `soul-cron` avec expression parser et câbler `SoulEntity` pour créer des goals périodiques.
4. **Subagents dans `SoulEntity`** (P2) — exposer `SubAgentManager` et un outil `spawn_subagent`.
5. **Sandbox container backend** (P2) — ajouter un backend bubblewrap/podman optionnel.
6. **Signal/iMessage** (P2) — si API/liaison système disponible.
7. **MCP integration** (P3) — brancher `soul-mcp` à `soul_tools`.
8. **Browser/webfetch tools** (P3) — ajouter à `soul_tools` et au ReAct loop.
9. **Migration de configuration** (P3) — parser une config existante et importer modèle/tokens/MEMORY.md.
10. **Validation empirique des skills** (P3) — fermer la boucle `soul-rsi` pour valider chaque skill induite.

## 5. Métriques à suivre

- Taux de tests verts : `cargo test --workspace --lib`.
- Nombre de runtimes autonomes : objectif = 1 (`SoulEntity`), `AutonomousEntity` legacy à déprécier.
- Nombre de backends sandbox : objectif ≥ 2 (host seccomp + container).
- Nombre de providers tool-calling natifs : objectif ≥ 3.
