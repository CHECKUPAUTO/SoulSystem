## 🧠 RDEE — Research-Driven Ecosystem Evolution

### Vision
Chaque paper arXiv est une opportunité d'améliorer l'écosystème SoulLink/OpenClaw.

### Architecture (3 couches)

```
arXiv RSS (264 papers/jour)
    │
    ├─→ NOTIFIER (seuil 0.65)
    │       └─→ Telegram (papers "hot")
    │
    ├─→ GENERATOR (potentiel > 0.3)
    │       ├─→ Évalue potentiel outil
    │       ├─→ Génère Python
    │       ├─→ Anti-stub guard
    │       └─→ discovered_tools/ (~50-80/jour)
    │
    └─→ EVOLVER (impact écosystème)
            ├─→ Analyse quel composant
            ├─→ Identifie amélioration
            └─→ evolution/patches/ (~10-20/jour)
```

### Fichiers
- `skills/paper-to-tool-opportunistic.js` — Génération outils
- `skills/ecosystem-evolver.js` — Évolution écosystème
- `evolution/{reports,backups,patches,tools}/` — Stockage

### Configuration
- `config.json` — Jobs scheduler mis à jour
- Timer: toutes les 2h
- Modèle: qwen3-coder-next:cloud (rapide)

### Résultat attendu
- 264 papers/jour
- ~20 notifications
- ~50-80 outils
- ~10-20 améliorations

### Test
- 5 papers testés
- 3 impacts HNN détectés
- Temps: ~3s par paper (évaluation rapide)

### Prochaines étapes
1. Lancer batch sur 264 papers existants
2. Optimiser vitesse si nécessaire
3. Review manuel des améliorations critiques
