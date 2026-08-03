# Rapport d'Audit — Soul System (OS-AGENTS)

**Date :** 2026-06-08  
**Scope :** L'intégralité du workspace Rust `soul_system` (27 crates, ~15 000 LoC)  
**Note :** Le dépôt ne contient aucune référence à "soulsystem". L'audit porte sur le projet réel : **Soul System / OS-AGENTS**.

---

## Résumé Exécutif

Soul System est un framework Rust multi-agents cognitifs structuré en deux sous-systèmes quasi disjoints : un noyau runtime (ordonnancement, GEMM, IPC, stockage) et un subsystem cognitif (affectivité, pare-feu sémantique, métacognition). Le code est de qualité globalement élevée — lock-free data structures, tests de concurrence, architecture bien découpée. Cependant, l'audit révèle **1 stub critique**, **2 placeholders fonctionnels**, **3 bugs potentiels de crash**, **2 crates orphelins** jamais câblés, et **1 composant construit mais jamais invoqué**. La résolution de ces problèmes renforcera la fiabilité et complétera l'intégration des sous-systèmes.

---

## Cartographie de l'Écosystème

```
┌─────────────────────────────────────────────────────────┐
│                    soul_kernel (BIN)                     │
│  soul_scheduler ─► soul_telemetry                       │
│  soul_matrix_engine ─► soul_scheduler                   │
│  soul_ipc (hub lock-free MPMC)                          │
│  soul_cortex ─► soul_matrix_engine                      │
│  soul_storage ─► soul_scheduler                         │
│  soul_orchestrator ─► soul_ipc                          │
│  soul_agent_runtime ─► scheduler+matrix+storage+ipc+orch│
│  soul_perception ─► soul_ipc                            │
│  soul_cluster ─► soul_ipc (UDP)                         │
│  soul_journal (mmap WAL)                                │
│  soul_guard (integrity)                                 │
│  soul_surgery (RepE steering)                           │
│  soul_scout (SearXNG TCP)                               │
│  soul_evolution (hot-swap .so)                          │
│  soul_forge (evolutionary genome)                       │
│  soul_acoustic (VAD) ← ORPHELIN                         │
│  soul_attention (KV-cache) ← ORPHELIN                   │
├─────────────────────────────────────────────────────────┤
│                 soul_system_bin (BIN)                    │
│  scirust_affective_core (PAD 3D)                        │
│  semantic_neuromodulator (PAD→chemical)                 │
│  semantic_firewall (cosine blocking)                    │
│  neural_metacognition (telemetry ring)                  │
│  neural_clinical_console (server :8080)                 │
│  neural_graph_compiler (DAG Kahn)                       │
│  neural_chaos_monkey (fault injection)                  │
│  neural_cluster_sync (CRDT merge-max)                   │
│  ontological_self_healing (NaN/inf repair)              │
│  ecosystem_synapse_linker (routing table)               │
├─────────────────────────────────────────────────────────┤
│  turbovec (submodule séparé, 8726 LoC)                  │
│  scirust-core (dépendance externe)                      │
└─────────────────────────────────────────────────────────┘
```

---

## Statistiques

| Métrique | Valeur |
|---|---|
| Crates workspace | 27 |
| Binaires | 2 (`soul_kernel`, `soul_system_bin`) |
| Fichiers `.rs` (hors target) | ~107 |
| LoC total (hors turbovec) | ~6 300 |
| LoC turbovec | ~8 700 |
| Tests unitaires | ~40 modules inline |
| Tests d'intégration | 15 fichiers (turbovec) + 1 (scheduler) |
| Benchmarks | 7 (scheduler) |

---

## Problèmes Détectés

### 🔴 CRITIQUE

| # | Catégorie | Fichier | Ligne | Description |
|---|---|---|---|---|
| C1 | **Stub** | `scirust_affective_core/src/affect/autograd_hook.rs` | 5-9 | `backpropagate_emotional_tension` retourne un vecteur de zéros fictifs. La méthode est un no-op complet — le gradient émotionnel n'est jamais calculé. |
| C2 | **Placeholder** | `soul_agent_runtime/src/runtime.rs` | 43-44 | `fake_query` est un vecteur zéro avec uniquement le `signal_code` injecté. Les résultats KNN ne sont pas significatifs. |

### 🟠 MAJEUR

