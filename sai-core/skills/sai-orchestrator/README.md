# SAI Orchestrator

Orchestre le cycle plan → exec → verify → log du Système d'Amélioration Interne.

## Usage

```bash
~/.openclaw/skills/sai-orchestrator/orchestrate.sh \
  --mode project \
  --files 4 \
  --cmds 2 \
  --verify file1.sh,file2.json \
  --log-action "création orchestrateur" \
  --log-result "succès"
```

## Détection auto

Si `--mode` est omis, détection automatique via `lib/detect-mode.sh` :
- >3 fichiers ou >2 commandes → `project`
- Mots-clés détectés → `project`
- Sinon → `interactive`

## Pipeline

1. **detect-mode** : détermine le mode
2. **verify** : exécute `auto-verify/verify.sh` sur chaque fichier
3. **log** : écrit dans `memory/YYYY-MM-DD.md`

## Fichiers

- `orchestrate.sh` — script principal
- `lib/detect-mode.sh` — détection mode projet
- `lib/log.sh` — fonctions de logging
