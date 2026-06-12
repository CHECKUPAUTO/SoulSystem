# SoulSystem Autonomous Entity

Système autonome basé sur Rust, propulsé par Ollama.

## Démarrage rapide

```bash
# Vérifier le statut
./launch-autonomous.sh status

# Lancer le REPL interactif
./launch-autonomous.sh repl

# Poser une question
./launch-autonomous.sh ask "Analyse les performances du serveur"

# Créer un plan
./launch-autonomous.sh plan "Configurer le backup automatique"
```

## Architecture

```
soul_llm/          → Client Ollama (Rust pur)
soul_planner/      → Boucle cognitive (observe→plan→act→evaluate→decide)
soul_tools/        → 40+ outils système découverts automatiquement
soul_repl/         → REPL interactif
src/autonomous.rs  → AutonomousEntity (assemblage)
```

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
| `status` | État du système |
| `models` | Lister les modèles Ollama |

## Installation comme service

```bash
# Copier le fichier service
sudo cp soulsystem-autonomous.service /etc/systemd/system/

# Activer et démarrer
sudo systemctl daemon-reload
sudo systemctl enable soulsystem-autonomous
sudo systemctl start soulsystem-autonomous

# Voir les logs
sudo journalctl -u soulsystem-autonomous -f
```

## Prérequis

1. Ollama installé et démarré (`ollama serve`)
2. Modèle disponible (`ollama pull qwen3:4b`)
3. Rust 1.75+ pour compiler

## Configuration

Le modèle par défaut est `qwen3:4b`. Pour changer :

1. Éditer `soul_llm/src/lib.rs` ligne 26
2. Ou passer un modèle différent via l'API

## Stack Technique

- **LLM**: Ollama (local, pas de cloud)
- **HTTP**: reqwest (blocking + async)
- **REPL**: rustyline
- **CLI**: clap
- **Sérialisation**: serde + serde_json
- **CLI flags**: `--repl`, `--ask`, `--plan`
