# SoulSystem 🦞

**SoulSystem** = Unified Autonomous Ecosystem: SoulLink HNN Mesh + OpenClaw-U Kernel + Clawd Assistant + AVID Engineering

*Last updated: 2026-06-09*

---

## Architecture Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                      SoulSystem Unified Monorepo v0.6.0              │
├──────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────────┐    ┌──────────────────────────────────────┐ │
│  │   OpenClaw-U (Rust) │    │     SoulLink Neural Mesh (Rust)      │ │
│  │   Autonomous Kernel  │    │     6 HNN organs v7.0               │ │
│  │   • Perception       │    │     • Science  (9010) • Mind (9011) │ │
│  │   • LLM dual-mode    │    │     • Engineer (9012) • Crypto      │ │
│  │   • Auto-evolution   │    │     • Creative (9014) • Meta (9015) │ │
│  │   • Q-Learning       │    │     • 254K ticks/sec                │ │
│  │   • Meta-cognition   │    │     • Verlet symplectic             │ │
│  │   • Resilience       │    │     • Emergent turbulence           │ │
│  └─────────┬────────────┘    └───────────────┬──────────────────────┘ │
│            │                                  │                         │
│  ┌─────────┴──────────────────────────────────┴──────────────────────┐│
│  │                       Infrastructure                               ││
│  │  Orchestrator :9020 │ Memory :9030 │ Chronos :9786 │ v14 :9095    ││
│  │  Ollama :11434 (57+ models) │ nftables firewall │ OwnCloud :8080 ││
│  │  GPU: RTX 4060 8GB │ RAM: 125GB │ Debian 6.12 │ NVMe RAID        ││
│  └──────────────────────────────────────────────────────────────────┘│
│                                                                      │
│  ┌─────────────────────┐    ┌──────────────────────────────────────┐ │
│  │   AVID Engineering   │    │     Clawd Assistant                  │ │
│  │   (12 Rust crates)   │    │     • Primary Telegram agent         │ │
│  │   • TokenJuice        │    │     • Self-evolving skills          │ │
│  │   • Model Routing     │    │     • Wiki + Persistent memory      │ │
│  │   • Scout (753 mods)  │    │     • Reflection Loop               │ │
│  │   • 827 files, 12     │    │     • BOUND System                  │ │
│  │     compiled crates   │    │                                      │ │
│  └─────────────────────┘    └──────────────────────────────────────┘ │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Clawd — Personal Assistant
- **Nature**: Native OpenClaw agent, direct Telegram interface
- **Capabilities**: LLM reasoning, code execution, system management, web scraping, video analysis
- **Model**: `deepseek-v4-pro:cloud` (Ollama)
- **Skills**: 40+ installed (1password, github, xurl, himalaya, obsidian, etc.)
- **Memory**: MEMORY.md + daily logs + wiki/index.md + Reflection Loop
- **Autonomy**: Silent heartbeat, auto-maintenance, skill crystallization L3
- **Security**: BOUND System (approvals required for external actions)

### 2. SoulLink Neural Mesh (V13 → V14)
- **Engine**: HNN v7.0 — Hamiltonian Neural Network, Verlet symplectic dynamics
- **6 organs**: Science, Mind, Engineer, Crypto, Creative, Meta (all HTTP 200)
- **Energy surface**: U(q) = α(q-μ)² + β(q-μ)⁴
- **Performance**: 254K ticks/sec, energy conservation verified (drift < 0.005/5000 steps)
- **Attractors**: DeepBasin, StableOrbit, StrangeAttractor, Transient
- **Services**: 50+ active systemd services, monitored via nftables + cron

### 3. OpenClaw-U — Autonomous Kernel
- **Port**: `:9051` (Bi-Bridge HTTP)
- **Modules**: Perception, Action, Memory, HNN Bridge, ONAEU Bridge
- **Auto-evolution**: v0.5.0, runtime self-configuration capability
- **Learning**: Q-Table for action optimization
- **Integrations**: Claudex (coding agent), Chronos (timeline), GBrain (knowledge graph)

