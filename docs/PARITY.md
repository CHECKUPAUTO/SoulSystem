# Matrice de parité — SoulSystem vs OpenClaw vs Hermes-Agent

**Date :** 2026-06-16  
**Objectif :** suivre objectivement l'écart entre SoulSystem et ses deux concurrents directs.

## Légende

| Symbole | Signification |
|---|---|
| ✅ | Disponible et fonctionnel |
| ⚠️ | Partiel / prototype / non câblé par défaut |
| ❌ | Non disponible |

## Comparaison

| Capacité | OpenClaw | Hermes-Agent | SoulSystem |
|---|---|---|---|
| **Runtime** | Node.js | Python | ✅ Rust |
| **Assistant personnel local** | ✅ | ✅ | ⚠️ (`souls` CLI) |
| **Gateway HTTP/WS** | ✅ | ✅ | ✅ `soul_gateway` |
| **Telegram** | ✅ | ✅ | ✅ (depuis 2026-06-16) |
| **WhatsApp** | ✅ | ✅ | ❌ |
| **Discord** | ✅ | ✅ | ❌ |
| **Slack/Signal/iMessage** | ✅ | ⚠️ | ❌ |
| **Multi-provider LLM** | ✅ | ✅ | ✅ Ollama/OpenAI/Anthropic |
| **Tool calling natif** | ✅ | ✅ | ✅ `soul_tools` |
| **Sandboxing** | ✅ Docker/SSH/Modal | ✅ 5 backends | ⚠️ whitelist + timeout |
| **ReAct loop** | ✅ | ✅ | ✅ `soul_agent_core` |
| **Mémoire persistante** | ✅ MEMORY.md | ✅ FTS5 + summary | ✅ `soul_persistence` Sled |
| **Auto-création de skills** | ✅ | ✅ | ⚠️ `soul-skills` + `soul_openclaw` |
| **Skill self-improvement** | ✅ hot-reload | ✅ during use | ⚠️ non validé en prod |
| **Subagents** | ✅ | ✅ | ⚠️ `soul-subagents` non câblé |
| **Cron / scheduling** | ✅ | ✅ | ❌ `soul_scheduler` orphelin |
| **Onboarding one-liner** | ✅ `openclaw onboard` | ✅ curl \| bash | ❌ |
| **Migration OpenClaw** | N/A | ✅ `hermes claw migrate` | ❌ |
| **TUI riche** | ✅ apps | ✅ `hermes` TUI | ⚠️ `soul_repl` basique |
| **Code signing / BoundSystem** | ❌ | ❌ | ✅ |
| **Empirical validation gate** | ❌ | ❌ | ⚠️ concept `soul-rsi` |

## Écarts prioritaires à combler

1. **Canaux de messagerie** : WhatsApp, Discord, Slack.
2. **Onboarding** : un setup wizard pour configurer LLM + mémoire + gateway.
3. **Cron** : activer `soul_scheduler` dans `soul_entity`.
4. **Subagents** : câbler `soul-subagents` à `SoulEntity`.
5. **Skill validation gate** : valider chaque skill induite par un test avant archive.

## Avantages différentiateurs de SoulSystem

- **Rust natif** : performance, sécurité mémoire, binaire autonome.
- **BoundSystem + seccomp + code signing** : garde-fous techniques.
- **Architecture modulaire** : 171 crates prêts à être assemblés.
- **Pipeline de validation empirique** : possible via `soul-rsi` / `soul-automodify`.
