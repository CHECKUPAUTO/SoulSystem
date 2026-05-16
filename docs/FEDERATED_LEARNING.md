# Federated Learning

## Principe

Plusieurs instances SoulSystem échangent des gradients HNN signés
pour améliorer collectivement le modèle sans partager les données brutes.

## Sécurité

- Chaque instance possède une paire ed25519
- Les gradients sont signés avant envoi
- Seules les clés publiques autorisées sont acceptées

## Configuration

Dans `soulsystem.toml` :

```toml
[federated]
peers = ["192.168.1.10:9876", "192.168.1.11:9876"]
public_key = "base64..."
```

## Usage

Activer avec `--federated` au lancement.
