# SoulLink Brain System

> Système cérébral neuronal distribué avec visualisation 3D, apprentissage Hebbian, persistance, et entraînement massif

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    SOULLINK BRAIN SYSTEM v8.5                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐     │
│  │  Brain v8.5  │────▶│  TRIBE Brain │────▶│  MSA Server  │     │
│  │  (Port 8084)  │     │  (Port 7440) │     │  (Port 7430) │     │
│  │              │     │              │     │              │     │
│  │ Visualisation│     │ Intelligence │     │  Stockage    │     │
│  │ 3D Neurones  │     │ RAG + Memory │     │  Embeddings  │     │
│  │ ENTRAÎNEMENT │     │              │     │              │     │
│  └──────────────┘     └──────────────┘     └──────────────┘     │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │                    PERSISTENCE LAYER                      │    │
│  │  brain_state.json (24 MB)  │  Auto-save 30s  │  Backup   │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Nouveautés v8.5

### ✅ Persistance CORRIGÉE
- **AVANT**: Seulement les compteurs sauvegardés (114 bytes)
- **MAINTENANT**: Tous les neurones et synapses sauvegardés (24+ MB)
- Le nombre de neurones **AUGMENTE**, ne diminue jamais
- Restauration correcte au démarrage

### ✅ API Stimulus — Entraînement
- `POST /api/stimulus` — Injecte des connaissances
- Crée 1-10 neurones par stimulus selon l'intensité
- Crée des connexions automatiquement
- Encode le knowledge dans les neurones

### ✅ Entraînement Massif
- 18 domaines encyclopédiques
- 33,955+ neurones créés
- 118,638+ synapses
- Scripts: `brain_massive_training.py`, `brain_massive_extended.py`

### ✅ Interface 3D
- Three.js hébergé localement (pas de CDN)
- Visualisation temps réel de 33,955+ neurones
- Rotation automatique, couleurs par module
- HUD: neurones, synapses, spikes/s, growth

## Composants

### Brain v8.5 — Visualisation 3D + Entraînement (Port 8084)

```bash
python brain_v8.5/brain.py --port 8084
```

**Endpoints:**

| Endpoint | Description |
|----------|-------------|
| `GET /` | Interface 3D |
| `GET /api/brain` | État complet (neurones, modules, stats) |
| `GET /api/stats` | Stats rapides (N, syn, spk, hz, growth, sig) |
| `POST /api/stimulus` | Entraîner le Brain avec des connaissances |
| `GET /three.min.js` | Three.js local |

**Stimulus API:**

```bash
curl -X POST http://localhost:8084/api/stimulus \
  -H "Content-Type: application/json" \
  -d '{"module":"memory","intensity":5.0,"knowledge":"Your knowledge here"}'
```

### Brain v8.4 — Visualisation (Backup)

Version précédente sans API stimulus.

### TRIBE Brain (Port 7440)

Serveur intelligence avec RAG intégré:

```bash
python tribe/tribe_msa_server.py --port 7440
```

### MSA Server (Port 7430)

Stockage distribué des embeddings:

```bash
python tribe/tribe_msa_server.py --port 7430
```

## Entraînement

### Script de base

```bash
python brain_massive_training.py
```

Entraîne le Brain avec 700+ connaissances encyclopédiques:
- Sciences (physique, chimie, biologie, mathématiques)
- Technologies (IA, programmation)
- Humanités (histoire, philosophé, psychologie, économie)
- Arts (musique)
- Vie pratique (santé)

### Script étendu

```bash
python brain_massive_extended.py
```

Génère procéduralement jusqu'à 100 MB de connaissances.

## Persistance

```
/mnt/nvme/soullink_brain/
├── brain_state.json      # État complet (24+ MB)
├── neurons/              # Neurones par module
├── synapses/             # Synapses
└── backups/              # Backups automatiques
```

## Modules Cérébraux

| Module | Neurones | Fonction |
|--------|-----------|----------|
| perception | 48+ | Entrée sensorielle |
| memory | 66+ | Mémoire à long terme |
| reasoning | 45+ | Raisonnement logique |
| learning | 40+ | Apprentissage |
| attention | 30+ | Attention sélective |
| output | 35+ | Sortie motrice |
| language | 45+ | Traitement langage |
| vision | 38+ | Vision |
| audio | 30+ | Audio |
| motor | 28+ | Contrôle moteur |

## Stats après entraînement

| Métrique | Valeur |
|----------|--------|
| Neurones | 33,955+ |
| Synapses | 118,638+ |
| Growth | 35,139+ |
| Spikes/s | 4,180+ |
| Fichier state | 24+ MB |

## Paramètres LIF

```python
V_REST = -70.0    # Potentiel de repos (mV)
V_THRESH = -55.0  # Seuil de décharge (mV)
V_RESET = -75.0   # Potentiel de reset (mV)
TAU_M = 20.0      # Constante membranaire (ms)
T_REFRAC = 3.0    # Période réfractaire (ms)
DT = 0.5           # Pas de temps (ms)
```

## Licence

MIT

## Auteur

SoulLink Team