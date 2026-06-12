# OpenClaw Gateway 🦀

Gateway OpenClaw réécrit en Rust — Haute performance, faible empreinte mémoire.

## Architecture

```
┌─────────────────────────────────────────────┐
│           OpenClaw Gateway (Rust)           │
├─────────────────────────────────────────────┤
│  Axum (HTTP/WebSocket)  │  Protocol v3     │
├─────────────────────────────────────────────┤
│  Auth Manager (DashMap) │  Session Manager │
├─────────────────────────────────────────────┤
│      Providers: Telegram │ WhatsApp        │
└─────────────────────────────────────────────┘
```

## Démarrage rapide

```bash
# Compilation
cargo build --release

# Exécution
GATEWAY_TOKEN=your_token cargo run
```

## Configuration

| Variable | Description | Défaut |
|----------|-------------|--------|
| `PORT` | Port d'écoute | 18889 |
| `GATEWAY_TOKEN` | Token d'authentification statique | - |
| `RUST_LOG` | Niveau de log | info |
| `ENABLE_TELEGRAM` | Activer Telegram | false |
| `ENABLE_WHATSAPP` | Activer WhatsApp | false |

## Endpoints

- `GET /health` — Health check
- `GET /status` — Statut du gateway
- `WS /ws` — WebSocket pour connexions temps réel

## Protocole v3

Le gateway implémente le protocole OpenClaw v3 avec messages JSON-RPC 2.0-like :

```json
// Request
{"id":"1","method":"connect","params":{"..."}}

// Response
{"id":"1","ok":true,"payload":{"..."}}
```

## Génération

Pour régénérer le projet complet :

```bash
./generate.sh
```

## License

MIT
