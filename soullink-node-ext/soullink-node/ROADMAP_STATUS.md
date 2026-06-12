# SoulLink Node — État de la Roadmap

Dernière mise à jour : 2026-04-28

---

## ✅ Terminé

### 1. Métabolisme Numérique (Phases 1-4)
- **Phase 1** — `PainSignal::fitness_pain` fonctionnel (drop_pain + low_pain, plus jamais 0.0)
- **Phase 2** — Budget métabolique par module : `energy_reserve`, `energy_budget`, `base_metabolic_mod`, `energy_priority`, `local_stress`
- **Phase 3** — Pool d'énergie global : collecte surplus neuronal, recharge, secours aux neurones affamés
- **Phase 4** — Coût de croissance + évoluabilité : `neuron_creation_cost`, `synapse_creation_cost`, `growth_energy_threshold`, 10 nouveaux paramètres évoluables

### 2. Instinct de Préservation (Phases A-C)
- Stress complet (Calm → Alert → Stress → Panic)
- Homéostasie : set points, pression métabolique, régulation arousal
- Mort/SOS/Reboot avec conservation du génome évolué

### 3. Génération d'Objectifs Autonome (Phases 1-6)
- **Phase 1** — Nouvelles métriques : `temporal_coherence`, `emergent_complexity`
- **Phase 2** — `DynamicWeights` 7 → 9 dimensions
- **Phase 3** — `FitnessTracker` : historique d'activations, `dynamic_novelty_weight`, `last_stress_mode`
- **Phase 4** — Calcul dans `sample()` : cohérence temporelle, complexité émergente, novelty weight dynamique, goal context-aware, overall pondéré
- **Phase 5** — `ParetoIndividual` [f64; 8] → [f64; 10], `dominates()`, `crowding_distance()`, `hypervolume()`, select_mutation Pareto branch
- **Phase 6** — Intégration `EvolutionGoal` + `select_mutation()` : `target_value()`, `goal_relevance()`

### Corrections de Compilation (session 2026-04-28)
- Tous les modules compilent proprement (lib + binaire)
- 85/86 tests passent (1 échec pré-existant : `test_auto_heal_no_false_positives`)
- Refactoring post-split modulaire : imports, re-exports, dérives, compatibilité Rhai 1.24

---

## ❌ Reste à Faire

### 4. Auto-Modification
Le cerveau peut modifier son propre code source et ses structures de données à chaud.
- Édition de champs dynamiques (`extensions: HashMap<String, f64>`)
- Mutation de scripts Rhai par le cerveau lui-même
- Re-compilation à chaud des scripts modifiés

### 5. Boucle Récursive
Le cerveau peut s'observer et se modifier en boucle fermée.
- Méta-cortex qui analyse les performances du cortex principal
- Feedback loop : observation → analyse → modification → observation
- Détection de boucles infinies et garde-fous

### 6. Reproduction
Un cerveau peut générer un nouveau cerveau à partir de son génome.
- Sérialisation complète du génome évolué
- Spawning d'un nouveau processus SoulLink avec le génome parent
- Variation génétique (crossover + mutation lors de la reproduction)

### 7. Métacognition
Le cerveau a conscience de son propre état et peut raisonner dessus.
- Modèle interne de soi (self-model)
- Prédiction de ses propres états futurs
- Raisonnement contrefactuel ("que se passerait-il si je modifiais X ?")

---

## Structure du Projet

```
/root/soullink-node/
├── src/
│   ├── lib.rs          — Re-exports publics
│   ├── main.rs         — Point d'entrée binaire + tests d'intégration
│   ├── brain.rs        — Simulation centrale
│   ├── brain_module.rs — Modules fonctionnels, StressMode, PainSignal, Stats
│   ├── neuron.rs       — Neurone
│   ├── synapse.rs      — Synapse
│   ├── evolution.rs    — Moteur d'évolution, Pareto, FitnessTracker
│   ├── ssm_cortex.rs   — Cortex Mamba/SSM
│   ├── script_engine.rs— Sandbox Rhai
│   ├── llm_mutator.rs  — Mutations assistées par LLM
│   ├── hotload.rs      — Rechargement à chaud du génome compilé
│   ├── api.rs          — Serveur HTTP + handlers
│   ├── math_engine.rs  — Moteur mathématique symbolique
│   └── migration.rs    — Migration de schéma
└── Cargo.toml
```
