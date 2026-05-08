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

## Modules

| Module | Langage | Port | Fonction |
|--------|---------|------|----------|
| `openclaw-u/` | Rust | — | Kernel agent autonome |
| `soullink-v13/` | Rust | 9010-9015 | Neural mesh HNN |
| `soullink-orchestrator/` | Rust | 9020 | Orchestrateur mesh |
| `soullink-memory/` | Rust | 9030 | Store état |
| `soullink-chronos/` | Rust | 9786 | Scheduler |
| `soullink-voice/` | — | 9050 | Module voix |
| `soullink-v14/` | — | 9095 | V14 expérimental |
| `config/` | — | — | Systemd + env |
| `scripts/` | Shell | — | Build & deploy |

## Démarrage rapide

```bash
# Build tout
./scripts/build-all.sh

# Déployer
./scripts/deploy.sh

# Status
./scripts/status.sh
```

## Version
- **SoulSystem**: 1.0.0
- **OpenClaw-U**: 0.5.0 (autonomie 10.0/10)
- **SoulLink**: V13 HNN v7.0
