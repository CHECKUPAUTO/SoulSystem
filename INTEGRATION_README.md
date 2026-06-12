# SoulSystem Autonomous Entity - Full Integration

Système autonome complet basé sur Rust, intégrant tous les services du serveur.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    AutonomousEntity                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   soul_llm  │  │soul_planner │  │ soul_tools  │        │
│  │  (Ollama)   │  │  (Cognitive)│  │  (40+ outils)│        │
│  └─────────────┘  └─────────────┘  └─────────────┘        │
│                          │                                   │
│  ┌───────────────────────┴───────────────────────┐         │
│  │              soul_bridges                      │         │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐     │         │
│  │  │OpenEvolve│ │  Docker  │ │ Monitor  │     │         │
│  │  │(Evolution)│ │(Containers)│ │(CPU/RAM) │     │         │
│  │  └──────────┘ └──────────┘ └──────────┘     │         │
│  └───────────────────────────────────────────────┘         │
└─────────────────────────────────────────────────────────────┘
```

## Démarrage rapide

```bash
# Vérifier le statut complet
./launch-autonomous.sh status

# Lancer le REPL interactif
./launch-autonomous.sh repl

# Poser une question
./launch-autonomous.sh ask "Analyse l'état du système"

# Créer un plan
./launch-autonomous.sh plan "Optimiser les performances"
```

## Crates

| Crate | Description |
|-------|-------------|
| `soul_llm` | Client Ollama (Rust pur, async + blocking) |
| `soul_planner` | Boucle cognitive (observe→plan→act→evaluate→decide) |
| `soul_tools` | 40+ outils système découverts automatiquement |
| `soul_repl` | REPL interactif avec rustyline |
| `soul_bridges` | Intégration des services externes |

## Modules soul_bridges

### OpenEvolve
- Auto-évolution de code via LLM
- Disponible si le service tourne sur le port configuré

### Docker
- `list_containers()` - Lister tous les conteneurs
- `start_container(name)` - Démarrer un conteneur
- `stop_container(name)` - Arrêter un conteneur
- `is_docker_running()` - Vérifier si Docker tourne

### System Monitor
- `get_metrics()` - CPU, Mémoire, Processes, Load
- `list_processes()` - Top 20 processus par CPU
- Métriques en temps réel depuis /proc

### Memory Bridge
- `store(content, type)` - Stocker une observation
- `search(query)` - Chercher dans la mémoire
- `recent(n)` - Dernières entrées
- `by_type(type)` - Filtrer par type

### Orchestrator
- Assemblage de tous les services
- `status()` - État complet du système
- `observe()` - Enregistrer une observation
- `decide()` - Prendre une décision basée sur les métriques

## Commandes REPL

| Commande | Description |
|----------|-------------|
| `ask <msg>` | Poser une question au LLM |
| `plan <goal>` | Créer un plan |
| `run <cmd>` | Exécuter une commande shell |
| `tools` | Lister les outils disponibles |
| `memory` | Voir la mémoire de travail |
| `observe <msg>` | Enregistrer une observation |
| `decide <ctx>` | Prendre une décision |
| `history` | Voir l'historique des actions |
| `status` | État complet (LLM, CPU, RAM, Docker) |
| `models` | Lister les modèles Ollama |

## Installation comme service

```bash
sudo cp soulsystem-autonomous.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable soulsystem-autonomous
sudo systemctl start soulsystem-autonomous
sudo journalctl -u soulsystem-autonomous -f
```

## Intégrations

| Service | Statut | Description |
|---------|--------|-------------|
| Ollama | ✅ Actif | LLM local (qwen3:4b) |
| Docker | ✅ Actif | Gestion conteneurs |
| OpenEvolve | ⏳ Dispo | Auto-évolution (si service actif) |
| System | ✅ Actif | CPU, RAM, Processus |

## Prérequis

1. Ollama avec modèle installé
2. Docker (optionnel, pour gestion conteneurs)
3. Rust 1.75+ pour compiler
4. OpenEvolve (optionnel, pour auto-évolution)

## Stack Technique

- **LLM**: Ollama (local)
- **HTTP**: reqwest (blocking + async)
- **REPL**: rustyline
- **CLI**: clap
- **Monitoring**: /proc filesystem
- **Docker**: CLI docker
- **Sérialisation**: serde + serde_json
