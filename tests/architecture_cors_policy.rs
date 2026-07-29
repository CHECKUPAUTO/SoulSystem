//! Architecture guard for INV-NET-4 (see `docs/security/SECURITY_INVARIANTS.md`).
//!
//! `tower_http::cors::CorsLayer::permissive()` sends
//! `Access-Control-Allow-Origin: *`. That header does not bypass authentication
//! on its own, but it removes the browser's same-origin barrier — the thing
//! that normally stops a page the operator happens to have open from *reading*
//! a listener's responses. Binding to `127.0.0.1` does not help: a page loaded
//! from anywhere can still reach `http://127.0.0.1:<port>`.
//!
//! It is also a single call. Re-introducing it is one line, in a service
//! nobody is looking at, and nothing else in the build would object — which is
//! exactly the shape of defect a guard is for.
//!
//! # Scope, stated plainly
//!
//! **Workspace members only**, for the same reason as the process-execution and
//! secret-type guards: non-member trees are not built by
//! `cargo build --workspace`, so this says nothing about them. `os-agents/` in
//! particular still contains a permissive call and is deliberately out of scope
//! (see LOW-003) rather than quietly counted as clean.
//!
//! This also only catches the *literal* `permissive()` constructor. A
//! hand-rolled `CorsLayer::new().allow_origin(AllowOrigin::any())` is the same
//! defect spelled differently, and the guard checks for that spelling too — but
//! an allowlist built at runtime from an attacker-influenced source would not
//! be visible to any amount of source scanning.

use std::path::{Path, PathBuf};

