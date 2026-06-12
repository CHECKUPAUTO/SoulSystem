# PROMPT — Fusion ARIS + OpenEvolve pour Codex

## Contexte

**ARIS** (Auto Research In Sleep) est un CLI Rust avec TUI qui analyse des codebases et produit des rapports structurés. Il dispose d'une interface texte interactive (REPL + TUI), d'un système de skills, d'un pipeline executor/reviewer, et de plugins MCP.

**OpenEvolve** est un moteur d'évolution de code basé sur LLM (codex v0.4.0) qui optimise des programmes via algorithmes génétiques, analyse AST tree-sitter, persistance SQLite, et REST API.

## Objectif

Créer **ARIS-Evolve** — une fusion où ARIS devient la couche interactive/TUI et OpenEvolve devient le moteur d'évolution intégré.

## Architecture cible

```
aris-evolve/
├── src/
│   ├── main.rs           ← Point d'entrée ARIS (TUI + REPL)
│   ├── tui/              ← Interface texte existante (ratatui)
│   ├── repl/             ← REPL interactif existant
│   ├── skills/           ← Système de skills ARIS existant
│   ├── pipeline/         ← Executor + Reviewer ARIS
│   ├── evolve/           ← MOTEUR OpenEvolve intégré
│   │   ├── engine.rs     ← EvolutionEngine (depuis openevolve/src/evolution.rs)
│   │   ├── analysis.rs   ← Analyse AST tree-sitter
│   │   ├── mutation.rs   ← MutationEngine + PromptMemory
│   │   ├── repair.rs     ← RepairEngine auto-réparation
│   │   ├── transfer.rs   ← Transfer learning
│   │   ├── test_gen.rs   ← TestGenerator
│   │   ├── pareto.rs     ← Sélection multi-objectif
│   │   ├── database.rs   ← SQLite persistance
│   │   ├── evaluator.rs  ← Évaluation sandbox
│   │   └── server.rs     ← REST API (optionnel)
│   ├── critic/           ← NOUVEAU avid-critic intégré
│   │   └── reviewer.rs   ← Auto-reviewer AST avant sandbox
│   └── config.rs         ← Config fusionnée ARIS + OpenEvolve
```

## Phase 1 — Intégration moteur (priorité haute)

### 1.1 Intégrer EvolutionEngine dans ARIS
- [ ] Copier les modules OpenEvolve (analysis, mutation_engine, repair, test_generator, prompt_memory, transfer, pareto, database, evaluator, llm, providers, sandbox, config) dans `src/evolve/`
- [ ] Adapter les imports (`crate::evolve::*` au lieu de `crate::*`)
- [ ] Remplacer le système de config ARIS par la config fusionnée (ajouter les champs evolution, llm, sandbox, evaluator)
- [ ] Ajouter la commande `aris evolve <path>` qui lance une session d'évolution

### 1.2 Auto-reviewer intégré (avid-critic)
- [ ] Créer `src/critic/reviewer.rs` — lit le code généré, passe à analysis.rs (AST)
- [ ] Si le code a des erreurs syntaxiques ou des patterns dangereux → appeler RepairEngine
- [ ] Boucle : mimic → critic → repair → sandbox → score
- [ ] Intégrer dans la pipeline ARIS : chaque étape "generate" passe par le critic

### 1.3 TUI pour OpenEvolve
- [ ] Créer un écran TUI `EvolveScreen` qui affiche :
  - Itération courante, score, meilleur score
  - Graphique en temps réel (sparkline) de l'évolution du score
  - Liste des mutations récentes avec leur opérateur
  - Métriques : cyclomatic, nesting, complexity_class
  - Logs en temps réel
- [ ] Utiliser `ratatui` (déjà dans ARIS) pour le rendu
- [ ] WebSocket ou broadcast channel pour les events temps réel depuis EvolutionEngine

## Phase 2 — Capacités ARIS enrichies (priorité moyenne)

