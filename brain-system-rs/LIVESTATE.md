# LIVESTATE — brain-system-rs (rustifié, v8.5)

> Dernière mise à jour : 2026-06-01 17:16 CEST

## Statut
- Port Rust du Python `brain-system/brain_v8.5/brain.py`
- Serveur API REST (axum) :8084
- Réseau LIF Hebbian avec persistance JSON

## Fonctionnalités
- ✅ API Status / Stimulus / Reset
- ✅ Réseau LIF (Leaky Integrate-and-Fire)
- ✅ Croissance Hebbian automatique
- ✅ Simulation loop (tick toutes les DT ms)
- ✅ Auto-save toutes les 30s (fichier JSON)
- 🔄 Manque : visualisation 3D Three.js, entraînement massif