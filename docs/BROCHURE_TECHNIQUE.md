# SoulSystem — Brochure Technique

## Résumé exécutif

SoulSystem est un **runtime autonome pour agents numériques**, implémenté en Rust, sous la forme d’un **monorepo de 149 crates Cargo**. Il intègre un réseau neuronal Hamiltonien (SoulLink), un agent ReAct complet (`soul_agent_core`), un système de mémoire hiérarchique, une couche sémantique de sécurité, un moteur de calcul scientifique (SciRust), un OS contextuel causal (CCOS) et un pont unifié vers neuf écosystèmes externes.

**Chiffres clés**

- 149 crates dans le workspace Cargo
- ~740 000 lignes de code Rust
- `cargo check --workspace` : zéro erreur
- Tests unitaires cœur : 97/97 passent
- Langage : Rust 2021, resolver v2, MSRV 1.75+

---

## 1. Objectif du système

Fournir une plateforme unique pour :

1. **Planifier** des objectifs complexes via un moteur de décomposition (`soul_planner`).
2. **Raisonner** en boucle ReAct avec un LLM multi-fournisseur (`soul_llm`).
3. **Agir** via des outils asynchrones, versionnés et contrôlés (`soul_tools`, `soul_sandbox`).
4. **Mémoriser** sous trois formes : travail, épisodique, sémantique (`soul-memory`, `soullink-memory`).
5. **S’auto-observer** : télémétrie, audit immuable, auto-guérison, critique interne.
6. **Collaborer** : mesh de cerveaux SoulLink, sous-agents, consensus sénatorial.

---

## 2. Architecture logicielle

### 2.1 Couche runtime principale (`src/`, `crates/`)

| Module | Rôle | Technologies |
|--------|------|--------------|
| `config` | Chargement TOML + override env | `serde`, `toml` |
| `bus` | Bus de messages interne asynchrone | `tokio::sync::broadcast` |
| `memory_hub` | Point d’accès unifié aux mémoires | `soul-memory`, `soullink-memory` |
| `audit_log` | Journal immuable signé par hachage | `sled`, `sha2` |
| `code_signing` | Vérification ed25519 de code exécutable | `ed25519-dalek` |
| `self_healer` | Réactions aux `DefenseAction` | `soullink-autonomy` |
| `telemetry` | Métriques Prometheus / OTLP | `tracing`, `tracing-subscriber` |
| `ws_bridge` | Passerelle WebSocket | `tokio-tungstenite` |

### 2.2 SoulLink Neural Mesh

SoulLink modélise le système comme un **organisme neuronal** composé d’organes spécialisés (Science, Mind, Engineer, Crypto, Creative, Meta). Le moteur `soullink-core` propulse la dynamique via un **Hamiltonian Neural Network** en intégration symplectique de Verlet.

Composants notables :

- `soullink-inference` : inférence, quantification, routage GPU/CPU, cache KV.
- `soullink-memory` / `soullink-memory-hierarchy` : graphe de concepts, consolidation.
- `soullink-orchestrator` : orchestration distribuée de cerveaux.
- `soullink-autonomy` : méta-cognition, cycles de rêve, préservation.
- `soullink-circuit` : circuit breaker et rate limiting.
- `soullink-senate` : vote multi-agent pour la décision collective.

### 2.3 Entité autonome

Le cœur opérationnel est `soul_agent_core::AutonomousAgent` :

```rust
pub struct AutonomousAgent {
    config: AgentConfig,
    llm: OllamaClient,            // via soul_llm legacy shim
    chat_session: ChatSession,
    planner: CognitiveLoop,
    registry: ToolRegistry,
    executor: AsyncShellExecutor,
    tool_schemas: Vec<ToolSchema>,
    memory: Arc<HierarchicalMemory>,
    metacognition: MetaCognition,
    reasoning: ThoughtTree,
    knowledge_graph: KnowledgeGraph,
    ...
}
```

Cycle ReAct :

1. **Observe** — lit la mémoire, le contexte, les événements.
2. **Think** — génère un plan ou choisit un outil via le LLM.
3. **Act** — exécute l’outil dans le sandbox.
4. **Evaluate** — met à jour les scores, enregistre la trajectoire, cristallise les compétences.

La boucle est pilotée par `soul-daemon`, qui gère les buts en arrière-plan, les sous-agents et les points de reprise automatiques.

### 2.4 SciRust

