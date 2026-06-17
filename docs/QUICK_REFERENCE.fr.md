# SoulSystem Fiche de Référence Rapide

## 🚀 Lancement rapide

| Action | Commande |
|--------|---------|
| Vérifier le statut | `./launch-autonomous.sh status` |
| Lancer REPL | `./launch-autonomous.sh repl` |
| Poser une question | `./launch-autonomous.sh ask "..."` |
| Créer un plan | `./launch-autonomous.sh plan "..."` |

## 💬 Commandes REPL

| Commande | Description |
|----------|-------------|
| `/ask <msg>` | Poser une question au LLM |
| `/help <sujet>` | Afficher l'aide pour un sujet |
| `/models` | Lister les modèles Ollama disponibles |
| `/status` | Afficher l'état système (LLM, CPU, RAM, Docker) |
| `/plan <objectif>` | Créer un plan d'action |
| `/run <cmd>` | Exécuter une commande shell |
| `/tools` | Lister les outils disponibles |
| `/memory` | Voir la mémoire de travail |
| `/observe <msg>` | Enregistrer une observation |
| `/decide <contexte>` | Prendre une décision |
| `/history` | Voir l'historique des actions |
| `/clear` | Effacer la conversation |
| `/save <nom>` | Sauvegarder la session |
| `/export <format>` | Exporter les données (json/yaml/txt) |
| `/files <motif>` | Lister les fichiers correspondant |
| `/search <recherche>` | Rechercher dans la mémoire |

## ⌨️ Raccourcis TUI

| Raccourci | Action |
|-----------|--------|
| `Ctrl+Shift+P` | Palette de commandes |
| `Ctrl+F` | Navigateur de fichiers |
| `Ctrl+R` | Recherche historique |
| `Ctrl+O` | Gestionnaire de sessions |
| `Ctrl+Y` | Copier dans le presse-papier |
| `Ctrl+E` | Exporter le chat |
| `Shift+Enter` | Saisie multi-lignes |

## 🔧 Configuration

```bash
# Fichiers principaux
~/.config/soulsystem/config.toml    # Configuration principale
~/.config/soulsystem/.env           # Variables d'environnement
```

```bash
# Variables d'environnement clés
export TELEGRAM_BOT_TOKEN="..."
export OLLAMA_HOST="http://localhost:11434"
export OLLAMA_MODEL="qwen3:4b"
```

## 🩺 Dépannage

| Problème | Solution |
|---------|-----|
| Ollama ne tourne pas | `sudo systemctl start ollama` |
| Port déjà utilisé | `sudo kill $(lsof -t :9030)` |
| Erreur configuration | `soulsystem --debug` |
| Permission refusée | `chmod 700 ~/.config/soulsystem/` |

## 📚 Documentation complète

- **User Guide**: `docs/USER_GUIDE.md`
- **Guide Utilisateur**: `docs/USER_GUIDE.fr.md`
- **Quick Reference**: `docs/QUICK_REFERENCE.md` (this file)
- **Fiche Référence**: `docs/QUICK_REFERENCE.fr.md`
