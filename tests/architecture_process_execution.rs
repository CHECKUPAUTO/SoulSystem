//! Architecture guard for INV-EXEC-1 (see `docs/security/SECURITY_INVARIANTS.md`).
//!
//! Process execution is a security boundary: the approved path is
//! `soul_sandbox`, which applies seccomp, bounded output, a timeout and
//! process-group termination. Findings CRIT-001 and HIGH-002 both trace back to
//! the same root cause — a `Command` spawned somewhere nobody was looking, or a
//! security component that quietly lost its last caller.
//!
//! This test pins the set of files in the **root `soulsystem` binary crate**
//! that may spawn a process. It is intentionally scoped to `src/`: the crate
//! that is actually shipped as the daemon. Widening it to the whole workspace
//! is tracked as follow-up work (the re-verification at `898f472` inventoried
//! 110 matches across 25 workspace-member crates), and would need each of those
//! classified before it could be enforced without a wall of false positives.
//!
//! When this test fails you have three honest options, in order of preference:
//!
//! 1. Route the new call through `soul_sandbox` and delete the bare `Command`.
//! 2. Replace the subprocess with a syscall or library call, as
//!    `SelfHealer::root_disk_used_percent` does instead of shelling out to `df`.
//! 3. If the call genuinely belongs outside the sandbox, add it to
//!    `ALLOWED` below **with a justification comment**, so the exception is
//!    reviewed rather than absorbed.
//!
//! Do not silence this test by deleting it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Files under `src/` permitted to spawn a process, each with the reason.
const ALLOWED: &[(&str, &str)] = &[
    (
        "src/self_healer.rs",
        "RestartService invokes `systemctl restart <unit>`. Gated behind \
         ProcessControl::Enabled (default Disabled) and the unit name is \
         validated by is_safe_service_name. Not sandboxed because restarting a \
         host unit is inherently a host-level operation; the control is that it \
         is opt-in, not that it is confined.",
    ),
    (
        "src/compute_backend.rs",
        "GPU capability probing (`nvidia-smi`, `rocm-smi`, `vulkaninfo`). \
         Read-only, fixed argv, no caller-controlled input. Currently has no \
         non-test caller at all (finding LOW-005, NOT_REACHABLE).",
    ),
];

/// Patterns that indicate a process spawn.
const SPAWN_PATTERNS: &[&str] = &[
    "std::process::Command",
    "tokio::process::Command",
    "Command::new",
    "libc::exec",
    "execve",
];

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is the root crate directory for this integration test.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Strip `#[cfg(test)]` modules and line comments so the scan reflects shipped
/// code, not test scaffolding or prose that merely names `Command`.
fn production_lines(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut in_test_mod = false;
    let mut test_mod_depth: i32 = 0;

    for (idx, raw) in source.lines().enumerate() {
        let line = raw.trim();

        if !in_test_mod && (line == "#[cfg(test)]" || line.starts_with("#[cfg(test)]")) {
            in_test_mod = true;
            test_mod_depth = 0;
            continue;
        }
        if in_test_mod {
            test_mod_depth += raw.matches('{').count() as i32;
            test_mod_depth -= raw.matches('}').count() as i32;
            // Depth returns to zero once the test module's braces balance out.
            if test_mod_depth <= 0 && raw.contains('}') {
                in_test_mod = false;
            }
            continue;
        }

        // Skip comments and doc comments: a mention is not a call.
        if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
            continue;
        }
        out.push((idx + 1, raw.to_string()));
    }
    out
}

#[test]
fn no_unapproved_process_execution_in_the_soulsystem_binary() {
    let root = repo_root();
    let src = root.join("src");
    let mut files = Vec::new();
    rust_files_under(&src, &mut files);
    assert!(
        !files.is_empty(),
        "found no .rs files under src/ — the guard would pass vacuously"
    );

    let allowed: BTreeSet<&str> = ALLOWED.iter().map(|(p, _)| *p).collect();
    let mut violations: Vec<String> = Vec::new();

    for file in &files {
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");

        let Ok(source) = std::fs::read_to_string(file) else {
            continue;
        };

        for (lineno, line) in production_lines(&source) {
            for pattern in SPAWN_PATTERNS {
                if line.contains(pattern) && !allowed.contains(rel.as_str()) {
                    violations.push(format!(
                        "  {rel}:{lineno}  matches `{pattern}`\n    {}",
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Unapproved process execution found in the soulsystem binary crate.\n\n{}\n\n\
         Process execution must go through `soul_sandbox` (INV-EXEC-1). Either route \
         it through the sandbox, replace it with a syscall/library call, or add the \
         file to ALLOWED in tests/architecture_process_execution.rs with a \
         justification. See docs/security/SECURITY_INVARIANTS.md and finding CRIT-001.",
        violations.join("\n")
    );
}

/// An allowlist entry that no longer spawns anything is stale: it grants a
/// standing exception nobody needs, and hides the next real one.
#[test]
fn every_allowlist_entry_is_still_justified() {
    let root = repo_root();
    let mut stale = Vec::new();

    for (rel, _reason) in ALLOWED {
        let path = root.join(rel);
        assert!(
            path.exists(),
            "allowlist entry {rel} does not exist; remove it from ALLOWED in \
             tests/architecture_process_execution.rs"
        );
        let source = std::fs::read_to_string(&path).expect("allowlisted file must be readable");
        let spawns = production_lines(&source)
            .iter()
            .any(|(_, line)| SPAWN_PATTERNS.iter().any(|p| line.contains(p)));
        if !spawns {
            stale.push(*rel);
        }
    }

    assert!(
        stale.is_empty(),
        "these files are allowlisted for process execution but no longer spawn a \
         process: {stale:?}. Remove them from ALLOWED so the exception does not \
         outlive its reason."
    );
}

/// The telemetry path that used to run `df` every 30 seconds must stay
/// syscall-based. This is the specific regression CRIT-001 turned on.
#[test]
fn disk_telemetry_does_not_shell_out() {
    let source = std::fs::read_to_string(repo_root().join("src/self_healer.rs"))
        .expect("src/self_healer.rs must be readable");

    for (lineno, line) in production_lines(&source) {
        assert!(
            !line.contains("Command::new(\"df\")"),
            "src/self_healer.rs:{lineno} spawns `df` again; disk usage must be read \
             via statvfs (see SelfHealer::root_disk_used_percent)"
        );
    }

    assert!(
        source.contains("statvfs"),
        "SelfHealer must read root disk usage via statvfs"
    );
}
