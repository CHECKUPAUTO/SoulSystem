# Documentation des Crates (APIs)

Voici un aperçu des principaux composants (crates) disponibles dans le workspace Soul System.

## 🛠 Modules de Fondation (Runtime)

### `soul_scheduler`
Le cœur de l'exécution.
- **Rôle** : Ordonnancement coopératif et gestion de la topologie CPU.
- **Types clés** : `AgentScheduler`, `Task`, `CpuTopology`.

### `soul_matrix_engine`
Moteur de calcul haute performance.
- **Rôle** : Exécution de GEMM (General Matrix Multiply) vectorisé.
- **Types clés** : `MatrixEngine`, `MatrixDescriptor`.

### `soul_ipc`
Le système nerveux du projet.
- **Rôle** : Passage de messages inter-agents.
- **Types clés** : `InterAgentBus`, `AgentMessage`.

### `soul_perception`
Interface avec le monde extérieur.
- **Rôle** : Parsing ultra-rapide (zero-copy) de flux JSON/binaires vers le bus IPC.

## 🧠 Modules Cognitifs

### `semantic_firewall`
Sécurité sémantique.
- **Rôle** : Blocage de vecteurs basé sur la similarité cosinus.
- **Types clés** : `FirewallGuard`.

### `scirust_affective_core`
Modèle émotionnel.
- **Rôle** : Gestion de l'état affectif (PAD : Pleasure, Arousal, Dominance).
- **Types clés** : `AffectiveState`.

### `soul_cortex`
Mémoire de travail.
- **Rôle** : Implémentation d'un cortex récurrent simple pour la continuité cognitive.
- **Types clés** : `RecurrentCortex`.

## 🛡 Modules de Support

- **`soul_telemetry`** : Collecte de statistiques d'exécution et monitoring thermique.
- **`soul_journal`** : Journalisation persistante (Write-Ahead Log) pour la tolérance aux pannes.
- **`soul_surgery`** : Manipulation directe des activations neuronales (Neuro-steering).
- **`soul_guard`** : Vérification d'intégrité constitutionnelle des flux de données.

---
*Note : Pour une documentation technique détaillée de chaque fonction, générez la documentation Rust avec `cargo doc --open`.*
