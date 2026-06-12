# AVID — Exemples d'Usage Concret

## Exemple 1 — Cloner une API Innovante

**Input:** `Clone l'API de Stripe Payments`

**Pipeline:**
1. **Scout** visite https://stripe.com/docs/api
   → Extraction endpoints, modèles, exemples
2. **Vision** identifie patterns REST + JSON + idempotency-key
3. **Cortex** comprend: PaymentIntent → 3D Secure → Webhooks
4. **Mimic** génère API Rust/axum équivalente
5. **Original** vérifie que c'est une réimplémentation propre
6. **Forge** produit crate compilable avec tests

**Output:** Clone fonctionnel de l'API Stripe en Rust

---

## Exemple 2 — Comprendre un Paper ArXiv

**Input:** `Explique le paper NSA (arXiv:2502.11089)`

**Pipeline:**
1. **Scout** télécharge PDF depuis arxiv.org
2. **Vision** extrait structure: Abstract, Method, Experiments
3. **Cortex** comprend: Compressed + Fine + Sliding Window attention
4. **Mimic** génère implémentation PyTorch from scratch
5. **Original** vérifie que ce n'est pas un clone du code original
6. **Forge** produit module documenté avec benchmarks

**Output:** Réimplémentation propre de Native Sparse Attention

---

## Exemple 3 — Analyser un Site E-commerce

**Input:** `Analyse Amazon et crée un moteur de recherche équivalent`

**Pipeline:**
1. **Scout** explore navigation, filtres, page produit
2. **Vision** détecte: search bar, faceted search, recommendations
3. **Cortex** comprend: TF-IDF, BM25, collaborative filtering
4. **Mimic** reconstruit moteur Rust + tantivy + reco matrix
5. **Original** confirme que c'est original
6. **Forge** produit app web complète avec Docker

**Output:** Moteur de recherche e-commerce from scratch

---

## Exemple 4 — Documentation Technique

**Input:** `Crée une lib Rust pour parser les fichiers STEP (CAD)`

**Pipeline:**
1. **Scout** cherche docs ISO 10303-21
2. **Vision** analyse grammar: ENTITY, TYPE, SCHEMA
3. **Cortex** comprend parsing: lexer → parser → AST → validation
4. **Mimic** génère parser Rust avec types forts
5. **Original** vérifie qu'il n'existe pas de similaire
6. **Forge** produit crate publiable sur crates.io

**Output:** Lib Rust ISO 10303-21 complète

---

## Exemple 5 — Reverse Engineer une App

**Input:** `Reverse engineer Notion.so et crée un clone local`

**Pipeline:**
1. **Scout** explore: pages, blocks, databases, relations
2. **Vision** détecte: block-based editor, database views, relations
3. **Cortex** comprend: CRDT, SQLite, WASM
4. **Mimic** reconstruit: éditeur Rust + Yjs + SQLite + Leptos
5. **Original** confirme architecture différente
6. **Forge** produit app desktop (Tauri) avec packaging

**Output:** Clone local de Notion (pas cloud)

---

## Autres Idées d'Usage

- **API Monitoring:** Surveiller une API concurrente et proposer améliorations
- **Doc Generator:** À partir d'un repo GitHub, générer documentation interactive
- **Security Audit:** Analyser une app web et proposer correctifs
- **Migration Assistant:** Migrer une app Python 2 → Rust moderne
- **Plugin Generator:** À partir d'une API, générer plugins pour VSCode, IntelliJ
- **Test Generator:** À partir d'un codebase, générer tests de régression
- **CLI Generator:** À partir d'une lib, générer CLI ergonomique
- **SDK Generator:** À partir d'une API REST, générer SDK multi-langues
