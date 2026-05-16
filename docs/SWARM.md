# Swarm — Essaims collaboratifs

## Architecture

Plusieurs instances SoulSystem s'auto-organisent :
1. Découverte mDNS
2. Élection d'un leader
3. Distribution de sous-tâches
4. Exécution distribuée (AVID + SciRust)
5. Agrégation des résultats

## Tolérance de panne

Si un pair ne répond pas dans le timeout, sa tâche est réassignée.