| # | Catégorie | Fichier | Ligne | Description |
|---|---|---|---|---|
| M1 | **Stub** | `soul_forge/src/lib.rs` | 30-38 | `evaluate_and_mutate` utilise des métriques hardcodées (`total_tasks=0.0`, `total_cycles=1.0`) au lieu de lire le `TelemetryHub` réel. L'optimisation génétique est non-fonctionnelle. |
| M2 | **Bug** | `soul_journal/src/rotation.rs` | 45,49,67,78,88 | 5 appels `RwLock::unwrap()` qui panicent en cas de poisoning (si un thread panique en tenant le lock write). |
| M3 | **Bug** | `soul_kernel/src/main.rs` | 77,86 | `expect()` sur `ClusterNode::bind` et `transmit_remote` — un échec réseau crash le kernel entier. |
| M4 | **Bug** | `soul_system_bin/src/main.rs` | 63 | `expect()` sur `signal(SignalKind::interrupt())` — panic si le setup signal échoue. |
| M5 | **Orphelin** | `soul_acoustic` | — | Crate VAD (voice activity detection) complet mais jamais câblé dans aucun binaire. |
| M6 | **Orphelin** | `soul_attention` | — | Crate KV-cache avec attention sinks (StreamingLLM) complet mais jamais câblé. |
| M7 | **Dead code** | `soul_agent_runtime` | — | Crate construit comme dépendance de `soul_kernel` mais jamais invoqué dans `main.rs`. |

### 🟡 MINEUR

| # | Catégorie | Fichier | Ligne | Description |
|---|---|---|---|---|
| m1 | Code pauvre | `soul_forge/src/lib.rs` | 45-46 | Mutation génétique naive (swap fixe 32↔64↔16, +25 fixe) — pas de diversification. |
| m2 | Documentation | `ARCHITECTURE.md` | — | 21/27 crates sans doc-comment ; rôles inférés du nom. |
| m3 | Tests | `soul_journal` | — | Tests écrivent dans `/tmp/` — potentiel conflit en parallèle CI. |

---

## Code Pauvre et Améliorations Architecturales

### 1. Couplage nul entre les deux sous-systèmes
Les deux binaires (`soul_kernel` et `soul_system_bin`) ne partagent aucune arête. L'affectivité, la neuromodulation et le pare-feu sémantique du subsystem cognitif n'influencent jamais l'ordonnancement, le GEMM ou l'IPC du runtime. **Recommandation :** câbler `soul_acoustic` et `soul_attention` dans le pipeline cognitif, et ajouter un canal de feedback affectif → scheduler.

### 2. `soul_forge` isolé
Le module d'évolution génétique ne reçoit aucune télémétrie réelle et ne peut pas optimiser les paramètres du scheduler. **Recommandation :** exposer des accesseurs publics dans `TelemetryHub` pour les métriques agrégées.

### 3. Absence de gestion d'erreur structurée
Plusieurs composants utilisent `unwrap()`/`expect()` là où un `Result` serait plus sûr (journal rotation, network bind). **Recommandation :** propager les erreurs avec `Result` ou utiliser des strategies de récupération (retry, fallback).

### 4. `soul_scout` synchronisme bloquant
Le client SearXNG est synchrone et bloquant — un timeout long bloquerait le thread appelant. **Recommandation :** ajouter un timeout configurable ou migrer vers async.

---

## Conclusions et Priorisation

### Priorité 1 (Corriger maintenant)
1. **C1** — Implémenter `backpropagate_emotional_tension` avec un gradient réel
2. **C2** — Remplacer `fake_query` par un encodage fonctionnel du signal
3. **M1** — Brancher `soul_forge` sur la télémétrie réelle
4. **M2** — Remplacer `unwrap()` par `unwrap_or_else` / gestion propre dans `rotation.rs`
5. **M3/M4** — Remplacer `expect()` par des fallbacks gracieux

### Priorité 2 (Câbler les orphelins)
6. **M5** — Intégrer `soul_acoustic` dans le pipeline de perception
7. **M6** — Intégrer `soul_attention` dans le cortex récurrent
8. **M7** — Activer `soul_agent_runtime` dans `soul_kernel`

### Priorité 3 (Évolution)
9. Nouvelles fonctionnalités : health check endpoint, métriques agrégées, config file, logging structuré
10. Évolutions : enrichir `soul_forge`, améliorer `soul_scout`

---

*Rapport généré par audit automatisé — 2026-06-08*
