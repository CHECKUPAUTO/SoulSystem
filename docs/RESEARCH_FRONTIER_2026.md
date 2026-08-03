# Research Frontier 2026

Status: research map, 2026-06. Companion to `docs/RECURSIVE_SELF_IMPROVEMENT.md`
(the RSI lineage) and `docs/COGNITIVE_ARCHITECTURE.md` (the 7-module design).

This document maps concrete recent papers onto specific crates with a
prioritized roadmap. Every paper has a link and a "how we apply it / which
crate" note — the goal is actionable, not a reading list.

---

## 1. Where SoulSystem already leads

SoulSystem's `soul-rsi` is a Darwin-Gödel-Machine loop that admits a change
**only when it empirically compiles and passes tests** (`soul-rsi/src/evaluator.rs`
`CargoEvaluator`). That empirical gate is a correctness moat. `soul-cognition`
adds honesty invariants (provenance + Level-2 confirmation). The task is to keep
those moats and close the routing/memory/skill-induction gaps below.

---

## 2. Multi-provider routing

Target crates: `avid-model-router`, `soullink-gateway/src/provider`,
`soullink-moe`.

The literature shows learned, cost-aware routing beats heuristics by ~2× cost at
equal quality, and predictive routing avoids paying for the strong model first.

| Paper | Link | Apply to SoulSystem |
|-------|------|---------------------|
| RouteLLM: Learning to Route with Preference Data | https://arxiv.org/abs/2406.18665 | Train a binary strong/weak router in `avid-model-router` from logged preference/outcome data; ~2× cost cut at equal quality. |
| Cost-Aware Contrastive Routing | https://arxiv.org/abs/2508.12491 | Contrastive embedding of query→provider; replaces the hardcoded routing table in the gateway provider layer. |
| Uncertainty Estimation for Efficient LLM Routing | https://arxiv.org/abs/2502.11021 | Use model uncertainty as the defer signal — escalate to a stronger provider only when the cheap one is unsure. |
| BEST-Route: Adaptive Routing with Test-Time Optimal Compute | https://arxiv.org/abs/2506.22716 | Couple routing with N-sampling: cheap model + best-of-N before escalating. |
| Dynamic Model Routing & Cascading (survey) | https://arxiv.org/abs/2603.04445 | Reference taxonomy for the router redesign. |
| LLMRouterBench | https://arxiv.org/abs/2601.07206 | Adopt as the offline eval harness so router changes are measured, not asserted. |

**Moat play:** make routing decisions themselves `soul-rsi`-improvable — the
router is a function the system can evolve and validate on LLMRouterBench.

**Implemented (first slice).** `avid-model-router::learned` adds a learned,
cost-aware router over the existing `ModelProfile` fleet:
- `DifficultyModel` — a logistic predictor of "needs a strong model" from cheap,
  deterministic query features; ships sensible prior weights (useful untrained)
  and a `train()` for RouteLLM-style preference data.
- `CostAwareRouter` — derives a quality bar from predicted difficulty (lowered by
  a `cost_aversion` knob) and picks the **cheapest model that clears it**;
  generalizes RouteLLM's strong/weak deferral to an N-model cascade, with an
  uncertainty band that flags borderline queries for escalation.
- `RouterParams` is serializable and `evaluate()` is a deterministic offline
  score (strong-fraction / avg-cost / accuracy, LLMRouterBench-style), so a
  `soul-rsi` loop can evolve the router and keep only measured improvements.
- `calibrate_threshold()` sets the deferral operating point to a target cost.
26 tests; no new dependencies. Next: wire it into the gateway provider layer and
train on logged gateway outcomes.

---

## 3. Agent memory

Target crates: `soul-cognition` (`memory.rs`), `soullink-memory-hierarchy`,
`soul-memory`.

We already have a 5-tier provenance-aware facade with episodic→semantic
consolidation. The frontier adds *self-organizing* links and disciplined
extract/update so memory stays consistent over long horizons.

| Paper | Link | Apply to SoulSystem |
|-------|------|---------------------|
| Mem0: Scalable Long-Term Memory | https://arxiv.org/abs/2504.19413 | Add the ADD/UPDATE/DELETE/NOOP extraction-and-update discipline to `CognitiveMemory::remember` so facts are reconciled, not duplicated. |
| A-MEM: Agentic Memory | (A-MEM, 2025) https://github.com/Shichun-Liu/Agent-Memory-Paper-List | Self-organizing links between records — back the `associations` field already on `MemoryEntry` with dynamic linking. |
| Reflective Memory Management (In Prospect and Retrospect) | https://arxiv.org/abs/2512.12818 | Powers the **reflexive** tier we added: retain/recall/reflect over the agent's own past. |
| CraniMem: Gated & Bounded Memory | https://arxiv.org/abs/2603.15642 | Gating/eviction policy for the working buffer and episodic decay. |
| Agentic Memory: Unified Long/Short-Term Management | https://arxiv.org/abs/2601.01885 | Single policy spanning working↔episodic↔semantic — generalizes our `ConsolidationEngine`. |
| Memory in the Age of AI Agents (survey + list) | https://github.com/Shichun-Liu/Agent-Memory-Paper-List | Canonical reading list to track the field. |