Moteur de calcul scientifique interne :

- `scirust-core` : algèbre linéaire, SIMD, solveurs (linéaire/quadratique/optimiseur), calcul symbolique, autodiff, mini-LLM, embeddings.
- `scirust-autodiff` : différenciation automatique forward/reverse.
- `scirust-symbolic` : parseur d’expressions, simplification, preuves d’équivalence.
- `scirust-trading-*` : pipeline quantitatif (core, engine, observer, news, persistence, monitor).

### 2.5 CCOS — Causal Context Operating System

Nouveau membre du workspace : CCOS apporte un modèle **event-sourced causal** avec :

- graphe d’événements causaux,
- replay déterministe,
- consensus distribué,
- tests adversariaux et fuzzing,
- scheduler et workspace intégrés.

### 2.6 Sécurité et sémantique

| Composant | Fonction |
|-----------|----------|
| `semantic_firewall` | Bloque les sorties proches sémantiquement de concepts interdits. |
| `semantic_neuromodulator` | Modulation neuro-chimique de l’attention et de la récompense. |
| `soul_sandbox` / `BoundSystem` | Exécution shell sandboxée (bubblewrap/seccomp). |
| `soul_critique` | Critique interne sur 6 dimensions après chaque tâche. |
| `code_signing` | Signature ed25519 du code exécutable par extensions. |

### 2.7 Ponts unifiés

`soul-bridge` remplace neuf crates historiques par des alias de module :

```rust
use soul_bridge::avid as avid_bridge;
use soul_bridge::brain as brain_bridge;
use soul_bridge::mesh as mesh_bridge;
// etc.
```

Cela préserve le code historique tout en réduisant la duplication.

### 2.8 Interfaces

- CLI : `soulsystem --dev`, `--repl`, `--daemon`.
- TUI : `soul_repl` avec streaming.
- HTTP/WS : `soul_gateway`, `soullink-gateway`.
- API bridge : `/api/bridges/probe`, `/health`, `/metrics`.

---

## 3. Mémoire : le cœur du raisonnement

SoulSystem considère la mémoire comme un **système de fichiers sémantique** :

- **Working memory** : contexte immédiat de la session (`WorkingMemory`).
- **Episodic memory** : traces des interactions passées.
- **Semantic memory** : concepts et relations consolidés.
- **Knowledge graph** : nœuds/tâches liés dans `soul_agent_core`.
- **Audit chain** : journal immuable de toutes les actions sensibles.

La consolidation est périodique et déclenchée par `soul-daemon`.

---

## 4. Sécurité et permissions

Les outils sont classés par niveau de permission :

- `Read`
- `Write`
- `Destructive` — bloqué par défaut, nécessite validation explicite.

L’exécution shell passe par `AsyncShellExecutor`, qui s’appuie sur `soul_sandbox::Sandbox` avec une politique configurable. Les signatures ed25519 garantissent l’intégrité du code exécuté par les extensions.

---

## 5. Performance et échelle

- Rust natif, pas de GC.
- Caches KV et modèles routés GPU/CPU.
- Pool de connexions LLM avec budgets tokens/minute et par objectif.
- Bus asynchrone tokio ; exécution parallèle des appels de cerveaux.
- Profiling thermique et métriques Prometheus intégrés.

---

## 6. Intégrations

| Fournisseur / Protocole | Support |
|-------------------------|---------|
| LLM | Ollama, OpenAI, Anthropic |
| Embeddings | Ollama batch, OpenAI `/embeddings` |
| Streaming | NDJSON (Ollama), SSE (OpenAI/Anthropic) |
| Automatisation navigateur | `soul_browser` |
| Web fetching | `soul_webfetch` |
| Protocole MCP | `soul_mcp` |
| Notifications | Telegram (`teloxide`) |
| GPU | CUDA via feature `gpu` (tronçons externs pour l’instant) |

---

## 7. Validation continue

```bash
cargo check --workspace        # vérification rapide
cargo test --workspace           # tests (certains longs désactivés par défaut)
cargo clippy --workspace         # analyse statique
./scripts/validate.sh          # pipeline complète
```

État actuel :

- `cargo check --workspace` : ✅
- Tests cœur : ✅ 97/97
- Warnings : nettoyés ; seul reste l’avertissement attendu sur la dépendance `souls` sans target lib.

---

## 8. Licence

MIT OR Apache-2.0.
