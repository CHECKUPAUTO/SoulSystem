# SoulSystem 🦞

**SoulSystem** = SoulLink Neural Mesh + OpenClaw-U Autonomous Kernel

Architecture unifiée de l'écosystème agentique autonome.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    SoulSystem v1.0.0                         │
├─────────────────────────────────────────────────────────────┤
│  OpenClaw-U (Rust)        │  SoulLink Neural Mesh (Rust)    │
│  ─────────────────          │  ───────────────────────────    │
│  Kernel agent autonome      │  6 organes HNN (Hamiltonian)   │
│  • Perception temps réel    │  • Science, Mind, Engineer      │
│  • LLM dual (rapide+profond)│  • Crypto, Creative, Meta     │
│  • Auto-évolution v0.5.0  │  • 254K ticks/sec              │
│  • Q-Learning               │  • Verlet symplectique          │
│  • Méta-cognition           │  • Turbulence émergente         │
│  • Resilience + Self-mod    │                                  │
├─────────────────────────────────────────────────────────────┤
│  Ponts : Bi-Bridge HTTP :9051 │  HNN Bridge :9010-9015        │
│  Memory :9030 │  Orchestrator :9020 │  Chronos :9786          │
└─────────────────────────────────────────────────────────────┘
```

## Composants Principaux

### OpenClaw-U (Kernel Autonome)
Le cerveau opérationnel du système. Il gère la boucle de conscience (heartbeat), prend des décisions via LLM (Ollama) et exécute des actions concrètes.
- **Auto-évolution** : Capacité à modifier sa propre configuration runtime.
- **Resilience** : Détection de boucles d'échec et stratégies de fallback.
- **Apprentissage** : Q-Table pour optimiser les actions basées sur les récompenses passées.
- **Claudex Integration** : Agent de codage autonome (`/usr/local/bin/claudex`) intégré pour les tâches de développement complexes.

### SoulLink Neural Mesh (V13)
Le moteur de simulation neuronale basé sur la dynamique Hamiltonienne.
- **Organes** : Science, Mind, Engineer, Crypto, Creative, Meta.
- **HNN Mesh** : Simulation physique des interactions neuronales à haute fréquence.

## Sécurité et Réseau

Pour garantir l'intégrité du système, les composants critiques sont durcis :
- **Bi-Bridge** : Interface de contrôle liée exclusivement à `127.0.0.1:9051`.
- **HNN Mesh** : Ports `9010-9015` pour la communication interne.
- **Firewall** : Protection via `nftables` (voir `scripts/setup-firewall.sh`).
- **Isolation** : Les services sont gérés par `systemd` avec des restrictions de privilèges.

## Utilisation de Claudex

OpenClaw-U peut désormais déléguer des tâches de codage à Claudex.
Exemple de but : `claudex: optimiser le parsing des logs dans perception.rs`

## Modules

| Module | Langage | Port | Fonction |
|--------|---------|------|----------|
| `openclaw-u/` | Rust | 9051 | Kernel agent autonome |
| `soullink-brain/` | Rust | 9010-9020 | Cœur de SoulLink (Mesh + Orchestrateur) |
| `soullink-organs/` | Rust | — | Logique spécifique des organes |
| `configs/` | — | — | Systemd + configurations environnementales |
| `scripts/` | Shell | — | Automatisation build, deploy et firewall |

## Démarrage rapide

```bash
# Build de tous les composants
./scripts/build-all.sh

# Configuration du firewall
sudo ./scripts/setup-firewall.sh

# Déploiement et démarrage des services
./scripts/deploy.sh

# Vérification du status
./scripts/status.sh
```

## Version
- **SoulSystem**: 1.0.0
- **OpenClaw-U**: 0.5.0 (autonomie 10.0/10)
- **SoulLink**: V13 HNN v7.0
