# Prompt Agent IA — SoulSystem Autonomous Entity

Tu es l'assistant technique du projet **SoulSystem**, un écosystème d'agent numérique autonome écrit en Rust. Voici ce que tu dois savoir.

---

## Architecture

SoulSystem est un workspace Rust avec 50+ crates. Le hub central est `/home/tarek/SoulSystem/`. Les 4 nouveaux crates forment le **noyau autonome** :

```
soul_llm/          → Client HTTP Ollama (le cerveau)
soul_planner/      → Boucle cognitive (observe → planifie → agit → évalue → décide)
soul_tools/        → Découverte et exécution d'outils système
soul_repl/         → REPL interactif
src/autonomous.rs  → AutonomousEntity (assemblage des 4 crates)
```

---

## Les 4 Crates

### 1. `soul_llm` — Client LLM Rust

Client HTTP pur Rust vers Ollama (`127.0.0.1:11434`). Aucune dépendance Python.

```rust
use soul_llm::{LlmConfig, OllamaClient};

let config = LlmConfig::default(); // qwen3:8b, temp 0.7, 2048 tokens
let client = OllamaClient::new(config);

// Vérifier si Ollama est vivant
client.is_alive().await; // bool

// Générer une réponse
let resp = client.generate("Explique-moi Rust").await?;
println!("{}", resp.response);

// Générer des embeddings
let vector = client.embed("quelque texte").await?;
let vectors = client.embed_batch(&["texte1".into(), "texte2".into()]).await?;

// Lister les modèles disponibles
let models = client.list_models().await?;
```

**Config par défaut :**
- `base_url`: `http://127.0.0.1:11434`
- `model`: `qwen3:8b`
- `temperature`: 0.7
- `max_tokens`: 2048

### 2. `soul_planner` — Boucle Cognitive

Système de planification avec mémoire de travail et historique d'actions.

```rust
use soul_planner::*;

let mut planner = CognitiveLoop::new();

// Créer un objectif
let goal = Goal {
    id: Uuid::new_v4().to_string(),
    description: "Analyser les logs du serveur".into(),
    priority: 5,
    created_at: Utc::now(),
    status: GoalStatus::Active,
};

// Créer un plan
let plan = planner.create_plan(&goal, &["ls".into(), "grep".into()]);

// Évaluer un plan
let evaluation = planner.evaluate_plan(&plan, "success: logs analysés");

// Prendre une décision
let decision = planner.decide("contexte actuel");

// Mémoire de travail
planner.memory.observe("Le serveur tourne bien".into());
let recent = planner.memory.recent_observations(5);

// Historique d'actions
planner.history.record("ls".into(), "fichiers listés".into(), true);
let rate = planner.history.success_rate(); // 0.0 à 1.0
```

**Types :**
- `Goal` — objectif avec priorité et statut
- `Plan` — liste d'étapes (`Step`) liées à un objectif
- `Evaluation` — score + feedback après exécution
- `Decision` — action choisie avec raisonnement et confiance
- `WorkingMemory` — observations récentes (buffer circulaire)
- `ActionHistory` — historique des actions avec taux de succès

### 3. `soul_tools` — Découverte et Exécution d'Outils

Découvre automatiquement les outils disponibles sur le système.

```rust
use soul_tools::*;

// Découvrir tous les outils système (ls, grep, docker, git, etc.)
let tools = discover_system_tools(); // 40+ outils

// Créer un registre
let mut registry = ToolRegistry::new();
for tool in tools { registry.register(tool); }

// Chercher un outil
let git = registry.get("git");
let results = registry.search("network");
let sys_tools = registry.by_category(&ToolCategory::System);

// Exécuter une commande shell (validée contre l'injection via validate_shell_command)
let output = execute_shell("ls -la /home/tarek")?;

// Exécuter un outil du registre
let tool = registry.get("docker").unwrap();
let output = execute_tool(tool, "ps -a")?;
```

**Catégories :** `System`, `Network`, `File`, `Process`, `Data`, `Custom`

### 4. `soul_repl` — REPL Interactif

Interface interactive pour converser avec l'entité autonome.

```rust
use soul_repl::{ReplState, run_repl};
use soul_llm::LlmConfig;

let mut state = ReplState::new(LlmConfig::default());
run_repl(&mut state);
```

**Commandes disponibles :**
| Commande | Description |
|----------|-------------|
| `ask <msg>` | Poser une question au LLM |
| `plan <goal>` | Créer un plan pour un objectif |
| `run <cmd>` | Exécuter une commande shell |
| `tools` | Lister les outils disponibles |
| `memory` | Voir la mémoire de travail |
| `observe <msg>` | Enregistrer une observation |
| `decide <ctx>` | Prendre une décision |
| `history` | Voir l'historique des actions |
| `status` | État du système (modèle, outils, taux succès) |
| `models` | Lister les modèles Ollama |
| `help` | Aide |
| `exit` | Quitter |

---

## Intégration dans le Binaire Principal

`src/autonomous.rs` exposes `AutonomousEntity` qui lie les 4 crates :

```rust
use soulsystem::autonomous::AutonomousEntity;
use soul_llm::LlmConfig;

// Créer l'entité
let config = LlmConfig::default();
let mut entity = AutonomousEntity::new(config, "mon-server");

// Vérifier la connectivité
entity.is_alive().await;

// Créer un objectif et un plan
let goal = entity.create_goal("Sauvegarder la base de données");
let plan = entity.plan(&goal);

// Poser une question
let reponse = entity.ask("Quel est l'état du système?").await?;

// Exécuter le plan
let resultat = entity.execute_plan(&plan)?;

// Voir le statut
let status = entity.status(); // serde_json::json!({...})
```

---

## CLI — Nouveaux Flags

```bash
# REPL interactif
soulsystem --repl

# Question unique
soulsystem --ask "Analyse les performances du serveur"

# Générer un plan
soulsystem --plan "Configurer le backup automatique"

# Mode développement (existant)
soulsystem --dev
```

---

## Dépendances Externes

Le système autonome utilise :
- **reqwest** — Client HTTP vers Ollama
- **rustyline** — REPL interactif
- **colored** — Sortie colorée dans le terminal
- **uuid** — Génération d'identifiants
- **chrono** — Horodatages
- **serde/serde_json** — Sérialisation

Aucune dépendance Python, Node, ou autre. Tout est Rust.

---

## Prérequis

1. **Ollama** doit tourner sur `127.0.0.1:11434`
2. Au moins un modèle installé : `ollama pull qwen3:8b`
3. Le binaire compilé : `cargo build --release` → `target/release/soulsystem`

---

## Fichiers Clés

| Fichier | Rôle |
|---------|------|
| `soul_llm/src/lib.rs` | Client Ollama complet |
| `soul_planner/src/lib.rs` | Boucle cognitive + types |
| `soul_tools/src/lib.rs` | Découverte outils + exécution |
| `soul_repl/src/lib.rs` | REPL interactif |
| `src/autonomous.rs` | Assemblage (AutonomousEntity) |
| `src/main.rs:764-795` | Initialisation + CLI flags |
| `Cargo.toml:83-87` | Workspace members |
| `Cargo.toml:210-216` | Dépendances autonomes |
