<img width="1024" height="572" alt="image" src="https://github.com/user-attachments/assets/34fe5743-39cd-42bb-a81a-aef48b20aef4" />

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://img.shields.io/badge/AVID-Organisme%20Num%C3%A9rique%20Intelligent-8b5cf6?style=for-the-badge">
    <img src="https://img.shields.io/badge/AVID-Organisme%20Num%C3%A9rique%20Intelligent-7c3aed?style=for-the-badge" alt="AVID">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/CHECKUPAUTO/AVID/actions"><img src="https://img.shields.io/github/actions/workflow/status/CHECKUPAUTO/AVID/ci.yml?branch=main&style=flat-square&label=CI" alt="CI"></a>
  <a href="https://crates.io"><img src="https://img.shields.io/badge/rustc-1.88%2B-orange?style=flat-square&logo=rust" alt="Rust 1.88+"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT%20%7C%20Apache--2.0-blue?style=flat-square" alt="License"></a>
  <a href="SECURITY.md"><img src="https://img.shields.io/badge/security-policy-8b5cf6?style=flat-square" alt="Security"></a>
  <img src="https://img.shields.io/badge/status-production--ready-brightgreen?style=flat-square" alt="Status">
</p>

---

**AVID** (Autonomous Verification & Intelligent Development) est un **organisme numérique intelligent** qui explore le web, comprend des documents complexes, reconnaît des patterns, et crée des clones intelligents d'API innovantes à forte valeur ajoutée.

## Vision

AVID n'est pas un simple générateur de code. C'est un organisme numérique autonome capable de :

- **Explorer** le web pour découvrir des ressources, APIs, et innovations
- **Comprendre** des papers académiques, documentations techniques, notices d'utilisation
- **Reconnaître** des patterns d'architecture, de design, et de logique métier
- **Cloner intelligemment** des solutions innovantes en créant des implémentations équivalentes

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    AVID — Organisme Numérique                │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │  SCOUT      │  │  VISION     │  │  CORTEX     │         │
│  │  (Web)      │  │  (Patterns) │  │  (Compréhension)│     │
│  │             │  │             │  │             │         │
│  │ • Crawl     │  │ • UI comps  │  │ • Papers    │         │
│  │ • Extract   │  │ • Architecture│ • Docs      │         │
│  │ • Navigate  │  │ • Workflows │  │ • Notices   │         │
│  │ • Cache     │  │ • Patterns  │  │ • Articles  │         │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘         │
│         │                │                │                  │
│         └────────────────┼────────────────┘                  │
│                          │                                   │
│                   ┌──────▼──────┐                           │
│                   │   MIMIC     │                           │
│                   │  (Clonage)  │                           │
│                   │             │                           │
│                   │ • API       │                           │
│                   │ • Logic     │                           │
│                   │ • Data      │                           │
│                   │ • Code      │                           │
│                   └──────┬──────┘                           │
│                          │                                   │
│                   ┌──────▼──────┐                           │
│                   │  ORIGINAL   │                           │
│                   │  (Anti-clone)│                           │
│                   └──────┬──────┘                           │
│                          │                                   │
│                   ┌──────▼──────┐                           │
│                   │   FORGE     │                           │
│                   │ (Production)│                           │
│                   └─────────────┘                           │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Capacités

### 🔍 Scout — Exploration Web
- Visite automatique de sites web
- Extraction de contenu structuré
- Suivi de liens et navigation profonde
- Cache intelligent des pages visitées

### 👁️ Vision — Reconnaissance de Patterns
- Analyse de structures de pages web
- Identification de composants UI réutilisables
- Détection de patterns d'architecture
- Extraction de workflows métier

### 🧠 Cortex — Compréhension Sémantique
- Lecture et compréhension de papers académiques
- Analyse de documentations techniques
- Extraction de concepts clés
- Génération de résumés structurés

### 🎭 Mimic — Clonage Intelligent
- Analyse d'API innovantes
- Extraction de modèles de données
- Reconstruction de logique métier
- Génération de code équivalent

## Pipeline de Clonage

