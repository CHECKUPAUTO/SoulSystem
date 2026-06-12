---
name: sai
description: |
  SAI — Système d'Amélioration Interne v2.0.
  Transforme un agent interactif en Ingénieur de Projet Autonome.
  Inclut auto-verify, project-mode, reflection-stack, caveman-coding,
  orchestrateur, working-memory, et CI GitHub Actions.
version: "2.0.0"
author: CHECKUPAUTO
license: MIT
---

# SAI v2.0 — Système d'Amélioration Interne

## Activation

Copiez ce repo dans `~/.sai-core/` et exécutez :

```bash
curl -sL https://raw.githubusercontent.com/CHECKUPAUTO/sai-core/main/install.sh | bash
```

## Architecture v2

```
SAI/
├── SKILL.md                    ← Ce fichier (skill maître)
├── skills/
│   ├── auto-verify/verify.sh   ← Pipeline de vérification
│   ├── project-mode/SKILL.md   ← Cycle plan/exec/verify/deliver
│   ├── reflection-stack/SKILL.md ← Métacognition 3 couches
│   ├── caveman-coding/SKILL.md   ← Règles de communication
│   └── sai-orchestrator/       ← Glue bash (detect + verify + log)
├── workspace/
│   └── .clawd-working-memory.json ← Mémoire active
├── memory/
│   └── YYYY-MM-DD.md           ← Journal journalier
├── tests/integration/          ← Tests end-to-end
├── .githooks/pre-commit        ← Hook auto-verify
├── .github/workflows/verify.yml ← CI GitHub Actions
├── Makefile                    ← Commandes de développement
├── install.sh                  ← Setup one-liner
└── templates/project/          ← Template de projet SAI
```

## Cycle de vie d'une tâche SAI

```
Utilisateur demande une tâche
         ↓
    detect-mode.sh
    → project ? → plan → exec → verify → deliver
    → interactive ? → réponse directe
         ↓
    verify.sh (syntaxe, stubs, secrets, hallucination)
         ↓
    log.sh → memory/YYYY-MM-DD.md
         ↓
    working-memory.json mis à jour
```

## Contraintes globales

- Langue : français uniquement
- Pas de confirmation intermédiaire en mode projet
- Safety > Speed toujours
- Auto-retry max 3 sur échec
- Jamais de silence > 2min sans update

## Commandes

```bash
cd ~/.sai-core
make help      # Aide
make install   # Hooks + dépendances
make test      # 8 tests d'intégration
make verify    # Auto-verify tous les fichiers
make lint      # YAML CI
make clean     # Nettoyage
make template  # Nouveau projet SAI
```

## CI

[![SAI Auto-Verify](https://github.com/CHECKUPAUTO/sai-core/actions/workflows/verify.yml/badge.svg)](https://github.com/CHECKUPAUTO/sai-core/actions/workflows/verify.yml)

## Version

v2.0.0 — Système complet avec tests, hooks, Makefile, install, template, et CI.
