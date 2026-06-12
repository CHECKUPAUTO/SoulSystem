# SoulSystem Audit Baseline

**Date :** 2026-06-12
**Scope :** Workspace Rust `soul_system` (39 crates, ~13000 LoC)

---

## 1. Métriques Globales

| Métrique | Valeur |
|---|---|
| Crates workspace | 39 |
| Binaires | 4 (`souls`, `soul_kernel`, `soul_system_bin`, `soul_repl`) |
| Fichiers `.rs` (hors target/turbovec) | 94 |
| LoC Rust (hors target/turbovec) | 12 978 |
| Tests unitaires | ~76 (soul_entity: 20, soul_sandbox: 33, autres: ~23) |
| Build clean (dev) | ~33s |
| Clippy warnings (`-D warnings`) | 1+ (soul_surgery: needless_range_loop) |

---

## 2. Fichiers les Plus Grands (>500 lignes = "god files")

| Fichier | LoC | Risque |
|---|---|---|
| `soul_sandbox/src/lib.rs` | 1 255 | 🔴 God object — à décomposer |
| `soul_entity/src/entity.rs` | 613 | 🔴 God object — à décomposer |
| `soul_entity/src/subsystems.rs` | 554 | 🔴 God object — à décomposer |
| `soul_openclaw/src/lib.rs` | 535 | 🔴 God object — à décomposer |
| `soul_orchestrator/src/orchestrator.rs` | 501 | 🔴 God object — à décomposer |
| `soul_gateway/src/lib.rs` | 448 | 🟡 Approche god object |
| `soul_scheduler/tests/scheduler_tests.rs` | 415 | 🟡 Tests volumineux |
| `soul_repl/src/lib.rs` | 399 | 🟡 Approche god object |

**Objectif :** Aucun fichier > 500 lignes. Cible idéale : 200-300 lignes.

---

## 3. Dépendances Circulaires

| Cycle | Gravité |
|---|---|
| `souls` → `soul_entity` → `souls` | 🔴 Critique |

**Impact :** Compilation plus lente, couplage excessif, risque de deadlock.

---

## 4. Dépendances Inutilisées (cargo-machete)

| Crate | Dépendances inutilisées |
|---|---|
| `soul_openclaw` | `async-trait` |
| `soul_planner` | `serde_json` |
| `neural_metacognition` | `parking_lot` |
| `soul_tools` | `serde_json` |
| `soul_orchestrator` | `soul_telemetry` |
| `soul_entity` | `soul_agent_runtime`, `thiserror` |
| `soul_telemetry` | `libc`, `tokio` |
| `soul_storage` | `soul_scheduler` |
| `scirust_affective_core` | `crossbeam-utils` |
| `neural_clinical_console` | `parking_lot` |
| `ecosystem_synapse_linker` | `arc-swap` |
| `souls` | `soul_persistence`, `soul_planner`, `soul_tools`, `tower-http` |

---

## 5. Gestion d'Erreur (unwrap/expect)

| Emplacement | Type | Gravité |
|---|---|---|
| `soul_entity/src/entity.rs:71` | `.unwrap()` | 🟡 |
| `soul_entity/src/entity.rs:231` | `.unwrap()` | 🔴 |
| `soul_orchestrator/src/orchestrator.rs:30-65` | `.unwrap()` (8×) | 🟡 |
| `soul_journal/src/rotation.rs` | `.unwrap()` (×) | 🟡 |
| Tests (×20+) | `.unwrap()` / `.expect()` | 🟢 |

**Objectif :** 0 unwrap/expect en code production. Tout_Result_ propagé.

---

## 6. Panic! en Production

| Fichier | Ligne | Description |
|---|---|---|
| `soul_orchestrator/src/orchestrator.rs:466` | `panic!("dispatch inattendu")` | 🔴 Crash évitable |

---

## 7. Coverage (llvm-cov)

| Crate | Coverage (lignes) |
|---|---|
| `soul_sandbox` | 54.73% |
| `soul_entity` | 72.61% |
| `soul_persistence` | 67.19% |
| **Moyenne** | ~65% |

**Objectif :** > 80% coverage sur tous les crates critiques.

---

## 8. Warnings Compilateur

| Crate | Warning |
|---|---|
| `test_telemetry` | unused imports: `PrometheusExporter`, `TelemetryHub`, `gather_metrics` |
| `soul_surgery` (clippy) | needless_range_loop |

---

## 9. Architecture Actuelle

```
souls (binaire unifié)
  → soul_entity (agrégat central)
    → soul_llm, soul_planner, soul_tools, soul_sandbox, soul_persistence
    → soul_openclaw, soul_gateway, soul_journal, soul_forge
    → 12 subsystems historiques (neural_*, ontological_*, ecosystem_*)
```

**Problème :** `soul_entity` dépend de `souls` (cycle) via le binaire.

---

## 10. Priorités de Correction

| Priorité | Action | Impact |
|---|---|---|
| 🔴 P1 | Corriger cycle `souls` ↔ `soul_entity` | Fiabilité |
| 🔴 P1 | Remplacer `panic!` par `Result` | Crash prevention |
| 🔴 P1 | Décomposer `soul_sandbox` (1255 LoC) | Maintenabilité |
| 🔴 P1 | Décomposer `soul_entity/entity.rs` (613 LoC) | Maintenabilité |
| 🟠 P2 | Nettoyer deps inutilisées (11 crates) | Build perf |
| 🟠 P2 | Remplacer unwrap/expect production | Fiabilité |
| 🟠 P2 | Atteindre 80% coverage | Qualité |
| 🟡 P3 | Clippy warnings | Code quality |
| 🟡 P3 | Documentation | Maintenabilité |

---

*Baseline générée le 2026-06-12 par analyse automatique du workspace.*
