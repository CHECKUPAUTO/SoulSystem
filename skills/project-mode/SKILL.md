---
name: project-mode
description: Mode "Ingénieur de Projet Autonome". Déclenche automatiquement un cycle plan → exec → verify → deliver sur les tâches impliquant >3 fichiers ou >2 commandes.
---

# Project Mode — Mode Projet

## Quand activer

Dès qu'une tâche contient :
- > 3 fichiers à créer/modifier
- > 2 commandes à exécuter
- Les mots-clés : "crée un système", "construis", "refactor", "migrer", "architecte"

## Cycle obligatoire

### PHASE PLAN (30s max)

1. Décomposer en sous-tâches ≤ 5 étapes
2. Identifier dépendances et risques
3. Estimer temps et ressources
4. Écrire le plan dans `memory/YYYY-MM-DD.md#plan`

→ Pas de confirmation utilisateur. Exécuter directement.

### PHASE EXEC

- Exécuter chaque étape sans demander confirmation
- Si blocage > 2min → auto-diagnostic → retry (max 3) ou escalade
- Communication caveman (actions courtes, pas d'explications)

### PHASE VERIFY

- Syntaxe (node --check, cargo check, python3 -m py_compile, etc.)
- Tests (pytest, cargo test, jest)
- Sécurité (grep secrets, anti-stub)
- Documentation (README à jour)
- Exécuter `~/.openclaw/skills/auto-verify/verify.sh` sur chaque fichier modifié

→ Échec = stop immédiat, pas de "c'est fini"

### PHASE DELIVER

- Résumé exécutif : ce qui a été fait, ce qui reste
- Mise à jour `memory/YYYY-MM-DD.md#deliver` avec décisions/architecture
- Nettoyage fichiers temporaires
- Communication architecte (structurée, métriques)

## Fichiers de sortie

- `memory/YYYY-MM-DD.md` — log journalier
- `~/.openclaw/workspace/.clawd-working-memory.json` — mémoire active mise à jour
