# Integration Plan — scirust / SLHAv2 / CCOS / OctaSoma → SoulSystem

> Status: PLAN (no code changes yet). Author: integration analysis pass.
> Toolchain facts: SoulSystem builds on **Rust 1.94.1 stable, edition 2021**.

## 0. Current state & constraints

| Repo | In SoulSystem today | External now | Edition / toolchain | Risk |
|---|---|---|---|---|
| **CCOS** | `ccos/` — old v0.2/early-v0.3 snapshot, missing 11 modules | v0.3.0, stable, 156 tests | 2021 / stable ✅ | **Low** |
| **OctaSoma** | absent | v0.4.0, 12 tests, 1 dep (`lz4_flex`) | 2024 / stable ✅ (no 2024-only features) | **Low** |
| **SLHAv2** | absent | v0.2.0, `scirust` kernel (zero-dep) + `slha-mcp` | 2021 / stable ✅ | **Med** |
| **scirust** | ~20 `scirust-*` crates, old/minimal (`nn/`: 4 files) | v0.14.0, 73 crates, 1718 tests | 2021 but **nightly** (`portable_simd`) | **High** |

**Hard constraint:** the external scirust pins `rust-toolchain = nightly` for `#![feature(portable_simd)]`. SoulSystem is stable-only. A full scirust subtree sync would force the whole monorepo onto nightly → **not acceptable**. scirust is therefore handled by *selective module порting*, not a wholesale sync.

**Recommended sequencing** (low-risk / high-value first):
1. **CCOS** (flagship: real agent-context win, lowest risk) →
2. **OctaSoma** (clean additive memory backend) →
3. **SLHAv2** (MCP tool now; KV-cache kernel into `soullink-inference` next) →
4. **scirust** (selective, gated, last — biggest/riskiest).

Each phase is independently shippable and independently testable.

---

## 1. CCOS — causal context manager for the agent

**What's new (v0.3):** `external_memory` façade, `agent_session` (time-travel replay), `mcp` server (8 tools), `postmortem` REPL, `trace.rs` (cargo-panic → page-fault), spatial `region_engine`, durable checkpoints. SoulSystem's `ccos/` is missing all of these.

**Integration surface:** `soul-agent-core/src/lib.rs` — `compact_if_needed()` (text-only 4-pass compaction, no causal structure) and the tool-result/failure paths.

**Steps**
1. **Re-sync `ccos/` to upstream v0.3** (drop-in: same Cargo.toml, edition 2021, stable). Verify `cargo test -p ccos`.
2. Add `ccos = { path = "ccos", features = ["syn-parser"] }` to `soul-agent-core`.
3. New `CcosContextManager` field on `AutonomousAgent` (workspace-scoped `CcosMemory`).
4. **Ingest**: in the tool-result path, `ingest_source(uri, src)` for each file the agent reads.
5. **Page-fault**: in the tool-failure path, parse `cargo test`/`error[` output → `page_fault(output)`; generic failures → `signal_failure(node, depth=3)`.
6. **Recall**: replace `compact_if_needed()` body with `recall(Recall::task(...), budget)` → rebuild `ChatSession` from the causal window (system prompt + last N turns preserved). Keep current text-compaction as a fallback if recall is empty.
7. (Optional) Register CCOS `mcp` server in `.mcp.json` for time-travel via Claude Code.

