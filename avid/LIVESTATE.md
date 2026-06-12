# LIVESTATE — AVID (intégré dans SoulSystem)

> Dernière mise à jour : 2026-06-01 16:04 CEST

## Statut d'intégration
- **Intégré dans SoulSystem** sous `/root/SoulSystem/avid/`
- Clone depuis GitHub (`CHECKUPAUTO/AVID`), HEAD `1eb5820`
- Workspace indépendant (19 crates cœur + 5 exclus)
- `cargo check` ✅ 0 erreurs

## Crates (24 total, 19 cœur + 5 exclus)
```
CŒUR:
avid-core         → Traits, types, config partagés
avid-cortex       → LLM cortex
avid-mimic        → API mimic
avid-scout        → Web explorer
avid-vision       → Vision/perception
avid-skills       → Skills registry
avid-tokenjuice   → Token optimization
avid-model-router → LLM routing
avid-knowledge-graph → KG
avid-critic       → Critique engine
avid-forge        → Code forgeron
avid-orchestrator → Orchestrateur
avid-server       → API HTTP
avid-tui          → Terminal UI
avid-cli          → CLI
avid-sandbox      → Sandbox exécution
avid-anticlone    → Anti-clonage
avid-k8s          → Kubernetes
avid-db           → Database

EXCLUS:
avid-cobalt       → Cobalt integration
avid-gomogo       → Go/mogo
avid-hnn          → HNN (optionnel autodiff)
avid-intel        → Intel
avid-security     → Security
```

## HEAD
- **Hash:** `1eb5820`
- **Message:** feat(backend): Chantier B — compute_backend hardening
- **Auteur:** Jules

## Compilation
- `cargo check` (cœur) ✅ 0 erreurs