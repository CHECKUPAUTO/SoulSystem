# Cognitive Architecture — Autonomous Digital Agent

Status: design reflection, 2026-06.
Scope: maps the 7-module *Autonomous Digital Agent* specification onto the
existing SoulSystem crates, names the gaps, and proposes a thin
coordination crate (`soul-cognition`) that wires them together while
enforcing the honesty invariants.

This document is a **map and a contract**, not a claim that every module is
finished. Where a module is only partially realized, it says so explicitly —
in keeping with the architecture's own first rule (never present the
hypothetical as the observed).

---

## 1. The seven modules and where they already live

| # | Module | Primary existing crate(s) | State |
|---|--------|---------------------------|-------|
| 1 | Perception | `soul_tools`, `soul-eventbus`, `src/telemetry.rs`, `soul-top` | partial — observations are produced but **not provenance-tagged** |
| 2 | Memory (5 types) | `soullink-memory-hierarchy`, `soul-memory`, `soul-graph-memory` | partial — 3 of 5 tiers exist (working/episodic/semantic) |
| 3 | Internal Goals | `soul_planner`, `soul-goaltree` | present |
| 4 | Artificial Curiosity | — | **gap** |
| 5 | Experimentation Loop | `soul-rsi` (Darwin Gödel Machine) | present, strong |
| 6 | Critical Evaluator | `soul-critique`, `soul-rsi::evaluator::CargoEvaluator` | present |
| 7 | Permission Levels 0/1/2 | `soul_tools::PermissionLevel` | present, **gate not enforced at the loop level** |

### 1.1 Perception
`soul_tools` is the sensorium: `AsyncShellExecutor`, file ops, and
`dispatch_tool` return real, executed results. `soul-eventbus` and
`src/telemetry.rs` carry system metrics. What is missing is **provenance**:
every observation that enters memory should be tagged

```
Observed     — produced by an executed tool call / measured metric
Deduced      — derived by the agent from one or more Observed facts
Hypothetical — proposed but not yet verified
```

This tag is the load-bearing mechanism behind the architecture's core
invariant ("never fabricate observations"). It belongs on the *perception
boundary*, not bolted on later, so that downstream memory and reasoning can
never silently launder a hypothesis into a fact.

### 1.2 Memory
`soullink-memory-hierarchy` already implements three of the five required
tiers as first-class types:

- `WorkingMemory` (`src/lib.rs:103`)
- `EpisodicStore` + `EpisodicConfig` (`src/lib.rs:178`, `:158`)
- `SemanticStore` + `SemanticConfig` (`src/lib.rs:316`, `:299`)
- `ConsolidationEngine` (`src/lib.rs:474`) decays/clusters episodic →
  semantic, which is exactly the consolidation the spec asks for.

The two missing tiers are **strategic** (long-horizon plans, lessons that
shape future goals) and **reflexive** (the agent's model of its own past
behaviour — Reflexion-style verbal reinforcement). A fifth, **user**
memory (stable facts about the operator), is partially served by
`soul-memory`'s `KnowledgeGraph` but is not a named tier.

Mapping (5 spec tiers → implementation):

| Spec tier | Implementation |
|-----------|----------------|
| episodic | `EpisodicStore` ✓ |
| semantic | `SemanticStore` ✓ |
| user | `soul-memory::KnowledgeGraph` (needs a typed facade) |
| strategic | **new** — derive from `soul-goaltree` + consolidation |
| reflexive | **new** — Reflexion log, feeds module 6 |

### 1.3 Internal Goals
`soul_planner` (`Goal`, `GoalStatus`, `CognitiveLoop::create_plan`) and
`soul-goaltree` cover decomposition and tracking. No new crate needed;
strategic memory should feed goal *generation*, not just goal execution.

### 1.4 Artificial Curiosity — the real gap
Nothing today computes an intrinsic reward / novelty signal to decide
*what to explore when no external goal is pending*. This is the one module
with no home. Minimal viable form: a `Curiosity` scorer that ranks
candidate probes by prediction-error or information-gain over semantic
memory, emitting low-stakes (Level 0) exploration goals into `soul_planner`.

### 1.5 Experimentation Loop — already a Darwin Gödel Machine
`soul-rsi` is the strongest piece. It is a faithful open-ended
self-improvement loop:

- `Archive` of stepping-stone `Variant`s with parent selection
  (`archive.rs:79`) — the DGM "open-ended archive".
- `Proposer` / `Evaluator` traits (`traits.rs`) — propose-then-validate.
- `CargoEvaluator` (`evaluator.rs:15`) — the **empirical gate**: a variant
  is only admitted if it actually compiles and passes tests. This is the
  Gödel-machine "provable benefit" relaxed to "empirically demonstrated
  benefit", which is exactly the STOP / DGM design.
