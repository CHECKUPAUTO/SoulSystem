# Changelog — OpenEvolve

Format : [Semantic Versioning](https://semver.org/lang/fr/)

---

## [4.3.0] — 2026-05-17

Wave D+E preview : scaffolding distribution multi-host (en attente du Jetson
Thor lundi), auto-PR sur repo cible, parser `// @evolve`, cache LLM, et
mutation rate adaptatif.

### Nouveaux modules

- **`cache.rs`** — `LlmCache` clé-SHA256 sur `(provider, model, temp_bucket,
  prompt)` avec TTL et éviction LRU approximative. Hit-rate exposé pour
  observability. Économies $$$ immédiates sur les prompts répétés (stagnation
  + feedback loops).
- **`distributed.rs`** — Scaffolding trait-based `MessageBus` + `LocalBus`
  in-memory + `DistributionManager`. Prêt pour adapter NATS / Redis Streams
  lundi avec le Thor sans toucher au reste du moteur.
- **`evolve_tag.rs`** — Parser de `// @evolve fitness=... iterations=...
  language=...` (Rust / Python / JS / Go). Détecte le nom de la fonction
  juste en dessous. Marche en single-file ou directory walk.

### Modifications

- **`auto_pr.rs`** — `create_pr_on_target(repo_dir, target_path, code,
  TargetPrReport)` : génération de PR sur un repo cible (pas openevolve
  lui-même). Parse `git remote get-url origin` pour discover owner/repo,
  gère SSH + HTTPS. Body Markdown riche avec table avant/après score,
  LOC, complexity_class, coût LLM par provider.
- **`mutation_engine.rs`** — `structured_rate` devient adaptatif :
  `note_outcome(was_structured, improved)` ajuste ±0.01 dans `[0.2, 0.95]`.
  Le moteur dérive vers la voie qui produit des gains.
- **`llm.rs`** — `LlmClient::with_cache(Arc<LlmCache>)` + `cache_stats()`.
  Le cache se branche sans changer la signature de `mutate_with_prompt`.
- **`config.rs`** — 2 nouvelles sections : `[cache]` (enabled / max_entries
  / ttl_secs) et `[distributed]` (host_id / publish_interval / poll_interval
  / max_inject).
- **`evolution.rs`** — Cache câblé dans `LlmClient`, adaptive rate appelé
  après chaque iteration via `mutator.note_outcome(...)`.
- **`main.rs`** — Nouvelle sous-commande `evolve-tag <path> [--json]` qui
  scanne un fichier ou dossier et liste les annotations trouvées.
- **`.github/workflows/evolve-on-tag.yml`** — Workflow GitHub Actions :
  scan `@evolve` au push de label `evolve`, lance une évolution par tag en
  matrix, upload best programs, post les résultats en commentaire de PR.

### Tests
- 78/78 unit tests OK (+18 vs v4.2)
- `cargo clippy --all-targets -- -D warnings` : clean
- `cargo fmt --check` : clean

### Reste pour v4.4 (vrai branchement distribué)
- Implémentation `NatsBus: MessageBus` (ou Redis Streams) selon broker choisi
- Branchement de `DistributionManager` dans la boucle `EvolutionEngine::run`
- Tests d'intégration avec broker dockerisé en CI

---

# Changelog — OpenEvolve

Format : [Semantic Versioning](https://semver.org/lang/fr/)

---

## [4.2.0] — 2026-05-17

Cette release ajoute la **persistance**, les **embeddings sémantiques**, le
**quality-diversity (Map-Elites)**, le **pre-filter nCPU**, l'**exporter
Prometheus**, le **suivi de coûts par provider**, et l'**AST crossover** réel.

### Nouveaux modules

- **`persistence.rs`** — backend SQLite (rusqlite + r2d2 pool, schéma migré
  idempotemment) avec 4 tables : `runs`, `programs` (indexé score / island /
  parent_id, blob embedding), `events` (stream d'itérations), `costs`
  (token in/out + USD par provider). Helpers : `create_run`, `insert_program`,
  `best_program`, `top_programs`, `programs_on_island`, `similar_programs`,
  `record_event`, `record_cost`, `run_total_cost`.
- **`wal.rs`** — Write-Ahead Log JSONL append-only avec fsync par ligne et
  recovery torn-write au chargement (s'arrête au premier JSON malformé).
- **`embedding.rs`** — Client HTTP `EmbeddingClient` compatible TRIBE
  (`POST /embed { texts } → { embeddings }`) avec fallback local
  feature-hash 256d L2-normalisé si l'endpoint est vide ou tombe. Cosine
  similarity inclus.
- **`map_elites.rs`** — Archive Quality-Diversity : `BehaviourKey` =
  (complexity_class × loc_bucket × fn_bucket), garde le meilleur programme
  par cellule. Sample uniforme pour diversifier les parents.
- **`ncpu_filter.rs`** — Client HTTP nCPU optionnel
  (`POST /classify { code, language } → { reject_score }`). Verdict en
  `Accept | Reject | Unknown` avec timeout par défaut 50 ms.
- **`metrics.rs`** — Façade Prometheus via `metrics` + `metrics-exporter-prometheus`.
  Counters / gauges / histogrammes pour iterations, mutations, repair,
  evaluations, best_score, population par île, breaker_open, durée
  d'itération, tokens in/out, USD micro-dollars. Inclut `CostTracker`
  in-process avec pricing OpenAI / Anthropic / Gemini (Ollama = $0).

### Modifications

- **`Program`** : `language: Option<String>` + `provenance: Option<String>`
  ajoutés en `#[serde(default)]` (compatible rétro v4.1, anciens JSON
  re-loadent sans erreur).
- **`Config`** : 5 nouvelles sections (`persistence`, `embedding`, `ncpu`,
  `map_elites`, `cost`) toutes en `#[serde(default)]`.
- **`evolution.rs`** : tous les nouveaux modules câblés dans la boucle :
  - nCPU pre-filter **avant** l'évaluation (skip + Prometheus tracking)
  - Embedding du child via TRIBE/local, stocké en blob SQLite
  - Insertion SQLite + WAL append après chaque itération réussie
  - Map-Elites archive maintenue en parallèle de NSGA-II
  - Cost guard avec abort gracieux sur `cost.max_usd`
  - Émission Prometheus pour chaque étape
- **`server.rs`** : 3 nouvelles routes :
  - `GET /islands` — résumé par île (size, best, mean) + push gauge Prometheus
  - `GET /costs` — snapshot CostTracker
  - `GET /metrics` — Prometheus exposition format
  - Dashboard HTML enrichi : panel USD, barres par île, breakdown coût
    par provider, lien `/metrics`.
- **`mutation_engine.rs`** : `ast_crossover(code_a, code_b, language, rng)`
  ajouté — vrai swap de subtree tree-sitter (function body de B inséré
  dans la frame de A).
- **`main.rs`** : 3 nouvelles sous-commandes :
  - `runs` — liste les runs SQLite avec stats (count, best, tokens, USD)
  - `replay-wal <path>` — replay déterministe d'un fichier WAL JSONL
  - `db-export <run-id>` — export top-N programmes en JSON

### Dépendances ajoutées

- `rusqlite 0.32` (bundled, pas de dep système)
- `r2d2 0.8` + `r2d2_sqlite 0.25`
- `metrics 0.23` + `metrics-exporter-prometheus 0.15` (default-features = false)
- `indexmap 2`

### Tests

- 60/60 unit tests OK (+19 vs v4.1)
- `cargo clippy --all-targets -- -D warnings` : clean
- `cargo fmt --check` : clean

### Migration depuis v4.1

Pas de breaking change explicite. La SQLite est créée automatiquement à
`openevolve_output/openevolve.db` au premier run. Si tu veux désactiver la
persistance, mets `persistence.enabled = false` dans ton TOML. Les JSON
sauvegardés en v4.1 (anciens programmes sans `language`/`provenance`) se
relisent sans erreur grâce à `#[serde(default)]`.

---

## [4.1.0] — 2026-05-16

Cette release transforme le précédent "merge qui compile mais ne câble rien"
en moteur réellement fonctionnel.

### Câblage (12 modules orphelins rebranchés)
- `analysis`, `mutation_engine`, `pareto`, `repair`, `transfer`,
  `semantic_diff`, `server`, `diagnostic`, `auto_pr`, `prompt_memory`,
  `fuzzer`, `providers` — tous appelés depuis la boucle ou les
  sous-commandes.

### Régressions corrigées
- `providers.rs` — support multi-provider complet
  (Ollama / OpenAI / Anthropic / Gemini) restauré.
- `mutation_engine.rs` — vraies mutations AST tree-sitter restaurées.
- `pareto.rs` — NSGA-II propre sur 4 objectifs.
- `server.rs` — REST + WebSocket + dashboard live restauré.

### Hygiène
- 5 dépendances mortes supprimées.
- `reqwest` 0.11 native-tls → 0.12 rustls.
- Configs YAML → TOML.
- `examples/my_task/` porté en Rust complet.
- `.github/workflows/ci.yml` créé.
- 41/41 tests OK (vs 25/26).

---

## [4.0.0] — 2026-04-11

- Unified IronReview + T430 → OpenEvolve v4.0
- Migration YAML → TOML
- Migration Python → Rust
- `src/lib.rs` ajouté pour l'exposition aux tests
- CI GitHub Actions
- `.gitignore` corrigé (`target/`)
