# ADR-002 : Réseau Neuronal Hamiltonien (HNN) comme Noyau de Raisonnement

## Date
2026-05-26

## Contexte
Besoin d'un système de raisonnement capable de maintenir un état cohérent
dans le temps, contrairement aux LLMs purement stateless.

## Décision
Utiliser un réseau neuronal Hamiltonien (HNN) avec intégration Verlet pour
le noyau de raisonnement. Les équations Hamiltoniennes garantissent la
conservation de l'énergie dans l'espace latent, évitant les dérives.

## Conséquences
- Stabilité temporelle garantie mathématiquement
- Complexité de débogage accrue
- Nécessite un solveur ODE (DOPRI5 avec garde-fou MAX_REJECTIONS)

## Références
- Greydanus et al., "Neural Hamiltonian Networks", NeurIPS 2019
- Chen et al., "Neural Ordinary Differential Equations", NeurIPS 2018
