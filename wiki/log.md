# Wiki Log

_Append-only chronological record of wiki operations._

## [2026-04-13] init | LLM-Wiki Created
- Created wiki directory structure (raw/, entities/, concepts/, synthesis/)
- Bootstrapped from existing knowledge: MEMORY.md, TOOLS.md
- Ingested Karpathy llm-wiki gist as concept page
- Initial 4 entity pages, 4 concept pages, 1 synthesis page

## [2026-04-13] ingest | Karpathy llm-wiki pattern
- Source: https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f
- Created concept page: concepts/llm-wiki-pattern.md
- Updated index.md with new concept entry
- Key insight: wiki is a persistent compounding artifact, not re-derived per query

## [2026-04-13] ingest | Provider registration issue
- Source: OpenClaw onboard/configure wizard investigation
- Created concept page: concepts/provider-registration.md
- Found: 3 stale plugin entries (serve, onboard, doctor) causing warnings
- Found: pi-tools.before-tool-call.runtime module missing (blocks exec)
- Action: config fix needed (remove stale plugins), npm reinstall needed

## [2026-04-13] queued | Karpathy gists exploration
- Source: https://gist.github.com/karpathy
- Identified gists of interest:
  - **microgpt** (8627fe) — GPT atomique en Python pur, 0 dépendance — pertinent pour comprendre les fondamentaux
  - **build-microgpt** (561ac2) — Construction progressive de microgpt couche par couche
  - **llm-wiki** (442a6b) — Déjà ingéré ✅
  - **pg-pong** (a4166c) — RL Policy Gradients sur Pong — pattern RL applicable
  - **min-char-rnn** (d4dee5) — RNN char-level minimal — éducatif
  - **batched LSTM** (587454) — LSTM vectorisé NumPy — pourrait inspirer SoulLink
  - **google-slides-css** (887015) — Hack CSS — pas pertinent
## [2026-04-13] fix | SoulLink Nodes + Orchestrateur reconnectés
- **Problème**: Orchestrateur voyait 0/6 nodes (API mismatch)
- **Cause 1**: Binaire prod `/opt/soullink-node` était un répertoire, pas un fichier — `ExecStart` pointait vers le dossier
- **Cause 2**: L'ancien binaire servait du HTML sur `/api/stats` au lieu du JSON attendu par l'orchestrateur
- **Fix**: Recompilé soullink-node depuis workspace (endpoint JSON `/api/stats`), déployé dans `/opt/soullink-node/soullink-node`, corrigé les 6 unit files systemd
- **Résultat**: 6/6 nodes ONLINE, orchestrateur les voit correctement
- **dream_cleaner**: Désactivé (legacy V10, importait `brain_v10_config` inexistant, crashait en boucle #2898)
- **Cleanup Rust**: 4 stubs supprimés (sensory, bridge, analyzer, logic), 4 binaires v1 + 2 symlinks cassés supprimés
- **Config**: openai supprimé partout (plugins, auth, memorySearch), chmod 600 openclaw.json
- **Disque**: 3,3 Go Rust targets + 11 Mo brain_state + 10 Mo misc supprimés (workspace 5,2Go → 1,9Go)

## [2026-04-13] ingest | Karpathy gists — exploration complète
- Sources: 6 gists analysés (microgpt, build-microgpt, pg-pong, batched LSTM, min-char-rnn, llm-wiki)
- Créé: concepts/karpathy-gists.md avec analyse détaillée
- Points clés pour nos projets:
  - **pg-pong**: Pattern RL Policy Gradient → applicable au reinforcement de SoulLink
  - **batched LSTM**: Vectorisation numpy → remplacement boucles Python
  - **microgpt**: Autograd minimal (Value class) → moteur de différentiation custom
  - **min-char-rnn**: BPTT minimal → base théorique réseaux récurrents
  - **build-microgpt**: Construction par couches → documentation pédagogique
- Mis à jour index.md avec nouvelle page concept + 4 sources
## [2026-04-14] ingest | OpenEvolve Auto-Apply — Night Cycle Consolidation

Processed 9 night cycle reports (00:00 through 05:01). Created 3 new reference documents and 3 wiki concept stubs:

**New references:**
- `neural_organ_detailed_specs_v2.md` — Consolidated organ specs: Memory (P0), Reflex, Integration/Synthesis, Affect, plus soullink-server-core shared lib, stimulus pipeline, turbulence regulation, attractor seeding, synaptic plasticity, performance & security
- `openclaw_rust_migration_roadmap.md` — Phased TS→Rust migration: Phase 1 pure-function modules (config, cron, security), Phase 2 I/O-bound (gateway, web-fetch, context-engine), unified workspace plan
- `neural_mesh_stimulus_pipeline.md` — Solving mesh dormancy (Hz=0.0): stimulus routing, cross-node resonance, turbulence cascade, sleep consolidation, node binary unification

**New wiki concepts:**
- neural-organ-specs, openclaw-rust-roadmap, neural-stimulus-pipeline

**Skipped (requires approval):**
- Kill sl13-mod-evolve.py (Python, ~17M RAM) — Rust binary exists
- Disable allowInsecureAuth=true in gateway config
- Gateway restart (WS 1006 fix)
- All organ Rust implementations (Memory, Reflex, Integration)
- Python→Rust ports (decision_engine, market_injector, reinforcement_critic)
- Attractor seeding, turbulence cascade, synaptic plasticity code