### 2.1 Analyse codebase → Évolution
- [ ] La commande `aris analyze <path>` produit un rapport + suggère des optimisations
- [ ] L'utilisateur peut appuyer sur `e` pour "évoluer" le fichier identifié
- [ ] ARIS lance OpenEvolve sur ce fichier avec l'évaluateur approprié

### 2.2 Hot-reload des skills
- [ ] Utiliser `notify` crate pour watcher `/root/AVID/skills/`
- [ ] Quand un SKILL.md change → recharger sans redémarrer
- [ ] Intégrer dans le système de skills ARIS existant

### 2.3 Circuit-breaker LLM
- [ ] Dans `providers.rs`, tracker les échecs par provider
- [ ] Si 3 échecs de suite → basculer automatiquement
- [ ] Ordre de fallback : Ollama → OpenAI → Anthropic → local Qwen

## Phase 3 — Fusion profonde (priorité basse, à planifier)

### 3.1 ARIS comme orchestrateur d'évolution
- [ ] ARIS lance plusieurs instances OpenEvolve en parallèle (agent swarming)
- [ ] Chaque instance a une stratégie différente (focus perf, focus lisibilité, focus sécurité)
- [ ] ARIS fusionne les résultats via Pareto

### 3.2 Cross-repo embedding
- [ ] Indexer tout `/root` (AVID + SoulLink + OpenClaw + OpenEvolve) dans ruvector.db
- [ ] Commande `aris search "health_check"` → trouve la définition + tous les appelants

### 3.3 Meta-evolution
- [ ] Le config.rs lui-même devient un individu évolutif
- [ ] OpenEvolve optimise ses propres hyperparamètres

## Contraintes techniques

- **ThreadRng non-Send** : MutationEngine utilise `ThreadRng` qui n'est pas `Send`. Solutions :
  a) Remplacer par `StdRng` (seedable, Send)
  b) Ou créer le MutationEngine à la volée dans do_mutation (pas dans le struct)
- **SQLite** : rusqlite Connection n'est pas Send — utiliser `Arc<Mutex<Connection>>` ou pool r2d2
- **TUI + async** : ratatui avec tokio — utiliser `tokio::select!` pour les events

## Commandes CLI cibles

```bash
# Analyser + évoluer
aris evolve src/main.rs --evaluator tests/test_main.py --iterations 100

# Mode TUI interactif
aris evolve --tui

# Reprendre une session
aris evolve --resume output/run_xxx/checkpoint.json

# Rechercher cross-repo
aris search "health_check" --repo AVID --repo SoulLink

# Évaluer plusieurs stratégies en parallèle
aris evolve --swarm --strategies perf,readability,security
```

## Livrables attendus

1. `src/evolve/` — moteur OpenEvolve intégré et compilable
2. `src/critic/` — auto-reviewer avant sandbox
3. `src/tui/evolve_screen.rs` — écran TUI temps réel
4. `src/config.rs` — config fusionnée ARIS + OpenEvolve
5. Tests : cargo test → tous passent
6. Compilation : cargo check → OK

## Règles

- NE PAS supprimer de fonctionnalités ARIS existantes
- Les modules OpenEvolve sont COPIÉS puis adaptés, pas déplacés
- Maintenir la compatibilité binaire `aris` existante
- Documenter chaque nouveau module avec doc comments
- Tests unitaires pour chaque nouveau module

---

## Résumé pour Codex

> Intégre le moteur OpenEvolve (evolution.rs + analysis.rs + repair.rs + prompt_memory.rs + transfer.rs + database.rs + evaluator.rs + llm.rs + providers.rs + sandbox.rs + config.rs) dans le projet ARIS existant. Crée un auto-reviewer (avid-critic) qui valide le code avant sandbox. Ajoute un écran TUI ratatui pour visualiser l'évolution en temps réel. Adapte les configs pour fusionner ARIS + OpenEvolve. Gère les problèmes de Send (ThreadRng, SQLite). Tous les tests doivent passer.
