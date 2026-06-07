# Architecture — soul_system

État réel du workspace au commit `47dd1fb`, généré depuis le disque (non aspirationnel).
27 crates membres + 1 submodule (`turbovec`), ~4 070 lignes Rust, **2 binaires**.

## Vue d'ensemble

Le workspace se scinde en **deux sous-systèmes quasi disjoints**, chacun piloté par son binaire ; aucune arête ne les relie.

- **`soul_kernel`** (bin) — moitié *runtime/OS* : 13 crates `soul_*` (ordonnancement, GEMM, stockage, IPC, télémétrie…).
- **`soul_system_bin`** (bin) — moitié *cognitive* : 10 crates `neural_*` / `semantic_*` / `scirust_affective_core`, bâtis sur la dépendance externe `scirust`.

Deux crates ne sont atteints par **aucun** binaire (orphelins) : `soul_cluster`, `soul_perception`.

## Sous-système `soul_kernel`

Fondation (0 dép interne) : `soul_ipc`, `soul_telemetry`, `soul_guard`, `soul_journal`, `soul_scout`, `soul_surgery`.
`soul_scheduler` (sur `soul_telemetry`) porte l'essentiel ; `soul_matrix_engine` / `soul_storage` / `soul_evolution` en dépendent. `soul_agent_runtime` agrège scheduler+matrix+storage+ipc.

| crate | LoC | type | dépend de | rôle |
|---|---|---|---|---|
| soul_kernel | 71 | bin | (les 12 ci-dessous) | point d'entrée runtime |
| soul_scheduler | 796 | lib | soul_telemetry | ordonnancement — cœur (non documenté) |
| soul_matrix_engine | 517 | lib | soul_scheduler | noyau GEMM vectorisé SIMD, conscient des caches |
| soul_ipc | 346 | lib | — | IPC — fondation (4 dépendants) |
| soul_journal | 305 | lib | — | journal/WAL (non documenté) |
| soul_storage | 206 | lib | soul_scheduler | stockage (non documenté) |
| soul_telemetry | 163 | lib | — | télémétrie — fondation |
| soul_evolution | 118 | lib | soul_scheduler | (non documenté) |
| soul_guard | 117 | lib | — | (non documenté) |
| soul_surgery | 109 | lib | — | (non documenté) |
| soul_agent_runtime | 85 | lib | scheduler, matrix_engine, storage, ipc | (non documenté) |
| soul_cortex | 47 | lib | soul_matrix_engine | (non documenté) |
| soul_forge | 46 | lib | soul_telemetry | (non documenté) |
| soul_scout | 35 | lib | — | (non documenté) |

## Sous-système `soul_system_bin`

| crate | LoC | type | dépend de | rôle |
|---|---|---|---|---|
| soul_system_bin | 164 | bin | (les 10 ci-dessous) + scirust | point d'entrée cognitif |
| semantic_firewall | 147 | lib | scirust | pare-feu sémantique : bloque un vecteur si similarité cosinus > seuil |
| semantic_neuromodulator | 135 | lib | scirust, scirust_affective_core | (non documenté) |
| scirust_affective_core | 87 | lib | scirust (ext) | (non documenté) |
| neural_graph_compiler | 83 | lib | — | compilateur de graphe : tri topologique (Kahn) d'un DAG |
| neural_chaos_monkey | 83 | lib | — | injecteur de fautes déterministe (chaos engineering) |
| ecosystem_synapse_linker | 76 | lib | — | (non documenté) |
| neural_cluster_sync | 52 | lib | — | synchro inter-nœuds par fusion CRDT monotone (merge-max) |
| ontological_self_healing | 47 | lib | — | auto-réparation : détecte/répare les incohérences d'un état |
| neural_clinical_console | 45 | lib | neural_metacognition | (non documenté) |
| neural_metacognition | 43 | lib | — | (non documenté) |

## Orphelins (compilent, aucun binaire ne les utilise)

| crate | LoC | dépend de | décision |
|---|---|---|---|
| soul_cluster | 75 | soul_ipc | câbler dans `soul_kernel` ou retirer |
| soul_perception | 75 | soul_ipc | câbler ou retirer |

## Hors-workspace

- **`scirust`** — workspace externe (CHECKUPAUTO/scirust), socle des crates cognitifs.
- **`turbovec`** — submodule git (gitlink), sans `.gitmodules` : non résolu au clone.

## Limites

21/27 crates n'ont ni `description` ni doc d'en-tête ; leur rôle est inféré du nom + du graphe, pas d'une spec. Les 6 rôles en clair viennent du doc-comment réel. Ce fichier décrit l'état au commit `47dd1fb` et remplace toute description aspirationnelle antérieure.