```
URL → Scout (fetch) → Vision (analyze) → Cortex (understand) → Mimic (clone) → Original (verify) → Forge (produce)

Example: API innovante
1. Scout visite la page de documentation de l'API
2. Vision identifie les endpoints, les modèles de données
3. Cortex comprend la logique métier, les flux de données
4. Mimic reconstruit l'API avec une architecture équivalente
5. Original vérifie l'originalité du code généré
6. Forge produit le code final prêt pour la production
```

## Crates

### avid-core
- **Agents** — Planner, CoreDesign, Critic
- **Orchestrator** — Coordination du pipeline
- **Memory** — Persistance SQLite WAL
- **LLM Client** — Ollama, OpenAI, Anthropic

### avid-scout (nouveau)
- **ScoutEngine** — Crawler web asynchrone
- **PageExtractor** — Extraction de contenu
- **LinkFollower** — Navigation profonde
- **ContentCache** — Cache intelligent

### avid-vision (nouveau)
- **VisionEngine** — Analyseur de structure
- **PatternDetector** — Détection de patterns
- **ComponentExtractor** — Extraction de composants
- **ArchitectureAnalyzer** — Analyse d'architecture

### avid-cortex (nouveau)
- **CortexEngine** — Moteur de compréhension
- **PaperReader** — Lecteur de papers
- **DocParser** — Parseur de documentations
- **KnowledgeExtractor** — Extracteur de connaissances

### avid-mimic (nouveau)
- **MimicEngine** — Moteur de clonage
- **APIAnalyzer** — Analyseur d'API
- **LogicExtractor** — Extracteur de logique
- **CodeGenerator** — Générateur de code

### avid-anticlone
- **AST Fingerprinting** — Empreinte structurelle
- **Node Histogram** — Distribution des nœuds
- **Call-graph Edges** — Graphe d'appels
- **Weighted Jaccard** — Similarité pondérée

### avid-sandbox
- **RLIMIT_CPU/AS/NPROC** — Limites ressources
- **PR_SET_NO_NEW_PRIVS** — Privilèges limités
- **Network Namespace** — Isolation réseau
- **Process Group Isolation** — Isolation processus

### avid-server
- **axum 0.7** — API HTTP
- **Queue** — Redis / SQLite
- **Metrics** — Prometheus
- **Healthz** — Health checks

## Quick Start

### Prerequisites

- **Rust** 1.88+ (via [rustup](https://rustup.rs))
- **Python 3** (pour l'exécution sandbox)
- **Ollama** (pour l'inférence LLM)

### Installation

```bash
git clone https://github.com/CHECKUPAUTO/AVID.git
cd AVID
./install.sh
```

### Configuration

```bash
# .env
OLLAMA_URL=http://localhost:11434
OLLAMA_MODEL=deepseek-v4-pro:cloud
SCOUT_DEPTH=3
VISION_THRESHOLD=0.85
CORTEX_MAX_TOKENS=4096
MIMIC_ORIGINALITY_MIN=0.7
```

### Usage

```bash
# Démarrer le serveur
cargo run --bin avid-server

# Soumettre une tâche de clonage
curl -X POST http://localhost:3000/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://api.example.com/docs",
    "type": "api_clone",
    "depth": 2
  }'
```

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/tasks` | POST | Soumettre une tâche de clonage |
| `/tasks/{id}` | GET | Récupérer le statut d'une tâche |
| `/tasks/{id}/result` | GET | Récupérer le résultat |
| `/scout` | POST | Lancer une exploration web |
| `/vision` | POST | Analyser une page web |
| `/cortex` | POST | Comprendre un document |
| `/mimic` | POST | Cloner une API |
| `/healthz` | GET | Health check |
| `/metrics` | GET | Métriques Prometheus |

## Security

- `forbid(unsafe_code)` sur 3 crates
- RLIMIT ressources dans sandbox
- Isolation réseau (namespace)
- Vérification d'originalité AST

## License

MIT | Apache-2.0

## Fuzzing

AVID utilise `cargo-fuzz` pour le fuzzing des parseurs.

```bash
# Installer cargo-fuzz
cargo install cargo-fuzz

# Lancer le fuzz de l'analyseur arXiv
cargo +nightly fuzz run fuzz_arxiv_parser

# Lancer le fuzz du parseur web
cargo +nightly fuzz run fuzz_web_parser
```

Les cibles sont dans `fuzz/fuzz_targets/`.
