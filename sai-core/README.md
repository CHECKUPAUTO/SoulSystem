# SAI — Système d'Amélioration Interne v1.0

[![SAI Auto-Verify](https://github.com/CHECKUPAUTO/sai-core/actions/workflows/verify.yml/badge.svg)](https://github.com/CHECKUPAUTO/sai-core/actions/workflows/verify.yml)

Transforme le fonctionnement d'un assistant interactif en mode **Ingénieur de Projet Autonome**.

## Quick Start

```bash
# Installer
curl -sL https://raw.githubusercontent.com/CHECKUPAUTO/sai-core/main/install.sh | bash

# Ou manuellement
git clone https://github.com/CHECKUPAUTO/sai-core.git ~/.sai-core
cd ~/.sai-core && make install

# Vérifier un fichier
make verify

# Tests
make test

# Orchestrer une tâche
bash skills/sai-orchestrator/orchestrate.sh --files 4 --cmds 2 --verify file1.py,file2.py
```

## Architecture

| Couche | Fichier | Rôle |
|--------|---------|------|
| **Auto-Verify** | `skills/auto-verify/verify.sh` | Pipeline syntaxe → stubs → secrets → hallucination |
| **Project Mode** | `skills/project-mode/SKILL.md` | Cycle plan → exec → verify → deliver |
| **Reflection Stack** | `skills/reflection-stack/SKILL.md` | Métacognition pré / intra / post-action |
| **Caveman Coding** | `skills/caveman-coding/SKILL.md` | Communication caveman (exec) vs architecte (deliver) |
| **Orchestrator** | `skills/sai-orchestrator/` | Glue bash : detect-mode + verify + log |
| **Working Memory** | `workspace/.clawd-working-memory.json` | Mémoire active, contraintes, leçons |
| **Journal** | `memory/YYYY-MM-DD.md` | Log journalier plan/exec/verify/reflection/deliver |

## Règles de déclenchement

Mode projet s'active si :
- > 3 fichiers impliqués
- > 2 commandes à exécuter
- Mots-clés : "crée un système", "construis", "refactor", "migrer", "architecte"

## Développement

```bash
cd ~/.sai-core
make help        # Liste les commandes
make test        # Tests d'intégration
make verify      # Auto-verify sur tous les fichiers
make lint        # Vérifie le workflow CI
make template    # Crée un nouveau projet SAI
```

## CI

Le workflow `.github/workflows/verify.yml` s'exécute sur chaque push/PR vers `main`.

## Principe

> Safety > Speed — Pas de confirmation intermédiaire en mode projet — Français uniquement
