//! Egress guard: which sandboxed executions run with host networking.
//!
//! `SandboxPolicy::network_isolated` defaults to `true`, which puts the child
//! in a network namespace with no configured interfaces — no route anywhere,
//! not even loopback. Some tools genuinely need a network, and each of those
//! sets `network_isolated: false`.
//!
//! ## What this guard does and does not claim
//!
//! It pins the **number of opt-out sites**, so the set cannot grow quietly.
//! Egress control here is per-tool and binary: a tool either gets the host's
//! network or none of it.
//!
//! It does **not** provide per-host egress filtering, and this file exists
//! partly to say so in a place that cannot drift from the code. A tool that
//! needs to reach one endpoint currently gets the whole network. Restricting
//! it to that endpoint needs one of:
//!
//!   * nftables/iptables rules inside the network namespace — privileged
//!     network setup the sandbox cannot perform unprivileged;
//!   * a forced proxy, which needs the same redirect rules to be enforcing
//!     rather than advisory;
//!   * `SECCOMP_RET_USER_NOTIF` supervision of `connect(2)`, where a
//!     supervisor reads the `sockaddr` out of the target's memory — plain
//!     seccomp cannot, because BPF may not dereference pointers.
//!
//! An `AllowedHosts` policy field that nothing enforced would look like the
//! missing control while providing none of it. Counting the exceptions is a
//! smaller claim that happens to be true.

use std::path::{Path, PathBuf};

/// Sites permitted to run a sandboxed child with the host's network.
///
/// Each entry names *why*. A reviewer adding one has to write that reason
/// down, which is the point: "it needed the network" is a decision, not a
/// detail.
const NETWORK_OPT_OUTS: &[(&str, &str)] = &[
    (
        "soullink-orchestrator-standalone/src/routes/spawn.rs",
        "A brain exists to bind and serve a port; isolating it from the \
         network would leave it running and unreachable, which is a failure \
         mode that looks like success.",
    ),
    (
        "soul-bridge/src/octasoma.rs",
        "Must reach the configured `ollama_url`.",
    ),
    (
        "soul-bridge/src/ccos.rs",
        "CCOS is expected to reach the configured endpoint.",
    ),
];

/// How many files may set `network_isolated: false`.
///
/// Lowering this as tools stop needing host networking is the intended
/// direction. Raising it should require saying so out loud in review.
const NETWORK_OPT_OUT_BUDGET: usize = 3;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Workspace member directories, parsed from the root manifest so that a new
/// crate joins this guard without anyone remembering to add it.
fn workspace_member_dirs() -> Vec<String> {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("root Cargo.toml must be readable");
    let block = manifest
        .split_once("members = [")
        .expect("root manifest declares workspace members")
        .1
        .split_once(']')
        .expect("the members list is terminated")
        .0;
    block
        .lines()
        .filter_map(|l| {
            let l = l.trim().trim_end_matches(',').trim_matches('"');
            (!l.is_empty() && !l.starts_with('#')).then(|| l.to_string())
        })
        .collect()
}

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every workspace file that opts a sandboxed child out of network isolation.
fn opt_out_files() -> Vec<String> {
    let root = repo_root();
    let mut found = Vec::new();
    for member in workspace_member_dirs() {
        let dir = root.join(&member);
        let mut files = Vec::new();
        rust_files_under(&dir, &mut files);
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            // Production code only. A `#[cfg(test)]` block setting
            // `network_isolated: false` is usually a test *proving* the
            // unisolated path behaves as documented — soul_sandbox has
            // exactly that — and counting it would push the guard to
            // allowlist the crate whose job is the isolation.
            //
            // Everything from the first `#[cfg(test)]` onward is treated as
            // test code. That is the layout this repo uses (test modules last)
            // and it is a rule a reader can check, unlike parsing Rust here.
            let production = match text.find("#[cfg(test)]") {
                Some(idx) => &text[..idx],
                None => &text[..],
            };

            // The assignment, not the mention: doc comments discussing the
            // field are not opt-outs, and a guard that cannot tell the
            // difference would force bogus allowlist entries onto the files
            // explaining the policy.
            let opts_out = production.lines().any(|line| {
                let l = line.trim();
                !l.starts_with("//") && l.contains("network_isolated: false")
            });
            if opts_out {
                let rel = file
                    .strip_prefix(&root)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                found.push(rel);
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The set of network opt-outs is exactly the documented set.
#[test]
fn network_opt_outs_are_exactly_the_documented_set() {
    let found = opt_out_files();
    let allowed: Vec<&str> = NETWORK_OPT_OUTS.iter().map(|(f, _)| *f).collect();

    for file in &found {
        assert!(
            allowed.contains(&file.as_str()),
            "{file} runs a sandboxed child with host networking but is not in \
             NETWORK_OPT_OUTS. Isolation is the default for a reason: add the \
             entry with the reason it needs a network, or leave \
             `network_isolated` alone."
        );
    }

    for (file, _) in NETWORK_OPT_OUTS {
        assert!(
            found.contains(&file.to_string()),
            "{file} is listed in NETWORK_OPT_OUTS but no longer sets \
             `network_isolated: false`. Remove the entry so the list keeps \
             describing the code."
        );
    }
}

/// The count is pinned in both directions, so the exception cannot grow and
/// a stale budget cannot hide progress.
#[test]
fn the_network_opt_out_budget_holds_in_both_directions() {
    let count = opt_out_files().len();
    match count.cmp(&NETWORK_OPT_OUT_BUDGET) {
        std::cmp::Ordering::Greater => panic!(
            "{count} files run sandboxed children with host networking but the \
             budget is {NETWORK_OPT_OUT_BUDGET}. Each one is a tool that can \
             reach anything on the network; route it through an isolated \
             policy instead of extending the exception."
        ),
        std::cmp::Ordering::Less => panic!(
            "only {count} files opt out of network isolation but the budget is \
             still {NETWORK_OPT_OUT_BUDGET}. Lower NETWORK_OPT_OUT_BUDGET to \
             {count} so the ratchet keeps holding."
        ),
        std::cmp::Ordering::Equal => {}
    }
}

/// Every opt-out carries a stated reason.
///
/// A blank justification would let the list grow by copy-paste, which is how
/// an allowlist stops being a decision record and becomes a formality.
#[test]
fn every_network_opt_out_states_why() {
    for (file, reason) in NETWORK_OPT_OUTS {
        assert!(
            reason.len() > 30,
            "{file} needs a real reason for holding host networking, not {reason:?}"
        );
    }
}
