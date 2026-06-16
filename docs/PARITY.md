# Matrice de parité — SoulSystem vs OpenClaw vs Hermes-Agent

**Date :** 2026-06-16  
**Objectif :** suivre objectivement l'écart entre SoulSystem et ses deux concurrents directs, et piloter le plan de rattrapage.

## Légende

| Symbole | Signification |
|---|---|
| ✅ | Disponible et fonctionnel dans le runtime par défaut |
| ⚠️ | Partiel / prototype / câblé mais non unifié / non validé en prod |
| ❌ | Non disponible |

## Comparaison

| Capacité | OpenClaw | Hermes-Agent | SoulSystem |
|---|---|---|---|
| **Runtime** | Node.js | Python | ✅ Rust monobinaire |
| **Assistant personnel local** | ✅ | ✅ | ✅ `souls` CLI + `soulsystem --repl` |
| **Gateway HTTP/WS** | ✅ | ✅ | ✅ `soul_gateway` (`/v1/*` + WS `/v1/stream`) |
| **Telegram** | ✅ | ✅ | ✅ inbound + outbound (long-poll) |
| **WhatsApp** | ✅ | ✅ | ✅ webhook-based (Business API) |
| **Discord** | ✅ | ✅ | ✅ webhook-based + REST send |
| **Slack** | ✅ | ⚠️ | ✅ webhook-based + REST send |
| **Signal/iMessage** | ✅ | ⚠️ | ❌ |
| **Multi-provider LLM** | ✅ | ✅ | ✅ Ollama/OpenAI/Anthropic via `LlmClient` |
| **Tool calling natif** | ✅ | ✅ | ⚠️ `soul_tools` dispatch, mais les outils sont aplatis en prompt pour `LlmClient` ; seul `soul_llm::chat` Ollama fait du vrai tool-calling |
| **Sandboxing** | ✅ Docker/SSH/Modal | ✅ 5 backends | ⚠️ whitelist + timeout + seccomp-BPF ; pas de backend container/VM |
| **ReAct loop** | ✅ | ✅ | ✅ `soul_agent_core::AutonomousAgent` (mémoire hiérarchique, KG, métacognition, critique) |
| **Mémoire persistante** | ✅ MEMORY.md | ✅ FTS5 + summary | ⚠️ deux systèmes coexistent : `soul_persistence::LongTermMemory` dans `SoulEntity` et `soullink-memory-hierarchy::HierarchicalMemory` dans `AutonomousAgent` |
| **Auto-création de skills** | ✅ | ✅ | ⚠️ `soul-skills::ValidatedSkillLibrary` + cristallisation LLM dans `AutonomousAgent`, mais pas de validation empirique automatique en prod |
| **Skill self-improvement** | ✅ hot-reload | ✅ during use | ⚠️ induction + validation structurelle, pas de boucle fermée en prod |
| **Subagents** | ✅ | ✅ | ⚠️ `soul-subagents::SubAgentManager` existe mais n'est câblé que comme worker LLM simple dans `soul-daemon` ; pas exposé dans `SoulEntity` |
| **Cron / scheduling** | ✅ | ✅ | ❌ `soul_scheduler` est un ordonnanceur CPU work-stealing, pas un cron ; les tâches périodiques sont commentées dans `src/main.rs` |
| **Onboarding wizard** | ✅ `openclaw onboard` | ✅ curl \| bash | ✅ `soulsystem --setup` / `--setup-tui` (ratatui) |
| **Migration OpenClaw** | N/A | ✅ `hermes claw migrate` | ❌ |
| **TUI riche** | ✅ apps | ✅ `hermes` TUI | ✅ `soulsystem --setup-tui` (ratatui) |
| **Code signing / BoundSystem** | ❌ | ❌ | ✅ |
| **MCP protocol** | ✅ | ⚠️ | ❌ `soul-mcp` est un client/serveur JSON-RPC sur channel, non branché aux outils |
| **Browser / web exploration** | ✅ | ✅ | ⚠️ `soul-browser` (CDP) et `soul-webfetch` existent mais ne sont pas utilisés par l'entité |
| **Empirical validation gate** | ❌ | ❌ | ⚠️ concept `soul-rsi` / `soul-automodify` ; pas en boucle de prod |

## Écarts prioritaires à combler (plan d'action)

1. **Unifier la mémoire** — `SoulEntity` et `AutonomousAgent` doivent partager `HierarchicalMemory` ; `LongTermMemory` reste un archive JSON structurée.
2. **Cron / scheduling** — transformer `soul_scheduler` en scheduler de tâches temporelles, ou créer un nouveau crate `soul-cron`, et câbler les objectifs périodiques.
3. **Tool calling multi-provider** — étendre `LlmClient` et les providers pour supporter nativement `tools` et parser les `tool_calls` (pas seulement Ollama).
4. **Subagents** — exposer `SubAgentManager` dans `SoulEntity` et permettre au ReAct loop de décomposer les goals complexes.
5. **Sandbox backends** — ajouter un backend containerisé (bubblewrap chroot ou podman/Docker) en complément du seccomp.
6. **Canaux de messagerie** — Signal / iMessage (WhatsApp, Discord, Slack déjà implémentés).
7. **MCP integration** — brancher `soul-mcp` comme source/destination d'outils pour `soul_tools`.
8. **Browser/webfetch** — intégrer `soul-browser` + `soul-webfetch` comme outils de l'agent.
9. **Migration OpenClaw** — importer une config OpenClaw existante (modèle, skills, MEMORY.md) vers SoulSystem.
10. **Validation gate** — valider chaque skill cristallisée par un test avant archive.

## Avantages différentiateurs de SoulSystem

- **Rust natif** : performance, sécurité mémoire, binaire autonome.
- **BoundSystem + seccomp + code signing** : garde-fous techniques intégrés.
- **Architecture modulaire** : ~173 crates permettant une composition fine.
- **Pipeline de validation empirique** : fondation `soul-rsi` / `soul-automodify` pour une future boucle fermée.

## Derniers jalons

- 2026-06-16 : merge PR #53 — TUI wizard, providers Slack/WhatsApp, docs, `main` unique.
- 2026-06-16 : `cargo check --workspace` et `cargo test --workspace --lib` verts.
