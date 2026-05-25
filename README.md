# SoulSystem 🦞

**SoulSystem** = Écosystème autonome unifié : SoulLink HNN Mesh + OpenClaw-U Kernel + Clawd Assistant + AVID Engineering

*Dernière mise à jour : 2026-05-11*

---

## Architecture Globale

```
┌──────────────────────────────────────────────────────────────────────┐
│                         SoulSystem Unified Monorepo v0.6.0                              │
├──────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  ┌─────────────────────┐    ┌──────────────────────────────────────┐ │
│  │   OpenClaw-U (Rust) │    │     SoulLink Neural Mesh (Rust)       │ │
│  │   Kernel autonome   │    │     6 organes HNN v7.0                │ │
│  │   • Perception       │    │     • Science    (9010)   • Mind (9011)│ │
│  │   • LLM dual-mode    │    │     • Engineer   (9012)   • Crypto    │ │
│  │   • Auto-évolution   │    │     • Creative   (9014)   • Meta (9015)│ │
│  │   • Q-Learning       │    │     • 254K ticks/sec                  │ │
│  │   • Méta-cognition   │    │     • Verlet symplectique             │ │
│  │   • Resilience       │    │     • Turbulence émergente            │ │
│  └─────────┬────────────┘    └───────────────┬──────────────────────┘ │
│            │                                  │                         │
│  ┌─────────┴──────────────────────────────────┴──────────────────────┐│
│  │                       Infrastructure                               ││
│  │  Orchestrator :9020 │ Memory :9030 │ Chronos :9786 │ v14 :9095    ││
│  │  Ollama :11434 (57+ modèles) │ nftables firewall │ OwnCloud :8080 ││
│  │  GPU: RTX 4060 8GB │ RAM: 125GB │ Debian 6.12 │ NVMe RAID        ││
│  └──────────────────────────────────────────────────────────────────┘│
│                                                                        │
│  ┌─────────────────────┐    ┌──────────────────────────────────────┐ │
│  │   AVID Engineering   │    │     Clawd Assistant                   │ │
│  │   (12 crates Rust)   │    │     • Agent Telegram principal        │ │
│  │   • TokenJuice        │    │     • Skills auto-évolutifs           │ │
│  │   • Model Routing     │    │     • Wiki + Mémoire persistante      │ │
│  │   • Scout (753 mod.)  │    │     • Reflection Loop                 │ │
│  │   • 827 fichiers, 12  │    │     • BOUND System                    │ │
│  │     crates compilés   │    │                                        │ │
│  └─────────────────────┘    └──────────────────────────────────────┘ │
│                                                                        │
└──────────────────────────────────────────────────────────────────────┘
```

---

## Composants Principaux

### 1. Clawd — Assistant Personnel
- **Nature** : Agent OpenClaw natif, interface Telegram directe
- **Capacités** : Raisonnement LLM, exécution de code, gestion système, web scraping, analyse de vidéos
- **Modèle** : `deepseek-v4-pro:cloud` (Ollama)
- **Compétences** : 40+ skills installés (1password, github, xurl, himalaya, obsidian, etc.)
- **Mémoire** : MEMORY.md + daily logs + wiki/index.md + Reflection Loop
- **Autonomie** : Heartbeat silencieux, maintenance auto, skill crystallization L3
- **Sécurité** : BOUND System (approvals requis pour actions externes)

### 2. SoulLink Neural Mesh (V13 → V14)
- **Moteur** : HNN v7.0 — Hamiltonian Neural Network, dynamique Verlet symplectique
- **6 organes** : Science, Mind, Engineer, Crypto, Creative, Meta (tous HTTP 200)
- **Surface d'énergie** : U(q) = α(q-μ)² + β(q-μ)⁴
- **Performance** : 254K ticks/sec, conservation d'énergie confirmée (dérive < 0.005/5000 pas)
- **Attracteurs** : DeepBasin, StableOrbit, StrangeAttractor, Transient
- **Services** : 50+ services systemd actifs, monitoring via nftables + cron

### 3. OpenClaw-U — Kernel Autonome
- **Port** : `:9051` (Bi-Bridge HTTP)
- **Modules** : Perception, Action, Memory, HNN Bridge, ONAEU Bridge
- **Auto-évolution** : v0.5.0, capacité à modifier sa propre config runtime
- **Apprentissage** : Q-Table pour optimisation des actions
- **Intégrations** : Claudex (agent codage), Chronos (timeline), GBrain (knowledge graph)

