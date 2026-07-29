//! Architecture guard for INV-MEM-3 and INV-MEM-4 (see
//! `docs/security/SECURITY_INVARIANTS.md`).
//!
//! `AutonomousAgent` owns three stores whose contents are recalled straight
//! back into the model's prompt: the CCOS causal graph, planner action history
//! and OctaSoma semantic memory. Anything that reaches one of them reaches the
//! next prompt, so every write must pass the injection scanner first.
//!
//! Two things enforce that, and this guard covers the gap between them:
//!
//! - **Across the crate boundary**, the compiler does it. The `ccos` and
//!   `semantic` fields are private, so no other crate can call
//!   `agent.ccos.ingest_source(uri, attacker_text)`. That is airtight and
//!   needs no guard.
//! - **Inside `soul_agent_core`**, privacy buys nothing — a new method three
//!   screens down can call `self.semantic.remember(raw)` and the compiler will
//!   happily accept it. That is what this guard is for.
//!
//! # How it decides
//!
//! It finds every direct call to a store-mutating method on `self.ccos` /
//! `self.semantic` in `soul-agent-core/src`, and requires each one to sit
//! inside a function named in `APPROVED_WRITERS`. Those functions take
//! `ScreenedContent` (not `&str`), so their callers cannot reach them with
//! unscreened text either.
//!
//! # What it does not see
//!
//! It is **name-based and syntactic**, like the other guards here. A write
//! reached through a local alias (`let store = &mut self.semantic;`) or a
//! trait object would not be spotted. It also says nothing about the *other*
//! memory systems in this workspace — `soul-memory`, `soullink-memory`,
//! `soullink-memory-hierarchy` — which have their own write paths and are not
//! in scope for INV-MEM-4 as recorded. Both limits are stated rather than
//! quietly counted as clean.

use std::path::{Path, PathBuf};

/// Functions in `soul_agent_core` allowed to mutate a memory store directly.
///
/// Each screens, or takes an already-screened value, before writing:
///
/// - `ccos_observe_tool` — takes `&ScreenedContent`; refuses quarantined
///   content rather than letting the placeholder displace a real record.
/// - `remember_observation` — private, takes `&ScreenedContent`.
/// - `observe_untrusted` — the public entry point; screens, then delegates.
///
/// Adding a name here is a security decision: it asserts the function cannot
/// be reached with unscreened content. Do not add one to silence the guard.
const APPROVED_WRITERS: &[&str] = &[
    "ccos_observe_tool",
    "remember_observation",
    "observe_untrusted",
];

