# Audit Log

Journal immuable de toutes les décisions autonomes.

## Structure

Chaque entrée contient :
- Timestamp
- Module émetteur
- Action effectuée
- Détails
- Hash de l'entrée précédente
- Signature ed25519

## API

```
GET /audit — Liste toutes les entrées
GET /audit/verify — Vérifie l'intégrité de la chaîne
```
