# Plan d'intégration OpenEvolve v0.4.0

## Modules à câbler

### 1. mutation_engine.rs → evolution.rs
- [ ] Ajouter MutationEngine dans EvolutionEngine
- [ ] Utiliser structured_mutate() avant le fallback LLM dans do_mutation()
- [ ] Configurer structured_rate depuis Config

### 2. repair.rs → evolution.rs
- [ ] Ajouter RepairEngine dans EvolutionEngine
- [ ] Quand validate_syntax échoue, tenter repair avant de skip
- [ ] Si repair réussit, évaluer le code réparé

### 3. test_generator.rs → evaluator.rs
- [ ] Ajouter TestGenerator dans Evaluator
- [ ] Après évaluation principale, générer et exécuter des tests
- [ ] Ajouter test_score aux métriques

### 4. prompt_memory.rs → evolution.rs
- [ ] Ajouter PromptMemory dans EvolutionEngine
- [ ] Tracker les prompts mutation et leur score
- [ ] Utiliser select_prompt() pour choisir le meilleur prompt

### 5. pareto.rs → evolution.rs
- [ ] Utiliser nsga2_select() pour la sélection des parents
- [ ] Multi-objective: performance + readability + size + security

### 6. transfer.rs → evolution.rs
- [ ] Ajouter TransferEngine dans EvolutionEngine
- [ ] Enrichir les prompts avec build_prompt_with_transfer()
- [ ] Réutiliser les patterns historiques réussis

## Fichiers à modifier
- [ ] src/evolution.rs — câbler mutation_engine, repair, prompt_memory, transfer, pareto
- [ ] src/evaluator.rs — câbler test_generator
- [ ] src/config.rs — ajouter les nouveaux champs de config
- [ ] src/main.rs — ajouter les arguments CLI
- [ ] Cargo.toml — ajouter les dépendances si manquantes