/// Store-mutating calls. Read-only methods (`recall`, `stats`, `len`,
/// `file_unchanged`) are deliberately absent — reading a store is not a way to
/// get unscreened content into it.
const MUTATING_CALLS: &[&str] = &[
    ".ccos.ingest_source(",
    ".ccos.ingest_deferred(",
    ".ccos.signal_failure(",
    ".semantic.remember(",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn agent_core_src() -> PathBuf {
    repo_root().join("soul-agent-core").join("src")
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// The name of the nearest enclosing `fn` at or above `line_idx`.
///
/// Deliberately crude — it walks backwards for the first `fn <name>` at a
/// lower-or-equal indentation. That is enough for this crate's shape (methods
/// in `impl` blocks) and its failure mode is to report `None`, which fails the
/// guard rather than passing it.
fn enclosing_fn(lines: &[&str], line_idx: usize) -> Option<String> {
    for line in lines[..=line_idx].iter().rev() {
        let trimmed = line.trim_start();
        let after_vis = trimmed
            .strip_prefix("pub(crate) ")
            .or_else(|| trimmed.strip_prefix("pub(super) "))
            .or_else(|| trimmed.strip_prefix("pub "))
            .unwrap_or(trimmed);
        let after_async = after_vis.strip_prefix("async ").unwrap_or(after_vis);
        if let Some(rest) = after_async.strip_prefix("fn ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

/// Every direct store-mutating call site, as `(file, line, fn, snippet)`.
fn store_write_sites() -> Vec<(String, usize, Option<String>, String)> {
    let mut sites = Vec::new();
    for path in rust_files(&agent_core_src()) {
        let text = std::fs::read_to_string(&path).expect("source file must be readable");
        let lines: Vec<&str> = text.lines().collect();
        let rel = path
            .strip_prefix(repo_root())
            .unwrap_or(&path)
            .display()
            .to_string();
        for (idx, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("//") || line.trim_start().starts_with("///") {
                continue;
            }
            if MUTATING_CALLS.iter().any(|call| line.contains(call)) {
                sites.push((
                    rel.clone(),
                    idx + 1,
                    enclosing_fn(&lines, idx),
                    line.trim().to_string(),
                ));
            }
        }
    }
    sites
}

#[test]
fn memory_stores_are_only_written_from_screened_paths() {
    let offenders: Vec<_> = store_write_sites()
        .into_iter()
        .filter(|(_, _, func, _)| {
            !func
                .as_deref()
                .is_some_and(|f| APPROVED_WRITERS.contains(&f))
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "a memory store is written outside the screened write paths \
         (INV-MEM-4). Whatever is written here is recalled back into the \
         model's prompt, so it must pass the injection scanner first. Route \
         the write through `observe_untrusted` (which screens) or accept a \
         `ScreenedContent` and add the function to APPROVED_WRITERS only if \
         it genuinely cannot be reached with unscreened text:\n{}",
        offenders
            .iter()
            .map(|(f, l, func, src)| format!(
                "  {f}:{l} in {}: {src}",
                func.as_deref().unwrap_or("<unknown fn>")
            ))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn the_agent_does_not_expose_its_memory_stores_as_public_fields() {
    // A `pub` field would let any downstream crate bypass every screened
    // write path in one line, which is what this whole arrangement exists to
    // prevent. Checked as source text because a private field is, by
    // construction, not observable from a test in another crate.
    let lib = std::fs::read_to_string(agent_core_src().join("lib.rs"))
        .expect("soul-agent-core/src/lib.rs must be readable");

    for field in ["ccos: CcosMemory", "semantic: SemanticMemory"] {
        let decl = lib
            .lines()
            .find(|l| l.trim().starts_with(field) || l.trim().starts_with(&format!("pub {field}")))
            .unwrap_or_else(|| {
                panic!(
                    "field declaration `{field}` not found — has it been renamed? \
                 This guard must be updated with it, not deleted."
                )
            });
        assert!(
            !decl.trim().starts_with("pub "),
            "`{field}` is a public field: any crate can write to the store \
             directly and skip screening entirely (INV-MEM-4). Got: {}",
            decl.trim(),
        );
    }
}

#[test]
fn the_guard_actually_finds_the_known_write_sites() {
    // A guard that silently matches nothing reports success over a codebase it
    // never examined. Anchor it: these writes exist today, so a scan that
    // finds none of them is broken, not clean.
    let sites = store_write_sites();
    assert!(
        sites.len() >= 4,
        "expected to find the known store-mutating call sites in \
         soul-agent-core/src; found {}. If the writes moved, update this \
         guard — do not assume the codebase went quiet.",
        sites.len(),
    );

    let functions: Vec<_> = sites.iter().filter_map(|(_, _, f, _)| f.clone()).collect();
    for approved in APPROVED_WRITERS {
        // `observe_untrusted` delegates rather than writing directly, so it
        // need not appear; the other two must.
        if *approved == "observe_untrusted" {
            continue;
        }
        assert!(
            functions.iter().any(|f| f == approved),
            "`{approved}` is allowlisted as a screened writer but contains no \
             store write — either the allowlist is stale or the scan missed it",
        );
    }
}

#[test]
fn enclosing_fn_handles_the_shapes_this_crate_actually_uses() {
    // The parser is crude on purpose, so its behaviour is pinned rather than
    // assumed. A silent `None` here would fail the guard loudly, not pass it.
    let lines = vec![
        "impl AutonomousAgent {",
        "    pub fn observe_untrusted(&mut self) {",
        "        self.semantic.remember(t);",
        "    }",
        "    async fn other(&mut self) {",
        "        self.ccos.ingest_source(a, b);",
        "    }",
        "    pub(crate) fn third(&mut self) {",
        "        self.ccos.signal_failure(n, 1);",
        "    }",
        "}",
    ];
    assert_eq!(
        enclosing_fn(&lines, 2).as_deref(),
        Some("observe_untrusted")
    );
    assert_eq!(enclosing_fn(&lines, 5).as_deref(), Some("other"));
    assert_eq!(enclosing_fn(&lines, 8).as_deref(), Some("third"));
}
