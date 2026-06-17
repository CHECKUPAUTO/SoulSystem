# SoulSystem - Guide Utilisateur Complet

Système d'entités numériques autonomes avec support multi-fournisseurs LLM.

## Table des matières

1. [À propos SoulSystem](#à-propos-soulsystem)
2. [Installation](#installation)
3. [Démarrage rapide](#démarrage-rapide)
4. [Commandes REPL](#commandes-repl)
5. [Commandes CLI](#commandes-cli)
6. [Commandes TUI](#commandes-tui)
7. [Configuration](#configuration)
8. [Intégrations](#intégrations)
9. [Référence API](#référence-api)
10. [Dépannage](#dépannage)
11. [Exemples d'utilisation](#exemples-dutilisation)

---

# À propos SoulSystem

SoulSystem est un workspace Rust unifié qui intègre le monolith original SoulSystem, le monolith des agents autonomes (`soul_agent_core`, `soul_entity`, `souls`, ...), le Neural Mesh SoulLink, le cœur SciRust, et CCOS (Causal Context Operating System).

## Caractéristiques principales

- **Agents autonomes** avec boucle ReAct (observer→planifier→agir→évaluer)
- **Support multi-LLM** (Ollama, OpenAI, Anthropic, etc.)
- **40+ outils système** découverts automatiquement
- **Intégration Docker et système** complète
- **Auto-guérison et métacognition** intégrées
- **Documentation multilingue** (Français et Anglais)

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

---

# Installation

## Prérequis

1. **Rust** 1.75+ installé
   ```bash
   rustup --version
   ```

2. **Ollama** avec un modèle installé
   ```bash
   ollama serve &
   ollama pull qwen3:4b
   ```

3. **Docker** (optionnel, pour gestion des conteneurs)
   ```bash
   docker --version
   ```

4. **Git** 2.0+ pour le clone

## Installation étape par étape

### Option 1 : Installation via Cargo (Recommandée)

```bash
# Cloner le dépôt
cgit clone https://github.com/copilotacker/SoulSystem
s cd SoulSystem

# Build et installation en release
cargo build --release

# Installer les binaires principaux
sudo cp target/release/soulsystem /usr/local/bin/
```

### Option 2 : Utilisation du script de lancement autonome

```bash
# Cloner le dépôt si non déjà fait
cgit clone https://github.com/copilotacker/SoulSystem
s cd SoulSystem

# Rendre le script exécutable
chmod +x launch-autonomous.sh

# Installer les services
sudo cp soulsystem-autonomous.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable soulsystem-autonomous
sudo systemctl start soulsystem-autonomous
```

### Option 3 : Installation manuelle avec tous les composants

```bash
# Cloner tous les sous-modèles (nécessite plus d'espace disque)
git clone --recursive https://github.com/copilotacker/SoulSystem

# Build workspace complet
cd SoulSystem
cargo build --release

# Installer les composants sélectionnés
sudo cp target/release/soulsystem /usr/local/bin/
```

## Vérification de l'installation

```bash
# Vérifier le statut des services
systemctl status soulsystem-autonomous

# Tester le binaire principal
soulsystem --help

# Lancer le REPL interactif
soulsystem --repl
```

---

# Démarrage rapide

## 1. Vérifier le statut complet

```bash
./launch-autonomous.sh status
```

## 2. Lancer le REPL interactif

```bash
./launch-autonomous.sh repl
```

## 3. Poser une question

```bash
./launch-autonomous.sh ask "Analyse l'état du système"
```

## 4. Créer un plan

```bash
./launch-autonomous.sh plan "Optimiser les performances"
```

---

# Commandes REPL

Le REPL (Read-Eval-Print Loop) est l'interface principale pour interagir avec les agents autonomes.

| Commande | Description | Exemple |
|----------|-------------|---------|
| `/ask <msg>` | Poser une question au LLM | `/ask Quelles sont les métriques système actuelles ?` |
| `/help <topic>` | Afficher l'aide pour un sujet | `/help tools` |
| `/models` | Lister tous les modèles Ollama disponibles | `/models` |
| `/status` | Afficher l'état complet du système (LLM, CPU, RAM, Docker) | `/status` |
| `/plan <goal>` | Créer un plan pour atteindre un objectif | `/plan Mettre à jour la documentation` |
| `/run <cmd>` | Exécuter une commande shell | `/run df -h` |
| `/tools` | Lister tous les outils disponibles | `/tools` |
| `/memory` | Voir la mémoire de travail actuelle | `/memory` |
| `/observe <msg>` | Enregistrer une observation dans la mémoire | `/observe Visited /etc/passwd` |
| `/decide <ctx>` | Prendre une décision basée sur le contexte | `/decide Analyser les logs système` |
| `/history` | Voir l'historique des actions | `/history` |
| `/clear` | Effacer la conversation actuelle | `/clear` |
| `/save <name>` | Sauvegarder la session actuelle | `/save session-2026-06-17` |
| `/export <format>` | Exporter les données (json, yaml, txt) | `/export json` |
| `/files <pattern>` | Lister les fichiers correspondant au motif | `/files *.rs` |
| `/search <query>` | Rechercher dans la mémoire et les fichiers | `/search performance` |

### Exemples d'utilisation du REPL

```bash
# Commencer une conversation
soulsystem --repl

# Poser une question sur le système
/ask Quelles sont les métriques CPU actuelles ?

# Lister les outils disponibles
/tools

# Voir la mémoire de travail actuelle
/memory

# Créer un plan pour une tâche
/plan "Mettre à jour la documentation utilisateur"

# Exécuter une commande système
/run ps aux | grep ollama

# Enregistrer une observation
/observe Checked system performance metrics

# Prendre une décision
/decide Recommend upgrading to qwen3:8b model

# Voir l'historique
/history
```

---

# Commandes CLI

Les commandes CLI (Command Line Interface) fournissent un accès programmatique aux fonctionnalités de SoulSystem.

```bash
/ask, /help, /models, /status, /plan, /run,
/observe, /decide, /clear, /save, /export,
/files, /search
```

### Utilisation des commandes CLI

```bash
# Poser une question via CLI
soulsystem /ask "Analyse le système"

# Créer un plan via CLI
soulsystem /plan "Optimiser les performances"

# Exécuter une commande via CLI
soulsystem /run uptime

# Voir le statut via CLI
soulsystem /status
```

---

# Commandes TUI

Le TUI (Terminal User Interface) est l'interface utilisateur graphique en mode texte avec support clavier.

| Raccourci | Action | Description |
|-----------|--------|-------------|
| `Ctrl+Shift+P` | Palette de commandes | Afficher la palette de commandes (comme VS Code) |
| `Ctrl+F` | Navigateur de fichiers | Ouvrir le navigateur de fichiers |
| `Ctrl+R` | Recherche historique | Rechercher dans l'historique des conversations |
| `Ctrl+O` | Gestionnaire de sessions | Ouvrir le gestionnaire de sessions |
| `Ctrl+Y` | Copier dans le presse-papier | Copier la sélection dans le presse-papier |
| `Ctrl+E` | Exporter le chat | Exporter la conversation actuelle |
| `Shift+Enter` | Saisie multi-lignes | Insérer une nouvelle ligne dans l'éditeur |

### Navigation TUI

```bash
# Lancer l'interface TUI
soulsystem --dev

# Utiliser les raccourcis clavier
Ctrl+Shift+P    → Palette de commandes
Ctrl+F        → Navigateur de fichiers
Ctrl+R        → Recherche historique
Ctrl+O        → Gestionnaire de sessions
Ctrl+Y        → Copier
Ctrl+E        → Exporter
Shift+Enter   → Nouvelle ligne
```

---

# Configuration

## Fichiers de configuration

### `~/.config/soulsystem/`

Créez ce répertoire et ajoutez les fichiers de configuration suivants :

#### `config.toml` (Configuration principale)

```toml
title = "SoulSystem"

[llm]
provider = "ollama"
model = "qwen3:4b"
host = "http://localhost:11434"
timeout = 30

[system]
monitor_interval = 5
cpu_threshold = 80
memory_threshold = 85

[docker]
enabled = true
endpoint = "unix:///var/run/docker.sock"

[soul_bridge]
rest_port = 9030
enable_telegram = false
telegram_token = ""
```

#### `.env` (Variables d'environnement)

```bash
# LLM Configuration
OLLAMA_HOST=http://localhost:11434
OLLAMA_MODEL=qwen3:4b

# Telegram Configuration
TELEGRAM_BOT_TOKEN=your_bot_token_here
TELEGRAM_CHAT_ID=your_chat_id

# SoulSystem Configuration
SOULSYSTEM_LOG_LEVEL=info
SOULSYSTEM_MAX_CONVERSATIONS=100

# Bridge Configuration
BRIDGE_REST_PORT=9030
BRIDGE_TELEGRAM_ENABLED=true
```

#### `extensions.json` (Extensions disponibles)

```json
[
  {
    "name": "telegram",
    "enabled": true,
    "config": {
      "bot_token": "${TELEGRAM_BOT_TOKEN}",
      "chat_id": "${TELEGRAM_CHAT_ID}"
    }
  },
  {
    "name": "docker",
    "enabled": true,
    "config": {
      "endpoint": "unix:///var/run/docker.sock"
    }
  },
  {
    "name": "system_monitor",
    "enabled": true,
    "config": {
      "interval": 5
    }
  }
]
```

---

# Intégrations

## Ollama (LLM Principal)

- **Statut** : ✅ Actif
- **Modèle** : qwen3:4b (par défaut)
- **Streaming** : NDJSON
- **Intégration** : Locale, sans clé, rapide

### Commande pour changer de modèle

```bash
/soulsystem ask "Change to llama2:7b model"
```

## Docker (Conteneurs)

- **Statut** : ✅ Actif
- **Fonctionnalités** :
  - `list_containers()` - Lister tous les conteneurs
  - `start_container(name)` - Démarrer un conteneur
  - `stop_container(name)` - Arrêter un conteneur
  - `is_docker_running()` - Vérifier si Docker tourne

### Utilisation des outils Docker

```bash
/run docker ps -a
/run docker start my-app
/run docker stop my-app
```

## OpenEvolve (Auto-évolution)

- **Statut** : ⏳ Disponible (si le service tourne sur le port configuré)
- **Fonctionnalité** : Auto-évolution de code via LLM

## System Monitor (Surveillance système)

- **Statut** : ✅ Actif
- **Métriques** : CPU, Mémoire, Processes, Load
- **Source** : Métriques en temps réel depuis `/proc`

### Commandes de surveillance

```bash
/run free -h
/run mpstat 1 3
/run iostat -x 1
```

---

# Référence API

## Points de terminaison REST

### `/api/v1/`

| Méthode | Endpoint | Description |
|---------|----------|-------------|
| `GET` | `/status` | Retourne l'état complet du système |
| `POST` | `/ask` | Poser une question au LLM |
| `POST` | `/plan` | Créer un plan |
| `POST` | `/run` | Exécuter une commande shell |
| `GET` | `/tools` | Lister les outils disponibles |
| `GET` | `/memory` | Voir la mémoire de travail |
| `POST` | `/observe` | Enregistrer une observation |
| `POST` | `/decide` | Prendre une décision |

### `/api/v1/health`

Retourne l'état de santé du système.

```json
{
  "status": "healthy",
  "timestamp": "2026-06-17T10:30:00Z",
  "services": {
    "llm": "connected",
    "docker": "connected",
    "system_monitor": "active"
  },
  "uptime": "2h 15m 30s"
}
```

### `/api/v1/telegram`

Gestion des webhooks Telegram.

```json
{
  "webhook": {
    "url": "https://your-domain.com/api/v1/telegram",
    "allowed_updates": ["message"]
  },
  "bot_info": {
    "username": "soul_system_bot",
    "first_name": "Soul",
    "description": "Système d'entités numériques autonomes"
  }
}
```

---

# Dépannage

## Erreurs courantes et solutions

### 1. Ollama non installé ou non démarré

**Problème** : Ollama n'est pas installé ou n'est pas en cours d'exécution.

**Solution** :
```bash
# Installer Ollama (Debian/Ubuntu)
curl -fsSL https://ollama.com/install.sh | sh

# Démarrer Ollama
sudo systemctl start ollama
sudo systemctl enable ollama

# Télécharger un modèle
sudo -u ollama ollama pull qwen3:4b
```

### 2. Token Telegram non configuré

**Problème** : Le bot Telegram n'est pas configuré.

**Solution** :
```bash
# Obtenir un token Telegram
https://t.me/BotFather
/appel /start
/appel /newbot

# Configurer les variables d'environnement
echo "TELEGRAM_BOT_TOKEN=your_token_here" >> .env
```

### 3. Ports bloqués

**Problème** : Les ports nécessaires sont déjà utilisés.

**Solution** :
```bash
# Vérifier quels processus utilisent les ports
netstat -tulpn | grep :9030
netstat -tulpn | grep :11434

# Terminer les processus conflictuels
sudo kill <pid>

# Ou changer les ports de configuration
# dans config.toml
```

### 4. Permissions insuffisantes

**Problème** : SoulSystem n'a pas les permissions nécessaires.

**Solution** :
```bash
# Permissions pour Ollama
sudo usermod -aG ollama $USER

# Permissions pour Docker
sudo usermod -aG docker $USER

# Permissions pour les fichiers de configuration
chmod 700 ~/.config/soulsystem/
chmod 600 ~/.config/soulsystem/*.toml
```

### 5. Configuration incorrecte

**Problème** : La configuration n'est pas correcte.

**Solution** :
```bash
# Valider la configuration TOML
cargo run --bin config-validator

# Afficher la configuration actuelle
soulsystem /status

# Afficher les erreurs de configuration
soulsystem --debug
```

## Commandes de diagnostic

```bash
# Vérifier l'état du système
./launch-autonomous.sh status

# Afficher les logs
journalctl -u soulsystem-autonomous -f

# Tester la connectivité
./launch-autonomous.sh test

# Recharger la configuration
sudo systemctl reload soulsystem-autonomous

# Redémarrer le service
sudo systemctl restart soulsystem-autonomous
```

---

# Exemples d'utilisation

## Exemple 1 : Surveillance système complète

```bash
# 1. Lancer le système
./launch-autonomous.sh repl

# 2. Demander une analyse système
/ask "Donne-moi une analyse complète des métriques système actuelles"

# 3. Lister les outils disponibles
/tools

# 4. Voir la mémoire de travail actuelle
/memory

# 5. Créer un plan pour optimiser
/plan "Optimiser les performances et réduire la latence"

# 6. Exécuter des commandes de diagnostic
/run uptime
/run free -h
/run mpstat 1 5

# 7. Enregistrer des observations
/observe System running smoothly with 45% CPU usage
/observe Memory usage at 67% of total
/observe Ollama responding normally

# 8. Prendre une décision
/decide Recommend monitoring CPU usage during peak hours
```

## Exemple 2 : Gestion Docker via Telegram

```bash
# 1. Configurer le bot Telegram
# Obtenir le token depuis @BotFather
# Ajouter au .env
TELEGRAM_BOT_TOKEN=your_token
TELEGRAM_CHAT_ID=your_chat_id

# 2. Lancer le système
./launch-autonomous.sh repl

# 3. Vérifier l'intégration Docker
/ask "List all running Docker containers"

# 4. Prendre une action via l'outil Docker
/run docker ps -a
/run docker stats --no-stream

# 5. Envoyer une notification via Telegram
/telegram "Container usage: high"
```

## Exemple 3 : Auto-guérison et métacognition

```bash
# 1. Lancer le système
./launch-autonomous.sh repl

# 2. Demander une métacognition système
/ask "Analyze system health and suggest improvements"

# 3. Voir les opportunités d'auto-guérison
/memory

# 4. Créer un plan d'auto-guérison
/plan "Implement auto-healing mechanisms and improve error recovery"

# 5. Exécuter les tâches de maintenance
/run journalctl -u soulsystem --since "1 hour ago" | grep ERROR
/run sudo systemctl status ollama

# 6. Enregistrer les résultats
/observe Auto-healing completed successfully
/observe System health improved

# 7. Prendre une décision finale
/decide System is stable and ready for production
```

## Aide rapide

```bash
# Commandes principales (REPL)
/ask <msg>        - Poser une question au LLM
/plan <goal>      - Créer un plan
/run <cmd>        - Exécuter une commande shell
/tools            - Lister les outils disponibles
/memory          - Voir la mémoire de travail
/observe <msg>    - Enregistrer une observation
/decide <ctx>     - Prendre une décision
/history          - Voir l'historique des actions
/status           - Voir le statut système
/models          - Lister les modèles disponibles
/clear            - Effacer la conversation
/save <name>      - Sauvegarder la session
/export <format>  - Exporter les données
/files <pattern>  - Lister les fichiers
/search <query>   - Rechercher

# Commandes TUI
Ctrl+Shift+P      - Palette de commandes
Ctrl+F           - Navigateur de fichiers
Ctrl+R           - Recherche historique
Ctrl+O           - Gestionnaire de sessions
Ctrl+Y           - Copier
Ctrl+E           - Exporter
Shift+Enter      - Nouvelle ligne
```

---

# Licence

MIT

---

# Support

Pour toute question, problème ou suggestion, veuillez contacter l'équipe SoulSystem à support@soulsystem.ai.

Vous pouvez également ouvrir un issue sur GitHub : https://github.com/copilotacker/SoulSystem/issues

Les retours sont toujours les bienvenus ! 🎉
