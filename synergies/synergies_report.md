# 🔗 SoulLink Synergy Report
**Generated:** 2026-05-13T21:25:28+02:00
**Total synergies:** 125

## Top Priority

### [4/5] AVID → OpenEvolve (`74148960`)
- **Type:** deduplication
- **Description:** Fonction 'new()' dupliquée dans 6 projets
- **Effort:** 1j | **Value:** high
- **Evidence:**
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/gomoco2/src/main.rs
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-scout/src/client.rs
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-scout/src/lib.rs
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-scout/src/ml_classifier.rs
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-scout/src/metrics.rs

### [4/5] AVID → OpenEvolve (`a90aa430`)
- **Type:** deduplication
- **Description:** Fonction 'stats(&self)' dupliquée dans 6 projets
- **Effort:** 1j | **Value:** high
- **Evidence:**
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-tokenjuice/src/lib.rs
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-tokenjuice/src/middleware.rs
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-knowledge-graph/src/graph.rs
  - SoulLink brain: /root/soullink-brain/soullink-inference/src/turboquant/proxy/kv_bridge.rs
  - SoulLink brain: /root/soullink-brain/soullink-inference/src/turboquant/proxy/server.rs

### [3/5] SoulLink organs → SoulLink organs (`2739cec1`)
- **Type:** bridge
- **Description:** Crate 'name' utilisée par 12 projets — possible abstraction commune
- **Effort:** 1s | **Value:** medium
- **Evidence:**
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - OpenClaw CRE: Cargo.toml
  - OpenClaw core: Cargo.toml
  - OpenClaw core: Cargo.toml
  - OpenClaw core: Cargo.toml
  - OpenClaw core: Cargo.toml
  - OpenEvolve: Cargo.toml

### [5/5] ARIS → OpenEvolve (`aris-openevolve`)
- **Type:** fusion
- **Description:** ARIS (analyse codebase) + OpenEvolve (optimisation) = pipeline auto-optimisation
- **Effort:** 1m | **Value:** critical
- **Evidence:**
  - ARIS analyse src/main.rs → rapport
  - OpenEvolve optimise src/main.rs → code amélioré
  - Boucle: analyze → evolve → test → PR

### [4/5] OpenClaw core → SoulLink brain (`790f6f10`)
- **Type:** deduplication
- **Description:** Fonction 'count(&self)' dupliquée dans 4 projets
- **Effort:** 1j | **Value:** high
- **Evidence:**
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-scout/src/cache.rs
  - SoulLink brain: /root/soullink-brain/soullink-inference/src/standby.rs
  - OpenClaw CRE: /root/openclaw-cre/src/sqlite_store.rs
  - OpenClaw core: /root/.openclaw/workspace/avid-vision/src/models.rs
  - OpenClaw core: /root/.openclaw/workspace/avid-skills/src/registry.rs

### [4/5] ARIS → SoulLink brain (`3cf62e13`)
- **Type:** deduplication
- **Description:** Fonction 'get(&self, name: &str)' dupliquée dans 4 projets
- **Effort:** 1j | **Value:** high
- **Evidence:**
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-model-router/src/registry.rs
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-skills/src/registry.rs
  - SoulLink brain: /root/soullink-brain/soullink-wasm-organ/src/lib.rs
  - OpenClaw core: /root/.openclaw/workspace/avid-vision/src/models.rs
  - ARIS: /root/aris/crates/runtime/src/config.rs

### [3/5] SoulLink organs → SoulLink organs (`f06cfcc3`)
- **Type:** bridge
- **Description:** Crate 'path' utilisée par 8 projets — possible abstraction commune
- **Effort:** 1s | **Value:** medium
- **Evidence:**
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - SoulLink organs: Cargo.toml
  - OpenClaw core: Cargo.toml
  - OpenClaw core: Cargo.toml
  - OpenClaw core: Cargo.toml

### [4/5] SoulLink gbrain → AVID knowledge-graph (`gbrain-kg`)
- **Type:** bridge
- **Description:** Fusionner le graphe de connaissances SoulLink avec AVID KG pour un graphe unifié
- **Effort:** 1s | **Value:** high
- **Evidence:**
  - soullink-gbrain/src/graph.rs — graphe neural
  - avid-knowledge-graph/src/graph.rs — graphe sémantique
  - Même trait Node, Edge, Query

### [4/5] ARIS → AVID (`5af0291f`)
- **Type:** deduplication
- **Description:** Fonction 'model(&self)' dupliquée dans 3 projets
- **Effort:** 1j | **Value:** high
- **Evidence:**
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-core/src/llm.rs
  - OpenClaw core: /root/.openclaw/workspace/openhuman-extract/routing/policy.rs
  - ARIS: /root/aris/crates/runtime/src/config.rs
  - ARIS: /root/aris/crates/runtime/src/config.rs

### [4/5] SoulLink brain → AVID (`440f0f71`)
- **Type:** deduplication
- **Description:** Fonction 'get(&self, key: &str)' dupliquée dans 3 projets
- **Effort:** 1j | **Value:** high
- **Evidence:**
  - AVID: /mnt/nvme_secondary/system_root/AVID/crates/avid-vision/src/cache.rs
  - SoulLink brain: /root/soullink-brain/soullink-workflow/src/context.rs
  - ARIS: /root/aris/crates/runtime/src/config.rs
