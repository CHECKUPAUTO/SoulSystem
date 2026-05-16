# Meta-Learning

OpenEvolve optimise périodiquement les hyperparamètres du HNN Mesh
et les stratégies de prompt de Clawd.

## Fonctionnement

1. Extraction des hyperparamètres via le trait `Optimizable`
2. Génération d'une population de configurations
3. Évaluation (vitesse de convergence, précision)
4. Application de la meilleure configuration

## Périodicité

Par défaut : toutes les 6 heures. Configurable via
`META_EVOLUTION_INTERVAL_SECS`.
