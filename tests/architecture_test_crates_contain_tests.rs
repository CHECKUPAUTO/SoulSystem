//! A crate that claims to hold tests must hold tests (LOW-007).
//!
//! Two `Cargo.toml` files named `soul_integration_tests` — at the repo root
//! and under `os-agents/` — declared themselves "Integration tests for
//! SoulSystem multi-crate workflows", listed fourteen path dependencies, and
//! contained **no source files at all**. Cargo refused them outright ("no
//! targets specified in the manifest"), so they had never been built, never
//! run, and were in no workspace's members list.
//!
//! The harm is not a broken build — nothing built them. It is a **false
//! assurance signal**: an auditor, a new contributor, or a release checklist
//! that greps for integration coverage finds a crate whose name promises
//! exactly that, and concludes the coverage exists. An empty test crate is
//! worse than no test crate, in the same way an unrecorded memory write is
//! worse than an absent one: both look like something happened.
//!
//! This guard runs over the whole repo, not just workspace members,
//! deliberately — the deleted crates were *not* members, so a members-only
//! check would have been blind to exactly the thing that went wrong.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Directories that are not ours to police.
fn is_skippable(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "target" || s == ".git" || s == "node_modules" || s == "vendor"
    })
}

fn manifests_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if is_skippable(&path) {
            continue;
        }
        if path.is_dir() {
            manifests_under(&path, out);
        } else if path.file_name().is_some_and(|n| n == "Cargo.toml") {
            out.push(path);
        }
    }
}

/// Whether a crate directory holds anything cargo would treat as a target or
/// a test: `src/`, a `tests/` directory, or an explicit target section.
fn has_any_target(manifest: &Path, text: &str) -> bool {
    let dir = manifest
        .parent()
        .expect("a manifest has a parent directory");
    dir.join("src").is_dir()
        || dir.join("tests").is_dir()
        || dir.join("benches").is_dir()
        || text.contains("[lib]")
        || text.contains("[[bin]]")
        || text.contains("[[test]]")
}

/// No manifest anywhere in the repo names itself as tests while containing
/// none.
///
/// Scoped to test-claiming crates rather than every crate: a manifest with no
/// targets is cargo's problem to report the moment anything references it,
/// but a manifest that *advertises test coverage* it does not have misleads
/// silently and forever, because nothing ever references it.
#[test]
fn a_crate_that_claims_to_hold_tests_holds_tests() {
    let root = repo_root();
    let mut manifests = Vec::new();
    manifests_under(&root, &mut manifests);

    assert!(
        manifests.len() > 20,
        "found only {} manifests; the walk is broken and this guard would \
         pass vacuously",
        manifests.len()
    );

    let mut offenders = Vec::new();
    for manifest in &manifests {
        let Ok(text) = std::fs::read_to_string(manifest) else {
            continue;
        };
        let claims_tests = text
            .lines()
            .filter(|l| l.trim_start().starts_with("name"))
            .any(|l| l.contains("test"));
        if claims_tests && !has_any_target(manifest, &text) {
            offenders.push(
                manifest
                    .strip_prefix(&root)
                    .unwrap_or(manifest)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        offenders.is_empty(),
        "these manifests name themselves as test crates but contain no \
         source, tests or target section:\n  {}\nAn empty test crate is a \
         false assurance signal — anyone grepping for integration coverage \
         finds it and concludes the coverage exists. Populate it or delete \
         it (LOW-007).",
        offenders.join("\n  ")
    );
}
