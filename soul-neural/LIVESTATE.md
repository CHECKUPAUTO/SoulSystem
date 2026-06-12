# LIVESTATE — SoulNeural (intégré dans SoulSystem)

> Fichier de bord partagé entre agents.
> Dernière mise à jour : 2026-06-01 11:02 CEST

## Statut d'intégration
- **Intégré dans SoulSystem** sous `/root/SoulSystem/soul-neural/`
- Clone depuis GitHub (`CHECKUPAUTO/SoulNeural`)
- Workspace indépendant — pas de conflit avec workspace racine de SoulSystem
- Dépendances scirust : paths absolus `/root/scirust/scirust-*`

## HEAD
- **Hash:** `cdc40fe`
- **Message:** Implement Soul_Spike and optimize CI
- **Auteur:** google-labs-jules[bot]
- **Date:** 2026-05-24 05:50:25 +0000

## Compilation
- `cargo check --release` ✅ 0 erreurs, 0 warnings
- `cargo test --release` ✅ 51/51 tests verts

## Crates (15)
```
soul-core          → Traits partagés
soul-hnn           → Hamiltonian Neural Network
soul-cortex        → Fusion HNN-LLM, biais symplectique
soul-minillm       → MiniLLM + LoRA + DPO
soul-snn           → Spiking Neural Network
soul-embed         → Embedder thread-safe
soul-symbolic      → Moteur symbolique
soul-pattern-miner → Fouille de patterns
soul-rstdp         → Optimiseur R-STDP
soul-identity      → Mémoire narrative HNSW, self-model, goals
soul-memory-store  → VectorStore HNSW persistant
soul-tools         → Orchestrateur d'outils
soul-learner       → Entraînement DPO
soul-monitor       → Interface HTTP/JSON
soul-quant         → Quantization-aware training
```

## Prochains commits (non pushés)
```
(aucun)
```