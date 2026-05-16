# Service Discovery (mDNS)

## Protocole

Chaque instance annonce `_soulsystem._tcp` sur le réseau local.

## Détection

- Les nouveaux pairs sont détectés automatiquement
- La liste est publiée sur le bus interne
- SYNERGIE utilise cette liste pour l'apprentissage fédéré
