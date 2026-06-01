# SoulNeural — Système 3 Cognitif 🧠

[![CI](https://github.com/CHECKUPAUTO/SoulNeural/actions/workflows/ci.yml/badge.svg)](https://github.com/CHECKUPAUTO/SoulNeural/actions/workflows/ci.yml)

**SoulNeural** est la nouvelle génération du système cognitif de l'écosystème CHECKUPAUTO. Il intègre un réseau neuronal hamiltonien (HNN), un MiniLLM local, une mémoire vectorielle HNSW et des boucles d'apprentissage DPO.

## Architecture (15 crates)

```
soul-monitor        → Interface HTTP/JSON
soul-cortex         → Fusion HNN-LLM, biais symplectique
soul-identity       → Mémoire narrative HNSW, self-model, goals
soul-learner        → Entraînement DPO, scénarios
soul-memory-store   → VectorStore HNSW persistant
soul-minillm        → MiniLLM + LoRA + DPO
soul-hnn            → Hamiltonian Neural Network
soul-snn            → Réseau impulsionnel + décodeur d'intentions
soul-tools          → Orchestrateur d'outils avec recettes
soul-embed          → Embedder thread-safe
soul-symbolic       → Moteur symbolique
soul-pattern-miner  → Fouille de patterns
soul-rstdp          → Optimiseur neuromorphique R-STDP
soul-quant          → Quantization-aware training
soul-core           → Traits partagés (CognitiveState, Critic, Tool, Goal...)
```

## Stats

- **15 crates**, **3545+ lignes** de Rust
- **42 tests**, 0 échec
- Dépend de `scirust-core`, `scirust-autodiff`, `scirust-simd`
- Compilé en `release` (LTO, panic=abort, strip)

## Démarrage rapide

```bash
git clone https://github.com/CHECKUPAUTO/SoulNeural.git
cd SoulNeural
cargo build --release
cargo test --release
```

## Les 3 piliers Système 3

| Pilier | Crate | Fonction |
|--------|-------|----------|
| 🔄 Biais symplectique | `soul-cortex` | Modulation des logits LLM par l'état HNN |
| 📊 DPO réel | `soul-minillm` | Apprentissage par préférences avec LoRA |
| 🗄️ Mémoire HNSW | `soul-memory-store` | Recherche vectorielle persistante <10ms |

## Licence

MIT