### 4. AVID — Organisme Numérique (Rust)
- **12 crates** : anticlone, cli, core, cortex, mimic, orchestrator, sandbox, scout, server, tokenjuice, tui, vision
- **827 fichiers Rust**, compilation release OK (2 min.)
- **Pipeline** : Planner → CoreDesign → Critic → AntiClone → Sandbox
- **TokenJuice** : 96 règles de compaction pour outils CLI (git, docker, cargo, npm…)
- **Model Routing** : Classification task → local/remote dispatch (hint:* system)
- **Scout** : 753 modules d'extraction web (le plus gros moteur de scraping open-source)
- **GBrain** : Knowledge Graph intégré, recherche hybride (vectorielle + texte)

### 5. Organe de Recherche (arXiv)
- **Sources** : 10 flux arXiv (cs.AI, cs.LG, cs.CL, cs.CR, cs.CV, cs.RO, stat.ML, cs.NE, cs.SD, HN)
- **Volume** : ~300 papers/jour collectés
- **Pipeline** : RSS → LLM analyse → outil SoulLink → wrapper JSON → ecosystem evolution
- **Anti-Stub Guard** : Binaire Rust qui vérifie que le code généré n'est pas un stub

---

## Infrastructure

### Serveur Physique
| Ressource | Spec |
|-----------|------|
| OS | Debian 12, kernel 6.12.74 |
| CPU | AMD Ryzen (x86_64) |
| RAM | 125 GB (18 GB utilisés) |
| GPU | NVIDIA RTX 4060, 8 GB VRAM (646 MB utilisés, 36°C) |
| Stockage | NVMe RAID — 179G/915G root (21%), 54% NVMe secondaire |
| Réseau | 192.168.0.26, nftables firewall strict |

### Services Clés (50+ actifs)
| Service | Port | Rôle |
|---------|------|------|
| Ollama | 11434 | Serveur LLM (57+ modèles) |
| Apache/OwnCloud | 80, 443, 8080, 777 | Cloud personnel |
| SoulLink Orchestrator | 9020 | Coordination centrale |
| SoulLink Memory | 9030 | Base mémoire (N=800, 1 Hz) |
| SoulLink Chronos | 9786 | Timeline & planification |
| SoulLink v14 | 9095 | Evolution engine |
| OpenClaw-U | 9051 | Kernel autonome |
| OpenClaw Gateway | 18890 | Gateway agentique |
| Research Agent | — | Veille arXiv 24/7 |
| TurboQuant | 11435 | Proxy + watch Ollama |
| SoulLink GBrain | — | Knowledge Graph + recherche hybride |
| Cloudflared | — | Tunnel pour Ollama externe |

### Sécurité
- **Firewall** : nftables, whitelist ports uniquement (22, 80, 443, 8080, 777, 9010-9015, 9020, 9030, 9051, 9095, 9786, 11434, 18890)
- **Port Guard** : Cron toutes les minutes — kill tout processus non-Apache sur les ports OwnCloud
- **Tokens** : Aucun en dur, tout dans variables d'environnement (600)
- **Isolation** : Sandbox AVID avec rlimits, no_new_privs

---

## Communication & Messaging

- **Clawd ↔ Tarek** : Telegram direct, 1:1
- **Clawd ↔ SoulLink** : sessions_send inter-agents
- **Jules (Google Labs)** : Coding agent cloud, workflows GitHub Actions
- **GitHub CHECKUPAUTO** : 20+ repos publics

---

## Roadmap (EVOLUTIONS.md)

Voir `EVOLUTIONS.md` pour le détail des évolutions planifiées :
1. Zero-Copy IPC (shared memory)
2. Dynamic VRAM Management
3. Distributed Mesh (mDNS)
4. Automated Fine-Tuning Pipeline
5. Hierarchical Memory (épisodique vs sémantique)
6. Mixture of Experts (MoE)
7. TUI Visualizer (`soul-top`)
8. Chaos Testing (`soul-chaos`)
9. Interactive CLI (`soul-shell`)

---

## Démarrage Rapide

```bash
# Build de tous les composants
cd /root/SoulSystem && ./scripts/build-all.sh

# Configuration du firewall
sudo ./scripts/setup-firewall.sh

# Déploiement et démarrage des services
./scripts/deploy.sh

# Vérification du status
./scripts/status.sh
```

## Versions
- **SoulSystem** : v2.0.0
- **OpenClaw-U** : v0.5.0
- **SoulLink HNN** : v7.0 (V13 Mesh → V14 émergent)
- **AVID** : v0.1.0 (12 crates, pré-production)
- **Clawd** : Agent principal (évolution continue)

---

*Ce document est vivant — mis à jour automatiquement par Clawd à chaque changement significatif de l'écosystème.*
