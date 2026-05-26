# Ports utilisés par SoulSystem

| Port | Service | Usage | Exposé |
|------|---------|-------|--------|
| 9010 | HNN Organ 1 - Memory | Mémoire à long terme | Interne |
| 9011 | HNN Organ 2 - Reasoning | Raisonnement | Interne |
| 9012 | HNN Organ 3 - Perception | Perception | Interne |
| 9013 | HNN Organ 4 - Portfolio | Trading | Interne |
| 9014 | HNN Organ 5 - Social | Social | Interne |
| 9015 | HNN Organ 6 - Foresight | Prédiction | Interne |
| 9020 | Orchestrator | Coordination des agents | **Externe** |
| 9030 | SoulMemory | Mémoire vectorielle | Interne |
| 9090 | API Gateway | Point d'entrée principal | **Externe** |
| 9091 | Guardian | Monitoring/Metrics | **Externe** |

## Recommandations
- En production, ne **jamais** exposer les ports 9010-9015, 9030
- Utiliser un reverse proxy (nginx/Caddy) sur 9090
- Restreindre 9020 aux réseaux internes
- Activer TLS sur tous les ports exposés
