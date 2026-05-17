# SoulSystem Operator Edition — Guide Opérateur

## Architecture

SoulSystem est un écosystème modulaire d'agent autonome en Rust.

### Modules actifs (après restructuration Operator Edition)

| Module | Rôle |
|--------|------|
| `audit_log` | Journal d'audit signé avec vérification d'intégrité |
| `bus` | Système de messagerie interne (broadcast channel) |
| `code_signing` | Certification des logs d'audit (clé HMAC-SHA256) |
| `compute_backend` | Abstraction CPU/GPU — détection CUDA/ROCm/Vulkan |
| `config` | Configuration centralisée (TOML + env vars) |
| `discovery` | Découverte de services mDNS |
| `dev_dashboard` | Dashboard SSE sur :9090 (feature `dev`) |
| `anomaly` | Détecteur de chute de ticks HNN (feature `dev`) |
| `soul_memory` | Mémoire vectorielle locale (sled + n-grammes) |
| `telemetry` | Métriques distribuées (OTLP) |

### Modules déplacés vers `backlog/`

soul_wallet, swarm, hardware_autoscaler, skill_marketplace, skill_api,
federated_learning, meta_learning, jit_hnn, sandbox/nix

## Installation

```bash
cargo build --release
sudo cp target/release/soulsystem /usr/local/bin/
```

Pour le mode développement (dashboard + anomaly) :

```bash
cargo build --release --features dev
```

## Configuration

Fichier `soulsystem.toml` :

```toml
[paths]
config_dir = "/opt/soulsystem/config"
data_dir   = "/var/lib/soulsystem/data"
log_dir    = "/var/log/soulsystem"
```

Surcharge possible via variables d'environnement :
- `SOULSYSTEM_CONFIG_FILE` — chemin alternatif vers le fichier de config
- `QDRANT_URL` — endpoint Qdrant (optionnel, fallback sled local)
- `OTEL_EXPORTER_OTLP_ENDPOINT` — endpoint télémétrie (défaut :4317)

## Utilisation

```bash
# Démarrage normal
soulsystem

# Mode développement (dashboard web :9090 + anomaly detection)
soulsystem --dev

# Mode mock (simulation)
soulsystem --mock

# Version
soulsystem --version
```

## Supervision

- **Dashboard** : `http://localhost:9090` (nécessite `--dev`)
- **Bus** : les modules peuvent s'abonner au bus pour recevoir les alertes
- **Anomaly** : détection de chutes > 40% du tick rate HNN, cooldown 60s

## Backup

```bash
# Backup des données
tar -czf soulsystem-backup-$(date +%Y%m%d).tar.gz /var/lib/soulsystem/ /opt/soulsystem/config/
```

## Dépannage

| Problème | Cause possible | Solution |
|----------|---------------|----------|
| `soulmemory: QDRANT_URL not set` | Pas de Qdrant | Normal, utilise sled local |
| `mDNS non disponible` | `mdns-sd` pas installé | Mode registre local uniquement |
| Dashboard inaccessible | `--dev` pas activé | Recompiler avec `--features dev` |