**Moat play:** memory writes carry **provenance** (`soul-cognition`); applying
Mem0-style updates while preserving Observed/Deduced/Hypothetical tags gives
consistency *and* honesty.

---

## 4. Self-improving skills

Target crates: `soul-skills`, `soul-rsi`, `soul-automodify`.

The Voyager lineage shows the right shape — an ever-growing library of
executable-code skills with self-verification — and 2025/26 work adds
**validation-gated** retention, which aligns perfectly with our empirical gate.

| Paper | Link | Apply to SoulSystem |
|-------|------|---------------------|
| Voyager: Open-Ended Embodied Agent | https://arxiv.org/abs/2305.16291 | The skill-library blueprint: store executable skills with self-verification; `soul-skills` becomes the library, `soul-rsi` the verifier. |
| AutoSkill: Lifelong Skill Self-Evolution | https://arxiv.org/abs/2603.01145 | Derive/maintain/reuse skills from interaction traces as a model-agnostic layer over `soul-skills`. |
| MUSE-Autoskill: Self-Evolving via Skill Create/Manage/Eval | https://arxiv.org/abs/2605.27366 | Full create→manage→evaluate lifecycle, evaluated by our `Evaluator` gate. |
| SEVerA: Verified Synthesis of Self-Evolving Agents | https://arxiv.org/abs/2603.25111 | **Strongest fit**: only retain a synthesized skill if it passes verification — exactly the DGM gate, applied to skills. |
| RL for Self-Improving Agent with Skill Library (SAGE) | https://arxiv.org/abs/2512.17102 | When a reward signal exists, GRPO-style RL over the skill library. |

**Moat play:** unify `soul-skills` + `soul-rsi` so every induced skill is an
archived `Variant` admitted only on empirical pass — no silent regression.

---

## 5. Verified multi-agent orchestration — a reliability moat

Target crates: `soul-subagents`, `soullink-senate`, `soullink-orchestrator`,
`soullink-reasoning`/`soullink-inference`.

Verified orchestration is open space to take a durable lead on reliability
(production SLAs).

| Paper | Link | Apply to SoulSystem |
|-------|------|---------------------|
| Multi-Agent Verification: Scaling Test-Time Compute with Multiple Verifiers | https://arxiv.org/abs/2502.20379 | Add N independent verifiers in `soullink-senate`; scales better than self-consistency. |
| Verified Multi-Agent Orchestration (Plan-Execute) | https://arxiv.org/abs/2603.11445 | Orchestrator checks whether the collective answer addresses the query and triggers targeted replanning. |
| Deterministic Multi-Agent Decision Support | https://arxiv.org/abs/2511.15755 | Zero-variance multi-agent outputs → production SLA commitments. |
| Inter-Rollout Action Agreement as Adaptive-Compute Signal | https://arxiv.org/abs/2604.08369 | Free signal for *when* to spend more compute — cheap reliability in the reasoning loop. |
| Inference-Time Scaling of Verification (rubric-guided) | https://arxiv.org/abs/2601.15808 | Rubric-guided self-verification for the autonomy/research loops. |

**Moat play:** the `senate` already does multi-agent voting; turn votes into
*verifiers* (2502.20379) and gate orchestrator output on coverage
(2603.11445) → deterministic, auditable answers.

---

## 6. The gateway parity track (the concrete blocker)

`soullink-gateway` must become a strict functional copy of the npm-distributed
`soulsystem-gateway` (same CLI options, same endpoints, same channels) so we can
cut over. A precise gap analysis between `soulsystem-gateway/` and
`soullink-brain/soullink-gateway/` (Rust) is the immediate next work item; the
routing research in §2 then makes the Rust gateway *better*, not just equal.
Tracked separately in the gateway parity worklog.

---

## 7. Prioritized roadmap

1. **Gateway parity** (§6) — unblocks cutover from the old gateway binary. *Blocking.*
2. **Learned cost-aware routing** (§2, RouteLLM + LLMRouterBench) — measurable
   win, and itself `soul-rsi`-improvable.
3. **Skill induction under the empirical gate** (§4, SEVerA + Voyager) — turns
   `soul-skills` + `soul-rsi` into a self-evolving, *validated* skill library.
4. **Memory consistency with provenance** (§3, Mem0 + reflective management) —
   long-horizon memory that is both consistent and honest.
5. **Verified orchestration** (§5) — the reliability moat.

The through-line: SoulSystem's differentiator is **empirical validation + honesty
invariants**. Every borrowed technique above is routed through the `soul-rsi`
gate and the `soul-cognition` provenance/permission layer, so we adopt the field's
best ideas without inheriting their failure mode (silent regression / fabricated
confidence).