- `RsiEngine::step` / `run` (`engine.rs:114`, `:190`) drive the loop with a
  seeded RNG for reproducibility.

Curiosity (module 4) should feed *candidate directions* into the Proposer;
the Critical Evaluator (module 6) is the Evaluator.

### 1.6 Critical Evaluator
`soul-critique` plus `soul-rsi`'s evaluator close the loop. The reflexive
memory tier should persist each verdict so the agent accumulates "what I
tried, what happened" — the Reflexion signal.

### 1.7 Permission Levels
`soul_tools::PermissionLevel` (`src/lib.rs:210`) already classifies actions
conservatively ("when in doubt, escalate", `from_command` at `:221`):

| Spec level | `PermissionLevel` | Rule |
|-----------|-------------------|------|
| 0 (read) | `Read` | auto-allowed |
| 1 (write) | `Write` | allowed within sandbox, logged |
| 2 (impactful) | `Destructive` | **ALWAYS requires explicit confirmation** |

The classifier exists; what is missing is a single **enforcement point**
that the autonomous loop *cannot bypass* — a gate that, for any
`Destructive` action, halts and requires confirmation rather than
proceeding. Today that decision is scattered. It must be centralized.

---

## 2. Honesty invariants (cross-cutting)

These are not a module; they are properties the whole loop must preserve:

1. **Never fabricate observations.** Only executed tool calls / measured
   metrics may enter memory as `Observed`.
2. **Never claim an unexecuted action as done.** The loop records actions
   only after the executor returns.
3. **Always distinguish Observed / Deduced / Hypothetical.** Carried as a
   provenance tag from perception through memory to output.
4. **Level 2 always requires confirmation.** No autonomous path may execute
   a `Destructive` action without an explicit human/owner approval token.

Invariants 1–3 are enforced by the provenance tag (§1.1); invariant 4 by
the central permission gate (§1.7).

---

## 3. Proposed wiring: `soul-cognition`

Rather than re-implement what exists, add one thin coordination crate that
owns *only* the missing connective tissue and the invariants:

```
soul-cognition
├── provenance.rs   // Provenance{Observed,Deduced,Hypothetical} + Tagged<T>
├── perception.rs   // wraps soul_tools dispatch -> Tagged observations
├── memory.rs       // 5-tier facade over memory-hierarchy + soul-memory
├── curiosity.rs    // novelty/info-gain scorer -> Level-0 probe goals  (NEW)
├── gate.rs         // single Level-2 confirmation enforcement point
└── loop.rs         // perceive -> recall -> (goal|curiosity) -> experiment
                    //   (soul-rsi) -> evaluate -> consolidate -> reflect
```

It depends on, and does not duplicate: `soul_tools`, `soul_planner`,
`soullink-memory-hierarchy`, `soul-memory`, `soul-rsi`, `soul-critique`.

### Loop sketch
1. **Perceive** — `soul_tools` -> `Tagged<Observation>` (always `Observed`).
2. **Recall** — pull relevant episodic/semantic/strategic context.
3. **Decide** — if an external goal exists, plan it (`soul_planner`);
   else ask **Curiosity** for the highest-value Level-0 probe.
4. **Experiment** — hand candidates to `soul-rsi`'s Proposer/Evaluator;
   only empirically-validated variants are admitted to the Archive.
5. **Evaluate** — `soul-critique` verdict; persist to **reflexive** memory.
6. **Consolidate** — `ConsolidationEngine` promotes durable episodic ->
   semantic; strategic memory updates future goal priors.
7. **Gate** — any `Destructive` step is intercepted by `gate.rs` and
   blocked pending confirmation, regardless of how it was generated.

---

## 4. Build order

1. `provenance.rs` + `gate.rs` — invariants first; they are the contract.
2. `memory.rs` facade — name all five tiers, back the new two with
   consolidation + goaltree.
3. `curiosity.rs` — the one genuinely missing capability.
4. `loop.rs` — wire it, reusing `soul-rsi` unchanged as the experiment core.

This keeps the proven DGM loop (`soul-rsi`) and the memory hierarchy
untouched, and confines new, riskier code to curiosity and the gate — the
two places where correctness actually matters for autonomy safety.

---

## 5. Research grounding

The experimentation loop follows the lineage documented in
`docs/RECURSIVE_SELF_IMPROVEMENT.md`: I.J. Good's intelligence explosion,
Schmidhuber's Gödel Machine (provable self-modification), STOP
(Zelikman 2023, self-taught optimizer), the Darwin Gödel Machine
(Zhang 2025, open-ended archive + empirical gate), and Reflexion (verbal
reinforcement -> the reflexive memory tier here). The honesty invariants are
the operational expression of a constitutional framing applied to an agent
that edits its own source.
