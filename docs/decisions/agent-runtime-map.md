# Agent runtime map

First deliverable of the canonical-runtime workstream. **No refactor here** —
the point is to establish what actually exists before moving anything, because
a migration decided from crate names would move the wrong things.

Measured on `main` at `2009419`.

---

## The premise was wrong, and that is the finding

The workstream was scoped as "four competing agent loops":
`soul-agent-core`, `soul_repl`, `soul_planner`, `src/autonomous.rs`.

Reading the callers, **two of those four are not loops at all**, and the one
loop that genuinely competes was not on the list.

| candidate | lines | what it actually is |
|---|---:|---|
| `soul_agent_core::AutonomousAgent` | — | **the agent loop** |
| `src/autonomous.rs` | **41** | a thin wrapper that delegates to it |
| `soul_repl` | — | an interactive REPL — a UI loop, not an agent loop |
| `soul_planner` | — | a component (goal decomposition), used *by* loops |
| **`soul_entity::SoulEntity`** | **954** | **a second, independent agent loop** |

`src/autonomous.rs` is 41 lines: it constructs `AutonomousAgent` and forwards
`ask`, `run_task`, `status`, `tools`. `soul-daemon` does the same. Neither
reimplements anything.

`soul_entity` has its own `run_cycle` and `execute_plan`. It *depends on*
`soul_agent_core` (`soul_entity/Cargo.toml:19`) but does not run its loop.

## Invariant coverage, counted

Occurrences per crate of the terms carrying each hardened invariant. Crude,
but the shape is unambiguous:

| invariant | agent-core | repl | planner | autonomous.rs | **entity** | daemon |
|---|---:|---:|---:|---:|---:|---:|
| emergency stop | **50** | 0 | 0 | 0 | **0** | 0 |
| execution budget | **38** | 0 | 2 | 0 | 5 | 0 |
| provenance / trust | **210** | 0 | 0 | 0 | **0** | 13 |
| sandbox | 1 | 2 | 0 | 0 | 3 | 0 |
| capabilities | 3 | 0 | 0 | 0 | **0** | 0 |

Every invariant hardened over the last 60 PRs lives in `soul-agent-core`. The
second loop carries none of the emergency stop, none of the provenance, and
none of the capability model.

## The canonical runtime is `soul_agent_core::AutonomousAgent`

Not by preference — by the table above and by call sites. It is the loop that
`src/autonomous.rs` and `soul-daemon` already use, and the only one that
enforces what the hardening work established.

## The one real migration: `soul_entity`

`soul_entity` is the second loop, and it is also HIGH-006 — the simulated
autonomy. `execute_plan` formats each step and **dispatches none of them**:

```rust
// No step is executed here. Nothing dispatches this command to a
// shell, a tool or an executor — the loop only formats it.
let outcome = format!("[SIMULATED — not executed] {}", step);
```

That is honest today (it says so, and #161 made the production guard actually
refuse `--entity`), but it means the migration is **not** a port of working
code. There is no execution to move. Migrating `soul_entity` onto the
canonical runtime *is* the work of finishing HIGH-006 — the two are the same
task, and the decision record already says "keep gated, review 2026-10-31".

**So the honest sequencing is:** there is no refactor to schedule right now.
The canonical runtime exists and is used by everything that executes. The one
divergent loop executes nothing, is gated, and is refused in production.

## What would change that

Three triggers, any of which should reopen this:

1. **`soul_entity` starts dispatching steps.** At that moment it becomes a
   second executing loop without the emergency stop, provenance or capability
   model — the exact HIGH-008 shape. It must be built on `AutonomousAgent`
   rather than beside it.
2. **A third loop appears.** The guard below is what catches that.
3. **`soul_repl` grows tool dispatch.** It is a UI loop today; if it starts
   executing tools directly rather than through the agent, it joins this map.

## Guard

`tests/architecture_agent_runtime.rs` pins the set of agent loops by name,
with a budget in both directions. A new loop fails the test; removing one
without lowering the budget also fails it.

It counts *loops*, not crates that mention agents — the distinction this
document exists to make.