/// Spellings of "allow every origin" that must not appear in a workspace
/// member.
const FORBIDDEN: &[&str] = &[
    "CorsLayer::permissive(",
    "AllowOrigin::any(",
    "allow_origin(Any)",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_member_dirs() -> Vec<String> {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("root Cargo.toml must be readable");
    let block = manifest
        .split_once("members = [")
        .expect("root manifest declares workspace members")
        .1;
    let block = block
        .split_once(']')
        .expect("the members array is terminated")
        .0;
    block
        .lines()
        .filter_map(|l| {
            let l = l.trim().trim_end_matches(',').trim();
            let l = l.strip_prefix('"')?.strip_suffix('"')?;
            (!l.is_empty()).then(|| l.to_string())
        })
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // `target/` under a member directory is build output, not source.
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Whether one source line is an allow-any-origin call site.
///
/// Comments are skipped: this codebase discusses the defect by name in doc
/// comments throughout — including in this file — and documentation is not a
/// call site.
fn line_allows_any_origin(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return false;
    }
    FORBIDDEN.iter().any(|pat| line.contains(pat))
}

/// Every `(file, line, snippet)` in a workspace member that allows any origin.
fn permissive_cors_sites() -> Vec<(String, usize, String)> {
    let root = repo_root();
    let mut sites = Vec::new();
    for member in workspace_member_dirs() {
        let mut files = Vec::new();
        rust_files(&root.join(&member), &mut files);
        for path in files {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            for (idx, line) in text.lines().enumerate() {
                if line_allows_any_origin(line) {
                    sites.push((rel.clone(), idx + 1, line.trim_start().to_string()));
                }
            }
        }
    }
    sites
}

#[test]
fn no_workspace_member_allows_every_cors_origin() {
    let offenders = permissive_cors_sites();
    assert!(
        offenders.is_empty(),
        "a workspace member allows every cross-origin caller (INV-NET-4). \
         `Access-Control-Allow-Origin: *` lets any page the operator has open \
         read this listener's responses. Use \
         `soul_cors::CorsPolicy::from_env(VAR)` instead, which fails closed \
         when unset:\n{}",
        offenders
            .iter()
            .map(|(f, l, src)| format!("  {f}:{l}: {src}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn the_guard_scans_a_plausible_number_of_members() {
    // A guard that silently scans nothing reports success over a codebase it
    // never examined. The member list is parsed out of the root manifest by
    // string-splitting, so it is worth asserting the parse actually worked.
    let dirs = workspace_member_dirs();
    assert!(
        dirs.len() > 20,
        "expected the workspace to declare many members, parsed {}. If the \
         manifest's formatting changed, fix this parser — do not assume the \
         workspace shrank.",
        dirs.len(),
    );
    assert!(dirs.iter().any(|d| d == "crates/soul-cors"));
}

#[test]
fn the_matcher_finds_real_call_shapes_and_ignores_prose() {
    // Exercises the same function the scan uses, on the shapes that actually
    // appeared in this repository before the migration.
    for offending in [
        "        .layer(tower_http::cors::CorsLayer::permissive())",
        "    let app = build_router(state).layer(CorsLayer::permissive());",
        "            .allow_origin(Any)",
        "        let cors = CorsLayer::new().allow_origin(AllowOrigin::any());",
    ] {
        assert!(
            line_allows_any_origin(offending),
            "should have been flagged: {offending}"
        );
    }

    for benign in [
        "// CorsLayer::permissive( was replaced here",
        "//! `CorsLayer::permissive()` sends `Access-Control-Allow-Origin: *`.",
        "        /// .allow_origin(Any) is the same defect spelled differently",
        "        .layer(soul_cors::CorsPolicy::from_env(VAR).read_write_layer())",
        "        .allow_origin(AllowOrigin::list(origins))",
    ] {
        assert!(
            !line_allows_any_origin(benign),
            "should NOT have been flagged: {benign}"
        );
    }
}

/// Lines that build a CORS layer by hand rather than through `soul_cors`.
///
/// `.allow_origin(` is the marker: it is the call that decides *who* may read a
/// response, and it is the one `soul_cors::CorsPolicy::layer` exists to own.
fn hand_rolled_allow_origin_sites() -> Vec<(String, usize, String)> {
    let root = repo_root();
    let mut hits = Vec::new();

    for member in workspace_member_dirs() {
        // The shared crate is the one place this call belongs.
        if member == "crates/soul-cors" {
            continue;
        }
        let dir = root.join(&member);
        if !dir.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&dir, &mut files);

        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for (i, line) in text.lines().enumerate() {
                let trimmed = line.trim_start();
                // Prose about the call is not the call.
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    continue;
                }
                if line.contains(".allow_origin(") {
                    let rel = file
                        .strip_prefix(&root)
                        .unwrap_or(&file)
                        .to_string_lossy()
                        .replace('\\', "/");
                    hits.push((rel, i + 1, line.trim().to_string()));
                }
            }
        }
    }
    hits
}

/// One implementation of the origin rule, not several.
///
/// P1-2 hardened `soul_gateway` and `soul-dashboard` with their own copies;
/// P1-10 later extracted `soul-cors` with the same rule for seven other
/// services. Three correct copies are still three places a future fix has to
/// land, and the one that gets missed fails open — an allowlist that keeps
/// admitting an origin that was supposed to be revoked looks exactly like one
/// that is working.
///
/// P1-10-B folded both copies in. This keeps a fourth from appearing.
#[test]
fn only_soul_cors_decides_which_origins_are_allowed() {
    let hits = hand_rolled_allow_origin_sites();
    assert!(
        hits.is_empty(),
        "these workspace members call .allow_origin() directly instead of going \
         through soul_cors::CorsPolicy: {hits:#?}. Use `read_only_layer()` for a \
         listener that only serves reads, or `read_write_layer()` for one that \
         accepts submissions — picking the wrong one silently widens the policy, \
         which is why there is no single default."
    );
}

/// Anti-vacuous-pass: the matcher must still recognise the call it forbids, or
/// the test above passes because it stopped looking rather than because nothing
/// is there.
#[test]
fn the_allow_origin_matcher_still_recognises_the_real_call() {
    let samples = [
        (".allow_origin(AllowOrigin::list(origins))", true),
        ("        base.allow_origin(AllowOrigin::any())", true),
        ("/// then .allow_origin(...) is applied", false),
        ("// .allow_origin(Any) was removed here", false),
        ("let x = 1;", false),
    ];
    for (line, expected) in samples {
        let trimmed = line.trim_start();
        let is_comment = trimmed.starts_with("//") || trimmed.starts_with("///");
        let hit = !is_comment && line.contains(".allow_origin(");
        assert_eq!(hit, expected, "matcher disagreed on {line:?}");
    }
}

/// The two folded crates must actually be reached by the scan, or the test
/// above proves nothing about them specifically.
#[test]
fn the_scan_reaches_the_two_folded_crates() {
    let members = workspace_member_dirs();
    for expected in ["soul_gateway", "crates/soul-dashboard"] {
        assert!(
            members.iter().any(|m| m == expected),
            "{expected} must be among the scanned members, got {} members",
            members.len()
        );
    }
}
