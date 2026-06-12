Tu es l'architecte principal d'OpenEvolve, un moteur d'évolution de code par LLM en Rust.
L'écosystème AVID/SoulLink/OpenClaw est maintenant stable (cargo check OK partout).

MISSION : Transformer OpenEvolve d'un outil de proto-typage en une plateforme
d'ingénierie logicielle autonome de niveau industriel.

PHASE 1 — Fondations (modules existants à renforcer)

1. REMPLACER analysis.rs (regex) par analyse AST réelle :
   - Intégrer tree-sitter avec parsers Python, Rust, JS, Go
   - Détecter : dead code, imports non utilisés, variables non utilisées,
     fonctions trop longues (>50 lignes), nesting réel (pas indent/2),
     complexité cyclomatique réelle (branches AST)
   - Scorer : maintenabilité, testabilité, sécurité (patterns dangereux)

2. RENFORCER database.rs avec persistence + embeddings :
   - SQLite via rusqlite : table programs(id, code, score, metrics_json,
     embedding, parent_id, generation, island, created_at)
   - Embeddings sémantiques via fastembed-rs ou appel LLM pour vectoriser le code
   - Similarity search : trouver les programmes proches pour transfer learning
   - Resume d'une run interrompue : recharger population depuis SQLite

3. ACTIVER parallel_evaluations dans evolution.rs :
   - tokio::spawn pour évaluer N programmes en parallèle
   - Semaphore pour limiter la concurrence (config)
   - Garder l'ordre des résultats pour l'affichage

PHASE 2 — Capacités nouvelles (modules à créer)

4. Créer src/test_generator.rs — Génération automatique de tests :
   - LLM génère des inputs/expected_outputs à partir du code candidat
   - Exécuter les tests dans la sandbox
   - Score = % tests passés (en plus du score évaluateur externe)
   - Fallback : si LLM indisponible, générer inputs aléatoires + exécuter sans crash

5. Créer src/repair.rs — Auto-réparation syntaxique :
   - Quand validate_syntax échoue, appeler LLM avec le code + l'erreur
   - LLM retourne le code corrigé (pas regeneré, juste réparé)
   - Max 3 tentatives de repair avant abandon
   - Comptabiliser repair_success_rate dans les métriques

6. Créer src/mutation_engine.rs — Mutation structurée par AST :
   - Operations : extract_function, inline_variable, rename_variable,
     loop_unroll, add_early_return, replace_with_iter, add_type_hint
   - Utiliser tree-sitter pour appliquer des transformations sûres
   - LLM utilisé UNIQUEMENT pour les mutations créatives (nouvelle logique)
   - 70% mutations structurées (AST) + 30% mutations créatives (LLM)

7. Créer src/server.rs — Mode serveur REST + WebSocket :
   - POST /evolve {code, evaluator, config} → lance evolution async
   - GET /status/{run_id} → progrès, score actuel, best program
   - WebSocket /ws/{run_id} → streaming temps réel des itérations
   - GET /library → recherche dans la base SQLite de programmes historiques

PHASE 3 — Intelligence (stratégies avancées)

8. Prompts adaptatifs :
   - Créer src/prompt_memory.rs : tracker quels prompts ont produit
     les meilleures mutations (prompt → score moyen)
   - Adapter le prompt de mutation en fonction du langage, de la complexité,
     et de l'historique de succès

9. Multi-objectif Pareto :
   - Remplacer score unique par front Pareto (performance, lisibilité,
     taille, sécurité)
   - NSGA-II algorithm dans database.rs pour la sélection
   - Garder les non-dominés sur toutes les dimensions

10. Transfer learning inter-tâches :
    - Quand une tâche A est résolue, extraire les patterns réutilisables
    - Rechercher dans la base SQLite les programmes similaires (embedding)
    - Injecter les patterns réussis dans le prompt initial de la tâche B

CONTRAINTES
- Tout doit compiler : cargo check --workspace OK
- Tests : cargo test doit passer (au moins 80% coverage des nouveaux modules)
- Pas de breaking change sur la CLI existante (ajout de flags optionnels)
- SQLite pour la persistence (pas de dépendance externe)
- tree-sitter déjà utilisé par AVID-scout : réutiliser la config

PRIORITÉ D'IMPLÉMENTATION
1. analysis.rs (AST) + database.rs (SQLite) → base de tout
2. parallel_evaluations → gain immédiat perf
3. test_generator.rs → fiabilité des programmes
4. mutation_engine.rs → qualité des mutations
5. repair.rs → moins de rejets
6. server.rs + prompts adaptatifs + Pareto + transfer → niveau supérieur

LIVRABLE : Tous les modules, tests, et documentation dans docs/OPNEVOLVE_V2.md