### 4. Autonomous Entity (v0.2.0)
- **Core**: `soul-agent-core` — ReAct loop (observe→think→act→evaluate)
- **LLM**: `soul_llm` — ChatSession, streaming, native Ollama tool calling
- **Planning**: `soul_planner` — LLM-powered goal decomposition
- **Tools**: `soul_tools` — Async shell, file ops, permissions (Read/Write/Destructive)
- **Interface**: `soul_repl` — Conversational REPL with real-time streaming
- **Safety**: Automatic destructive command blocking, safety warnings at turns 7/10/15/25/35/50
- **Self-evolution**: Memory distillation (task → persistent learnings)
- **Launch**: `cargo run -p soul_repl --release` or `cargo run --bin soulsystem -- --repl`

### 5. AVID — Digital Organism (Rust)
- **12 crates**: anticlone, cli, core, cortex, mimic, orchestrator, sandbox, scout, server, tokenjuice, tui, vision
- **827 Rust files**, release build OK (2 min)
- **Pipeline**: Planner → CoreDesign → Critic → AntiClone → Sandbox
- **TokenJuice**: 96 compaction rules for CLI tools (git, docker, cargo, npm...)
- **Model Routing**: Task classification → local/remote dispatch (hint:* system)
- **Scout**: 753 web extraction modules (largest open-source scraping engine)
- **GBrain**: Integrated Knowledge Graph, hybrid search (vector + text)

### 6. Research Organ (arXiv)
- **Sources**: 10 arXiv feeds (cs.AI, cs.LG, cs.CL, cs.CR, cs.CV, cs.RO, stat.ML, cs.NE, cs.SD, HN)
- **Volume**: ~300 papers/day collected
- **Pipeline**: RSS → LLM analysis → SoulLink tool → JSON wrapper → ecosystem evolution
- **Anti-Stub Guard**: Rust binary that verifies generated code is not a stub

## Infrastructure

### Physical Server
| Resource | Spec |
|-----------|------|
| OS | Debian 12, kernel 6.12.74 |
| CPU | AMD Ryzen (x86_64) |
| RAM | 125 GB (18 GB used) |
| GPU | NVIDIA RTX 4060, 8 GB VRAM (646 MB used, 36°C) |
| Storage | NVMe RAID — 179G/915G root (21%), 54% secondary NVMe |
| Network | 192.168.0.26, nftables strict firewall |

### Key Services (50+ active)
| Service | Port | Role |
|---------|------|------|
| Ollama | 11434 | LLM server (57+ models) |
| Apache/OwnCloud | 80, 443, 8080, 777 | Personal cloud |
| SoulLink Orchestrator | 9020 | Central coordination |
| SoulLink Memory | 9030 | Memory base (N=800, 1 Hz) |
| SoulLink Chronos | 9786 | Timeline & planning |
| SoulLink v14 | 9095 | Evolution engine |
| OpenClaw-U | 9051 | Autonomous kernel |
| OpenClaw Gateway | 18890 | Agent gateway |
| Research Agent | — | 24/7 arXiv monitoring |
| TurboQuant | 11435 | Ollama proxy + watch |
| SoulLink GBrain | — | Knowledge Graph + hybrid search |
| Cloudflared | — | Tunnel for external Ollama |

### Security
- **Firewall**: nftables, port whitelist only (22, 80, 443, 8080, 777, 9010-9015, 9020, 9030, 9051, 9095, 9786, 11434, 18890)
- **Port Guard**: Cron every minute — kills any non-Apache process on OwnCloud ports
- **Tokens**: None hardcoded, all in environment variables (600 permissions)
- **Isolation**: AVID sandbox with rlimits, no_new_privs

## Communication & Messaging

- **Clawd ↔ Tarek**: Direct Telegram, 1:1
- **Clawd ↔ SoulLink**: inter-agent sessions_send
- **Jules (Google Labs)**: Cloud coding agent, GitHub Actions workflows
- **GitHub CHECKUPAUTO**: 20+ public repos

## Roadmap

See `ROADMAP.md` for the detailed evolution history.

## Quick Start

```bash
# Build all components
cd /root/SoulSystem && ./scripts/build-all.sh

# Configure firewall
sudo ./scripts/setup-firewall.sh

# Deploy and start services
./scripts/deploy.sh

# Check status
./scripts/status.sh
```

## Versions
- **SoulSystem**: v13.5.0
- **OpenClaw-U**: v0.5.0
- **SoulLink HNN**: v7.0 (V13 Mesh → V14 emerging)
- **AVID**: v0.1.0 (12 crates, pre-production)
- **Clawd**: Primary agent (continuous evolution)

## License
MIT OR Apache-2.0

---

*This document is live — automatically updated by Clawd on every significant ecosystem change.*