**Terminal tests**
- `cargo test -p ccos` (upstream's 156 tests).
- New `soul-agent-core` tests: multi-file dep chain (main→handler→db); assert `recall` keeps all 3 files in the window; assert `page_fault` re-pages to the failing file; assert checkpoint round-trips (`stats` identical after reopen).
- `cargo run -p ccos -- postmortem workspace.ccos` → `timeline`, `goto`, `recall`, `energy`.

**AI-agent test (terminal window):** give the agent a multi-file bug task and force a long session that would normally evict the buggy file; assert (via `AgentEvent`s / CCOS `stats`) that the buggy file stayed in context and the agent fixed it. Compare runs with CCOS on vs off.

---

## 2. OctaSoma — 3-D fractal semantic memory backend

**What it is:** topical semantic store — high-D embedding → 3-D PCA/JL projection → cache-line octree, exact 3-D k-NN, LZ4-persisted. **Honest limit:** ~71% *cluster* recall@1, ~0% *exact* NN — a coarse topical pre-filter, not a full ANN. Use it as a complement to HNSW, not a replacement.

**Edition:** 2024, but zero 2024-only features → fine as a path dependency for our 2021 crates (precedent: we already mix editions).

**Integration surface:** `soullink-brain/soullink-memory` (Ollama embed + graph), `soullink-vector` (HNSW), `soullink-memory-hierarchy` (semantic tier).

**Steps**
1. Vendor `octasoma/` (copy or subtree) as a workspace member.
2. New adapter crate `soullink-brain/soullink-octasoma-backend` translating `FractalMemory3D` ↔ SoulSystem types (`VectorSearchResult`, labels).
3. Feature-gate it in `soullink-memory` (`octasoma-backend`) as an alternative/secondary backend (swap at construction).
4. (Optional) Hybrid index in `soullink-vector`: store in both HNSW + OctaSoma, boost HNSW hits that also appear in OctaSoma's topical top-k.

**Terminal tests**
- `cargo test -p octasoma` (12 tests) + `cargo run -p octasoma --example agent_demo`.
- Adapter tests: store/recall, PCA-calibrated recall, persistence round-trip (`.frac` reload identical), dimension-mismatch rejected (no panic), topic-coherence (≥3/5 same-topic hits). Use `HashEmbedder` for offline determinism.

**AI-agent test:** agent perceives a set of user facts across turns, then is asked a question whose answer requires recalling an earlier-stored fact; assert the recalled context contains it and the agent uses it. Run offline with `HashEmbedder`, then with `OllamaEmbedder`.

---

## 3. SLHAv2 — GPU-free KV-cache compression

**What it is:** a 128-byte/token KV-cache tile (INT4 low-rank latent + 1-bit sign-LSH residual) giving ~125× cache compression vs FP16, with SIMD scoring (AVX2/AVX-512/NEON) and CCOS-style soft-paging (HOT/WARM/COLD). It is a **compression kernel, not a full inference engine** — plus a zero-dep `slha-mcp` stdio server (5 tools: audit/explain/compress/score/benchmark).

**Integration surface:**
- `soul-mcp` / agent MCP layer ← `slha-mcp` (immediate, easy).
- `soullink-brain/soullink-inference` (we already have `page_cache.rs` / paged KV cache there) ← SLHA tile + elastic cache (real kernel-level integration).
- `soul_llm` `ProviderKind` ← only meaningful if/when a full local inference engine is built (deferred, large).

**Steps**
- **Phase A (now):** vendor SLHA's `scirust` kernel crate (rename to avoid clash with our `scirust-*` — e.g. `slha-kernel`) + `slha-mcp`. Register `slha-mcp` as an agent MCP tool. No `soul_llm` change.
- **Phase B (next):** wire the SLHA tile + `ElasticKvCache` into `soullink-inference`'s KV-cache path as an alternative backend (it complements the MTP `page_cache.rs` we just hardened).
- **Phase C (deferred):** add `ProviderKind::Slha` only alongside a real local inference engine (weights+tokenizer+forward) — a multi-week effort, out of scope for a first integration.

**Terminal tests**
- `cargo test` on the vendored `slha-kernel` (51 tests) + `cargo run -p slha-mcp` then drive `slha.audit` (must pass all assertions, exit 0) and `slha.benchmark`.
- `bash SLHAv2/demo.sh` on the server for a human-visible end-to-end.
- Phase B: `soullink-inference` test — fill an `ElasticKvCache`, page HOT→WARM, assert budget invariant + output cosine ≥ ~0.999.

**AI-agent test:** agent receives a diagnostic task ("benchmark the local SLHA kernel and report throughput + SIMD path"); it calls the `slha.audit`/`slha.benchmark` MCP tools and synthesizes a report; assert the tool was invoked and the summary mentions the 128-byte tile + dispatched path.

---

## 4. scirust — selective capability port (gated, last)

**Why not a full sync:** 73 crates, nightly `portable_simd`, dependency drift (`nalgebra 0.33`, `half 2.4`, `rand 0.8` vs our `0.9`), nested sub-workspace. A subtree pull forces the monorepo onto nightly. **Rejected.**

**What's genuinely new & valuable** (per CHANGELOG v0.14): modern SSM layers (Mamba/Mamba-2/S4/S5/Hyena/xLSTM/RetNet/RWKV), the full quantization suite (GPTQ/AWQ/NF4/BitNet/QuIP#…), tensor-network quantum sim, verifiable/deterministic inference (Freivalds/DiFR), advanced optimizers (Sophia/GaLore/Muon/Shampoo), Neural-ODE/PINN/FNO.

**Steps**
1. **Decide a toolchain policy first** (see Decisions). Options: (a) keep scirust port stable-only by avoiding `portable_simd` modules; (b) put scirust-derived crates behind an optional `nightly`-gated feature; (c) move the whole workspace to nightly (not recommended).
2. Port high-value modules into our existing `scirust-core` **selectively**, only those that compile on stable: start with `quantization/`, `nn/nd_optim.rs` (optimizers), `reproducible/` (deterministic reductions). Defer `portable_simd`-dependent kernels behind a feature.
3. Re-export through our current `scirust::` alias so existing consumers (`soul-neural`, `synergie`, `scirust_affective_core`, `semantic_*`) keep compiling; run their tests after each module.
4. Add new modules incrementally, `cargo check`-ing the **whole workspace** after each (catches the nested-sub-workspace + dep-drift issues early).

**Terminal tests**
- Per-module: port → `cargo test -p scirust-core` → `cargo test` for each downstream consumer.
- `cargo check --workspace --all-targets` after every module (our CI gate).
- Optional: run a ported example (e.g. an optimizer convergence test) to validate numerics vs the upstream repo's expected values.

**AI-agent test:** lower priority here (scirust is a compute lib, not an agent capability) — covered by unit/numeric tests rather than agent behavior.

---

## 5. Global test harness — "via the AI agent, in a terminal"

Two complementary layers, both runnable on the server:

**A. Terminal / cargo (deterministic, CI-able)**
```bash
# per-crate gates
cargo test -p ccos -p octasoma -p slha-kernel
cargo test -p soul_agent_core            # CCOS-wired context tests
cargo check --workspace --all-targets    # whole-workspace compile gate
# external repo self-tests as ground truth
( cd /tmp/ext-repos/CCOS && cargo test )
( cd /tmp/ext-repos/octasoma && cargo run --example agent_demo )
bash /tmp/ext-repos/SLHAv2/demo.sh
```

**B. Via the autonomous agent (behavioral, end-to-end)**
The `soulsystem` binary exposes `--repl` and `--goal "<task>"`, and `soul_repl` is the interactive loop. Pattern for each integration: launch the agent with a scripted goal that can only succeed if the new capability works, then assert on `AgentEvent`s / tool calls / subsystem stats.
```bash
# interactive, in a terminal window on the server:
cargo run -p soul_repl --release
#   > task: "Fix the bug in src/db.rs; you'll need handler.rs and main.rs"   (CCOS keeps them in context)
#   > task: "Remember: I prefer metric units. … (many turns later) Which units do I prefer?"  (OctaSoma recall)
#   > task: "Audit and benchmark the local SLHA kernel and report the SIMD path"  (SLHA MCP tool)

# non-interactive (scriptable / CI-able behavioral check):
cargo run --bin soulsystem -- --goal "…task…"
```
For each, add a `#[tokio::test]` that drives `AutonomousAgent::run_task` with the scripted goal against a temp workspace + `HashEmbedder`/mock LLM, asserting the new subsystem was exercised (e.g. CCOS `stats().events > 0`, OctaSoma recall non-empty, SLHA tool invoked).

---

## 6. Decisions needed before implementation

1. **scirust toolchain policy** — stay stable-only (selective port, skip `portable_simd` modules) vs gate scirust behind an optional nightly feature. *Recommendation: stable-only selective port.*
2. **Vendoring mechanism** — `git subtree` (keeps upstream history, re-syncable) vs flat copy (simpler, manual re-sync). *Recommendation: subtree for CCOS/octasoma/SLHA; selective copy for scirust.*
3. **SLHA naming** — its bundled `scirust` crate clashes with our `scirust-*`; rename to `slha-kernel`. *Recommendation: yes.*
4. **Execution order & scope** — confirm CCOS → OctaSoma → SLHA(A/B) → scirust, and whether to do all or stop after the low-risk two.
