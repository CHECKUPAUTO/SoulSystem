//! Guard: how many independent agent loops exist, and which.
//!
//! Every invariant hardened over the last 60 PRs — the emergency stop,
//! execution budgets, provenance/trust, the capability model — lives in
//! `soul_agent_core::AutonomousAgent`. A second loop that runs beside it
//! rather than through it does not inherit any of them, which is the HIGH-008
//! shape: a security mechanism that exists and that the real path does not go
//! through.
//!
//! `docs/decisions/agent-runtime-map.md` records the measurement this guard
//! pins. Read it before changing the budget — the map is what makes the number
//! mean something.
//!
//! ## What counts as a loop
//!
//! A crate that *drives* the observe → think → act cycle itself. Not:
//!
//! - a wrapper that constructs `AutonomousAgent` and forwards calls
//!   (`src/autonomous.rs`, 41 lines; `soul-daemon`),
//! - a component a loop uses (`soul_planner` decomposes goals),
//! - a UI loop (`soul_repl` reads input and streams output).
//!
//! That distinction is the whole point. Counting "crates that mention agents"
//! would have listed four loops where there are two.

use std::path::PathBuf;

/// Crates that drive an agent cycle themselves.
///
/// Each entry says why it is a loop and what it enforces, so that adding one
/// requires stating both.
const AGENT_LOOPS: &[(&str, &str)] = &[
    (
        "soul-agent-core",
        "The canonical runtime. Holds the emergency stop, execution budgets, \
         provenance/trust and the capability model. Everything that executes \
         goes through it.",
    ),
    (
        "soul_entity",
        "A second, independent cycle (`run_cycle` / `execute_plan`). It \
         dispatches nothing — every step is formatted as \
         `[SIMULATED — not executed]` — and `--entity` is refused in \
         production since #161. Migrating it onto the canonical runtime is the \
         same task as finishing HIGH-006; see the decision record.",
    ),
];

/// How many independent agent loops may exist.
///
/// Lowering this as loops are merged is the intended direction. Raising it
/// should require saying so out loud in review, with the reason written into
/// `AGENT_LOOPS`.
const AGENT_LOOP_BUDGET: usize = 2;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Crates that define their own agent cycle, found by looking for the driving
/// methods rather than for the word "agent".
fn crates_driving_a_cycle() -> Vec<String> {
    let root = repo_root();
    let mut found = Vec::new();

    for (dir, entry) in [
        ("soul-agent-core", "src/lib.rs"),
        ("soul_entity", "src/entity.rs"),
        ("soul_repl", "src/app.rs"),
        ("soul_planner", "src/lib.rs"),
        ("soul-daemon", "src/lib.rs"),
    ] {
        let Ok(text) = std::fs::read_to_string(root.join(dir).join(entry)) else {
            continue;
        };
        // Production code only: a test that exercises a cycle is not a loop.
        let production = match text.find("#[cfg(test)]") {
            Some(i) => &text[..i],
            None => &text[..],
        };
        // Driving a cycle means owning the step-execution entry point. A
        // wrapper forwards; it does not define these.
        let drives = production.contains("fn run_cycle") || production.contains("fn execute_plan");
        // `soul-agent-core` is the canonical loop and names its entry points
        // differently; it is identified by owning the agent type itself.
        let is_core = dir == "soul-agent-core" && production.contains("impl AutonomousAgent");
        if drives || is_core {
            found.push(dir.to_string());
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The set of agent loops is exactly the documented set.
#[test]
fn agent_loops_are_exactly_the_documented_set() {
    let found = crates_driving_a_cycle();
    let listed: Vec<&str> = AGENT_LOOPS.iter().map(|(c, _)| *c).collect();

    for c in &found {
        assert!(
            listed.contains(&c.as_str()),
            "{c} drives its own agent cycle but is not in AGENT_LOOPS. A loop \
             that runs beside the canonical runtime instead of through it \
             inherits none of its emergency stop, budgets, provenance or \
             capability model. Build on `soul_agent_core::AutonomousAgent`, or \
             add the entry with the reason it must be separate."
        );
    }

    for (c, _) in AGENT_LOOPS {
        assert!(
            found.contains(&c.to_string()),
            "{c} is listed in AGENT_LOOPS but no longer drives a cycle. Remove \
             the entry and lower the budget so the list keeps describing the \
             code."
        );
    }
}

/// The count is pinned in both directions.
#[test]
fn the_agent_loop_budget_holds_in_both_directions() {
    let count = crates_driving_a_cycle().len();
    match count.cmp(&AGENT_LOOP_BUDGET) {
        std::cmp::Ordering::Greater => panic!(
            "{count} crates drive their own agent cycle but the budget is \
             {AGENT_LOOP_BUDGET}. Each extra loop is a path that does not go \
             through the hardened runtime."
        ),
        std::cmp::Ordering::Less => panic!(
            "only {count} agent loops remain but the budget is still \
             {AGENT_LOOP_BUDGET}. Lower AGENT_LOOP_BUDGET to {count} so the \
             ratchet keeps holding."
        ),
        std::cmp::Ordering::Equal => {}
    }
}

/// The wrapper stays a wrapper.
///
/// `src/autonomous.rs` is 41 lines that construct `AutonomousAgent` and
/// forward to it. If it grows its own cycle it becomes a third loop, and the
/// crate-level scan above would not notice because it is part of the root
/// binary rather than a member crate.
#[test]
fn the_root_binary_wrapper_does_not_grow_its_own_cycle() {
    let text = std::fs::read_to_string(repo_root().join("src/autonomous.rs"))
        .expect("src/autonomous.rs must be readable");

    assert!(
        text.contains("soul_agent_core::") || text.contains("AutonomousAgent"),
        "src/autonomous.rs no longer delegates to the canonical runtime"
    );
    assert!(
        !text.contains("fn run_cycle") && !text.contains("fn execute_plan"),
        "src/autonomous.rs has grown its own agent cycle. It exists to forward \
         to `soul_agent_core::AutonomousAgent`; a cycle here is a third loop \
         outside the hardened runtime."
    );
}

/// Every listed loop states why it is separate.
///
/// A list of loops without reasons becomes a registry of things nobody
/// remembers deciding.
#[test]
fn every_agent_loop_states_why_it_is_separate() {
    for (c, reason) in AGENT_LOOPS {
        assert!(
            reason.len() > 40,
            "{c} needs a real reason for being its own loop, not {reason:?}"
        );
    }
}
