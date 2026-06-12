# OS-AGENTS (Soul System)

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-blue.svg)](https://www.rust-lang.org)

**OS-AGENTS**, également connu sous le nom de **soul_system**, est un framework de système d'exploitation cognitif et multi-agents ultra-performant écrit en Rust. Il est conçu pour orchestrer des agents intelligents avec une latence minimale, en utilisant une architecture hybride combinant un noyau de calcul vectorisé (Runtime) et une couche neuro-cognitive avancée.

## 🚀 Vision du Projet

Le projet vise à fournir une infrastructure robuste pour des agents capables de :
- **Percevoir** leur environnement via des pipelines zero-copy.
- **Raisonner** à travers un cortex récurrent et des moteurs de graphes neuronaux.
- **Interagir** via un bus IPC (Inter-Agent Bus) ultra-rapide et des protocoles de cluster UDP.
- **S'auto-réguler** grâce à un pare-feu sémantique et des mécanismes d'auto-réparation.

## 📋 Table des Matières

- [Fonctionnalités Principales](#-fonctionnalités-principales)
- [Architecture](#-architecture)
- [Prérequis](#-prérequis)
- [Installation](#-installation)
- [Utilisation](#-utilisation)
- [Configuration](#-configuration)
- [Contribution](#-contribution)
- [Licence](#-licence)

## ✨ Fonctionnalités Principales

### Sous-système Runtime (`soul_kernel`)
- **Ordonnanceur Coopératif** : Un scheduler sans verrou avec vol de travail (work-stealing) conscient de la topologie NUMA et des caches CPU.
- **Matrix Engine** : Noyau GEMM optimisé pour SIMD (AVX-512, AVX2, Neon) pour des calculs tensoriels haute performance.
- **Bus IPC** : Communication inter-agents à latence ultra-faible.
- **Télémétrie & Garde** : Monitoring thermique en temps réel et vérification d'intégrité des flux de données.

### Sous-système Cognitif (`soul_system_bin`)
- **Affective Core** : Modélisation des états émotionnels et neurochimiques (Dopamine, Noradrénaline, Sérotonine).
- **Pare-feu Sémantique** : Filtrage des vecteurs d'activation basé sur la similarité cosinus pour prévenir les états pathologiques.
- **Cortex Récurrent** : Gestion de la mémoire de travail et des cycles cognitifs.
- **Self-Healing** : Détection et réparation automatique des incohérences d'état ontologique.

## 🏗 Architecture

Le projet est organisé en un workspace Cargo de 27 crates, divisé en deux piliers majeurs :

```text
.
├── soul_kernel (Binaire Runtime)
│   ├── soul_scheduler      # Ordonnanceur & Topologie CPU
│   ├── soul_matrix_engine   # Calculs matriciels SIMD
│   ├── soul_ipc            # Bus de communication
│   ├── soul_perception     # Parsing de signaux
│   └── ... (15 crates)
│
├── soul_system_bin (Binaire Cognitif)
│   ├── semantic_firewall   # Sécurité sémantique
│   ├── neural_metacognition # Audit système
│   ├── scirust_affective_core # États affectifs
│   └── ... (10 crates)
│
└── turbovec (Submodule)    # Accélération vectorielle
```

## 💻 Prérequis

- **Rust** : Version 1.75 ou supérieure.
- **OpenBLAS** : Nécessaire pour le lien avec `turbovec` (installez `libopenblas-dev` sur Linux).
- **Système** : Linux fortement recommandé pour le support complet de l'affinité CPU et de la topologie matérielle.

## 🛠 Installation

1. **Cloner le dépôt** :
   ```bash
   git clone https://github.com/CHECKUPAUTO/OS-AGENTS.git
   cd OS-AGENTS
   ```

2. **Initialiser les submodules** (si applicable) :
   ```bash
   git submodule update --init --recursive
   ```

3. **Installer les dépendances système** :
   ```bash
   sudo apt-get install libopenblas-dev
   ```

4. **Compiler le projet** :
   ```bash
   cargo build --release
   ```

## 📖 Utilisation

Le projet propose deux points d'entrée principaux selon vos besoins :

### Lancer le noyau Runtime
Idéal pour tester l'ordonnancement, le bus IPC et le cortex récurrent de base.
```bash
cargo run --bin soul_kernel
```

### Lancer le système Cognitif
Initialise l'affectivité, le pare-feu sémantique et la console clinique.
```bash
cargo run --bin soul_system_bin
```

## ⚙️ Configuration

La plupart des paramètres sont gérés via des structures de configuration internes ou des variables d'environnement (selon les modules).
- **Seuils du Pare-feu** : Configurable dans `semantic_firewall`.
- **Topologie CPU** : Détectée automatiquement par `soul_scheduler`.

## 🤝 Contribution

Les contributions sont les bienvenues ! Consultez le fichier [CONTRIBUTING.md](CONTRIBUTING.md) pour connaître la marche à suivre.

## 📄 Licence

Ce projet est distribué sous la licence **MIT**. Voir le fichier [LICENSE](LICENSE) pour plus de détails.

---
*Développé par l'équipe CHECKUPAUTO.*
