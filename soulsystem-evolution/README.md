# SoulSystem — Système d'Évolution Autonome

## Vue d'ensemble

SoulSystem Evolution est un écosystème computationnel auto-améliorant déployé en **container Docker** à côté d'une instance Ollama existante. Il compose d'agents évolutifs, d'un cerveau web asynchrone, d'un moteur de mutation LLM local, et de couches d'abstraction conceptuelle.

## Architecture

```
Sources Web
   ↓
Crawler Async (Rust + Reqwest)
   ↓
Moteur de Ranking (pertinence + nouveauté + qualité)
   ↓
File de tâches (async channels)
   ↓
Moteur de Mutation Ollama (LLM local GPU)  ←── localhost:11434
   ↓
Population d'Agents (évolution Darwin)
   ↓
Évaluation Fitness
   ↓
Sélection + Reproduction
   ↓
M�moire + Snapshot
   ↓
Boucle infinie
```

## Déploiement Docker (recommandé)

### Prérequis

- **Docker** + **Docker Compose** installés
- **Ollama** déjà en fonctionnement sur le serveur (`localhost:11434`)
- **Modèle** chargé : `ollama pull llama3`
- **GPU** : driver NVIDIA + nvidia-container-toolkit (pour passthrough GPU)

### Lancement rapide

```bash
# Copier la config
cp .env.example .env

# Build + lancement
make build
make up

# Voir les logs
make logs
```

### Commandes Make

| Commande | Action |
|----------|--------|
| `make build` | Build l'image Docker |
| `make up` | Lance le container (vérifie Ollama d'abord) |
| `make down` | Arrête le container |
| `make restart` | Redémarre |
| `make logs` | Logs temps réel |
| `make status` | État + santé + ressources |
| `make shell` | Shell dans le container |
| `make backup` | Backup des snapshots |
| `make snapshots` | Lister les snapshots |
| `make clean` | Tout supprimer (images + volumes) |

### Stratégie réseau

Le container utilise `network_mode: host` pour accéder directement à Ollama sur `localhost:11434`. Aucune configuration réseau supplémentaire n'est nécessaire.

## Configuration

Toutes les options sont pilotées par variables d'environnement (voir `.env.example`) :

| Variable | Défaut | Description |
|----------|--------|-------------|
| `SOULSYSTEM_EVO_OLLAMA_URL` | `http://localhost:11434` | URL Ollama |
| `SOULSYSTEM_EVO_MODEL` | `llama3` | Modèle LLM |
| `SOULSYSTEM_EVO_MAX_LLM_CONCURRENT` | `2` | Max appels LLM simultanés |
| `SOULSYSTEM_EVO_CONTEXT_WINDOW` | `2048` | Fenêtre de contexte (tokens) |
| `SOULSYSTEM_EVO_POPULATION_SIZE` | `20` | Taille population d'agents |
| `SOULSYSTEM_EVO_MUTATION_RATE` | `0.3` | Taux de mutation |
| `SOULSYSTEM_EVO_SURVIVAL_RATE` | `0.5` | Taux de survie |
| `SOULSYSTEM_EVO_SNAPSHOT_INTERVAL` | `5` | Cycles entre snapshots |
| `SOULSYSTEM_EVO_CYCLE_DELAY` | `5` | Secondes entre cycles |

## Modules

| Module | Description |
|--------|-------------|
| `config.rs` | Configuration via env vars |
| `agent.rs` | Agents, rôles, fitness, croisement, sélection |
| `crawler.rs` | Crawler web async avec retry |
| `ranking.rs` | Scoring pertinence/nouveauté/qualité |
| `ollama.rs` | Client LLM local avec semaphore GPU |
| `memory.rs` | Mémoire lock-free DashMap + persistance |
| `il.rs` | Langage interne intermédiaire |
| `meta_language.rs` | Grammaire évolutive + vecteurs sémantiques |
| `concept.rs` | Couche d'abstraction haute |
| `snapshot.rs` | Sauvegarde et rollback |
| `evolution.rs` | Boucle d'évolution principale |

## Fonction de Fitness

```
fitness = performance × 0.4 + stabilité × 0.3 + créativité × 0.2 + efficacité_mémoire × 0.1
```

## Contraintes RTX 4060

- Max 2 appels LLM concurrents (semaphore)
- Fenêtre de contexte : 2048 tokens
- Un seul modèle chargé à la fois
- La VRAM ne doit jamais déborder

## Données persistantes

Les données sont stockées dans des volumes Docker :
- `soulsystem-data` → snapshots + agents
- `soulsystem-logs` → logs d'exécution

Backup : `make backup`
