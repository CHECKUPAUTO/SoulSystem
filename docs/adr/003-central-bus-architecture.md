# ADR-003 : Bus de Communication Central (soullink-bus)

## Date
2026-05-26

## Contexte
Les agents et services de SoulSystem doivent communiquer en temps réel.
Solutions évaluées : message queue (RabbitMQ/NATS), RPC direct, bus mémoire.

## Décision
Bus centralisé basé sur tokio mpsc channels avec interface pub/sub.
Tous les composants s'enregistrent sur le bus et échangent via messages typés.

## Conséquences
- Point de défaillance unique (mitigé par supervision)
- Faible latence (pas de sérialisation réseau)
- Couplage implicite entre producteurs et consommateurs

## Mitigations
- Orchestrateur supervise la santé du bus
- Timeout de 5s sur chaque message
- Fallback log file si bus indisponible
