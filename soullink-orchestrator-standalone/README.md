# 🧠 SoulLink Orchestrateur v3 — Rust Native

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![Axum](https://img.shields.io/badge/axum-0.7-blue.svg)](https://github.com/tokio-rs/axum)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

> **Remplacement natif en Rust de l'orchestrateur Python SoulLink**

## 🚀 Vue d'ensemble

SoulLink Orchestrateur v3 est la réimplémentation en Rust du cœur de l'orchestration du mesh neural SoulLink. Il gère 6 cerveaux spécialisés (Science, Mind, Engineer, Crypto, Creative, Meta) avec une architecture ultra-performante et sans verrous.

## ✨ Caractéristiques principales

| Feature | Description |
|---------|-------------|
| 🎯 **Turbulence-Aware Routing** | Route intelligemment vers les cerveaux les plus stables (préfère `StableOrbit`) |
| ⚡ **Appels parallèles réels** | `tokio::join_all` pour les appels concurrents aux cerveaux |
| 🔓 **Registry lock-free** | `DashMap` pour un accès concurrent sans contention |
| 📊 **Métriques Prometheus** | Endpoint `/metrics` compatible Prometheus/Grafana |
| 🆕 **Auto-spawn** | Création dynamique de nouveaux cerveaux via API |
| 🪶 **Léger** | Utilise `minreq` au lieu de `reqwest` pour des temps de réponse optimaux |

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    SoulLink Orchestrateur                    │
│                         Port 9020                            │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │   Science   │  │    Mind     │  │  Engineer   │  ...     │
│  │   :9010     │  │   :9011     │  │   :9012     │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
│         │              │              │                      │
│         └──────────────┴──────────────┘                      │
│                    Axum + Tokio                               │
│              DashMap (lock-free)                              │
└─────────────────────────────────────────────────────────────┘
```

## 📦 Installation

### Prérequis

- Rust 1.70+ ([rustup.rs](https://rustup.rs/))
- Cerveaux SoulLink v12 en cours d'exécution (ports 9010-9015)

### Build

```bash
# Cloner le repo
git clone https://github.com/yourusername/soullink-orchestrator.git
cd soullink-orchestrator

# Build release (optimisé)
cargo build --release

# Binary: target/release/soullink-orchestrator
```

### Installation système

```bash
# Copier le binaire
sudo cp target/release/soullink-orchestrator /usr/local/bin/
sudo chmod +x /usr/local/bin/soullink-orchestrator

# Utilisateur système
sudo useradd -r -s /bin/false soullink

# Service systemd
sudo cp systemd/soullink-orchestrator.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now soullink-orchestrator
```

## 🎮 Usage

### Démarrage

```bash
# Par défaut: port 9020, brain_dir /mnt/nvme/soullink_brain
soullink-orchestrator

# Port personnalisé
soullink-orchestrator 9021

# Port + répertoire des cerveaux
soullink-orchestrator 9020 /custom/brain/path
```

### Endpoints API

| Méthode | Endpoint | Description |
|---------|----------|-------------|
| `GET` | `/` | Documentation API |
| `GET` | `/api/mesh/status` | Statut de tous les cerveaux |
| `GET` | `/api/mesh/turbulence` | Rapport de turbulence global |
| `POST` | `/api/mesh/query` | Requête intelligentes aux cerveaux |
| `POST` | `/api/mesh/think` | Tâche de réflexion |
| `POST` | `/api/mesh/reinforce` | Renforcer un concept |
| `POST` | `/api/mesh/stimulate` | Stimuler un module |
| `POST` | `/api/mesh/spawn` | Créer un nouveau cerveau |
| `GET` | `/api/mesh/brains` | Lister tous les cerveaux |
| `GET` | `/metrics` | Métriques Prometheus |

### Exemples

```bash
# Statut du mesh
curl http://localhost:9020/api/mesh/status | jq

# Query avec routing turbulence-aware
curl -X POST http://localhost:9020/api/mesh/query \
  -H "Content-Type: application/json" \
  -d '{"question": "explain quantum mechanics"}' | jq

# Renforcer un concept
curl -X POST http://localhost:9020/api/mesh/reinforce \
  -H "Content-Type: application/json" \
  -d '{"concept": "neural_networks", "delta": 0.1}' | jq

# Spawn un nouveau cerveau
curl -X POST http://localhost:9020/api/mesh/spawn \
  -H "Content-Type: application/json" \
  -d '{"domain": "medical", "speciality": ["anatomy", "diagnosis"]}' | jq
```

## 🔧 Configuration

### Variables d'environnement

| Variable | Description | Défaut |
|----------|-------------|--------|
| `RUST_LOG` | Niveau de log | `info` |
| `BRAIN_DIR` | Répertoire des cerveaux | `/mnt/nvme/soullink_brain` |

### Cerveaux par défaut

| Nom | Port | Spécialités |
|-----|------|-------------|
| `science` | 9010 | physics, math, chemistry, computation |
| `mind` | 9011 | neuroscience, language, philosophy, memory |
| `engineer` | 9012 | optimization, logic, algebra, engineering |
| `crypto` | 9013 | trading, blockchain, defi, finance |
| `creative` | 9014 | patterns, geometry, art, vision, design |
| `meta` | 9015 | learning, optimization, reinforcement |

## 📊 Monitoring

### Métriques Prometheus

```prometheus
# HELP soullink_queries_total Total number of queries processed
# TYPE soullink_queries_total counter
soullink_queries_total 15234

# HELP soullink_spawns_total Total number of brain spawns
# TYPE soullink_spawns_total counter  
soullink_spawns_total 3

# HELP soullink_brains_registered Number of registered brains
# TYPE soullink_brains_registered gauge
soullink_brains_registered 6
```

### Logs

```bash
# Journal système
sudo journalctl -u soullink-orchestrator -f

# Logs détaillés
RUST_LOG=debug soullink-orchestrator
```

## 🔄 Différences avec Python

| Aspect | Python (v2) | Rust (v3) |
|--------|-------------|-----------|
| Verrous | `threading.Lock` | `DashMap` (lock-free) |
| HTTP client | `urllib` | `minreq` (léger) |
| Framework | Flask | Axum |
| Async | Threading | Tokio (vrai async) |
| Performances | ~200 req/s | ~10k req/s estimé |
| Mémoire | ~50 MB | ~5 MB estimé |

## 🛠️ Développement

```bash
# Build dev
cargo build

# Run avec logs
cargo run

# Tests
cargo test

# Clippy (linting)
cargo clippy --all-targets --all-features

# Format
cargo fmt

# Release optimisé
cargo build --release
```

## 📁 Structure du projet

```
soullink-orchestrator/
├── Cargo.toml           # Configuration Rust
├── README.md            # Ce fichier
├── LICENSE              # MIT
├── src/
│   ├── main.rs          # Point d'entrée
│   ├── state.rs         # État global (AppState)
│   ├── models/
│   │   ├── mod.rs
│   │   └── types.rs     # Structures de données
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── index.rs
│   │   ├── status.rs
│   │   ├── query.rs
│   │   ├── turbulence.rs
│   │   ├── think.rs
│   │   ├── reinforce.rs
│   │   ├── stimulate.rs
│   │   ├── spawn.rs
│   │   ├── brains.rs
│   │   └── metrics.rs
│   └── utils/
│       ├── mod.rs
│       └── helpers.rs
└── systemd/
    └── soullink-orchestrator.service
```

## 🤝 Contribution

Les contributions sont les bienvenues ! Voir [CONTRIBUTING.md](CONTRIBUTING.md) pour les guidelines.

## 📝 License

MIT License - voir [LICENSE](LICENSE)

## 🔗 Liens

- [SoulLink Project](https://github.com/yourusername/soullink)
- [Documentation API](https://docs.soullink.dev)
- [Discord](https://discord.gg/soullink)

---

<p align="center">
  <sub>🦞 Construit avec Rust, caféine, et un peu de magie neuronale</sub>
</p>
