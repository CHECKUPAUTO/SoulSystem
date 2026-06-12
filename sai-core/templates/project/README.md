# Nouveau Projet SAI

Projet créé avec le template SAI v1.0.

## Structure attendue

```
.
├── src/              # Code source
├── tests/            # Tests
├── docs/             # Documentation
├── memory/           # Journal SAI (YYYY-MM-DD.md)
└── .clawd-working-memory.json  # Mémoire active
```

## Commandes

```bash
# Vérifier un fichier
bash ~/.sai-core/skills/auto-verify/verify.sh src/monfichier.py

# Orchestrer une tâche
bash ~/.sai-core/skills/sai-orchestrator/orchestrate.sh --files 5 --cmds 2
```
