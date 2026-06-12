# Brain v8.5 — Persistance CORRIGÉE

Ce fichier contient le Brain v8.5 avec:
- Persistance corrigée (neurones et synapses sauvegardés)
- API `/api/stimulus` pour l'entraînement
- Interface 3D avec Three.js

## Installation

```bash
# Installer les dépendances
pip install flask numpy

# Télécharger Three.js
curl -sL "https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js" -o three.min.js

# Lancer le Brain
python brain.py --port 8084
```

## Entraînement

```bash
# Script d'entraînement de base
python brain_massive_training.py

# Script étendu (génération procédurale)
python brain_massive_extended.py
```

## Stats après entraînement

| Métrique | Valeur |
|----------|--------|
| Neurones | 33,955+ |
| Synapses | 118,638+ |
| Growth | 35,139+ |
| Fichier state | 24+ MB |

## Fichiers

- `brain.py` — Serveur Flask avec API
- `three.min.js` — Three.js pour l'interface (à télécharger)