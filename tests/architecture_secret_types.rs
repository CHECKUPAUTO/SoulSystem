//! Architecture guard for INV-SEC-2 (see `docs/security/SECURITY_INVARIANTS.md`).
//!
//! A credential in a plain `String` field on a `#[derive(Debug)]` struct is one
//! `tracing::debug!(?config)` away from a log line. Nobody writes
//! `println!("{}", token)` on purpose — it leaks through the derive, through a
//! panic message that formats the enclosing struct, or through a debug endpoint
//! that serializes it.
//!
//! [`soulsystem_common::secrets::SecretString`] removes the accidental path:
//! its `Debug`, `Display` and `Serialize` all render a fixed redaction, so an
//! enclosing derive is safe without its author having to think about it.
//!
//! This guard finds secret-named `String` fields on `Debug`-deriving structs
//! across every workspace member and fails on any that is not either migrated
//! or explicitly recorded below.
//!
//! # Scope, stated plainly
//!
//! Workspace members only, for the same reason as the process-execution guard:
//! non-member trees are not built by `cargo build --workspace`, so this says
//! nothing about them. It is also **name-based** — it finds fields called
//! `token`, `secret`, `api_key` and friends. A credential stored in a field
//! called `value` or `blob` will not be found by any amount of this, and the
//! guard does not pretend otherwise.
//!
//! # The allowlist is a record, not an endorsement
//!
//! `NOT_YET_MIGRATED` entries still hold credentials in plain `String`s. They
//! are listed so the count cannot grow silently and so the register can state
//! honestly how far the sweep got. INV-SEC-2 stays `PARTIAL` because they
//! exist.
//!
//! # What this guard does not see
//!
//! It keys on `#[derive(Debug)]`, because the derive is the accidental path.
//! A struct that holds a credential but derives only `Clone` — `clawd::Settings`
//! and `soul-dashboard::AppState` both do — is **not** reported here. That is
//! not the same as safe: such a struct can still leak through a hand-written
//! `Display`, through `serde`, or the moment somebody adds `Debug` to the
//! derive list. It is a smaller exposure, not an absent one, and it is
//! deliberately out of this guard's stated scope rather than quietly counted as
//! clean.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Field-name fragments that indicate credential material.
const SECRET_NAME_FRAGMENTS: &[&str] = &[
    "token",
    "secret",
    "api_key",
    "apikey",
    "password",
    "passwd",
    "private_key",
    "credential",
];

/// Field names that match [`SECRET_NAME_FRAGMENTS`] but hold no secret.
///
/// These are *names of* or *counts of* secrets, not the secret material —
/// redacting them would make configuration undebuggable while protecting
/// nothing. Each is a substring match against the field name.
const NOT_SECRET_MATERIAL: &[&str] = &[
    // A stable identifier used to look a credential up in the OS store.
    "secret_name",
    "secret_header",
    "token_url",
    "token_type",
    "client_secret_env",
    "signature_key_secret_name",
    "hmac_secret_name",
    "identify_secret_name",
    "access_token_field",
    // Token *accounting*, not credentials.
    "token_budget",
    "tokens_per_minute",
    "max_tokens",
    "token_count",
    "tokens_used",
    "prompt_tokens",
    "completion_tokens",
    "total_tokens",
];

/// Structs that still hold a credential in a plain `String`.
///
/// `(file, struct, why not yet)`. Recorded, not blessed — see the module
/// header.
const NOT_YET_MIGRATED: &[(&str, &str, &str)] = &[];

/// How many structs may remain unmigrated.
///
/// Pinned in both directions: it fails if the set grows, and it fails if
/// entries are removed without lowering the budget — a ratchet that only
/// tightens one way stops constraining anything.
const NOT_YET_MIGRATED_BUDGET: usize = 0;

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
        .expect("the members list is terminated")
        .0;

    let mut dirs = Vec::new();
    for line in block.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(start) = line.find('"') {
            let rest = &line[start + 1..];
            if let Some(end) = rest.find('"') {
                dirs.push(rest[..end].to_owned());
            }
        }
    }
    assert!(
        dirs.len() > 20,
        "parsed only {} workspace members; the manifest format changed and the \
         guard would scan almost nothing",
        dirs.len()
    );
    dirs
}

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name == "node_modules" || name == ".git" {
                continue;
            }
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn is_test_or_build_scaffolding(rel: &str) -> bool {
    rel.ends_with("build.rs")
        || rel.contains("/tests/")
        || rel.starts_with("tests/")
        || rel.contains("/benches/")
        || rel.contains("/examples/")
}

/// A secret-looking field on a `Debug`-deriving struct.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Finding {
    file: String,
    line: usize,
    struct_name: String,
    field: String,
}

fn field_is_secret_material(field: &str) -> bool {
    let lower = field.to_ascii_lowercase();
    if NOT_SECRET_MATERIAL
        .iter()
        .any(|exempt| lower.contains(exempt))
    {
        return false;
    }
    SECRET_NAME_FRAGMENTS
        .iter()
        .any(|fragment| lower.contains(fragment))
}

/// Every `<name>: String` / `<name>: Option<String>` pair in a line, keeping
/// only those whose name looks like credential material.
///
/// Scans the **whole line** rather than assuming one field per line, because
/// two real shapes put fields inline and an earlier version of this guard
/// silently missed both:
///
/// ```ignore
/// pub struct AnthropicProvider { pub api_key: String, pub model: String }
/// enum AuthMethod { Basic { username: String, password: String } }
/// ```
///
/// A guard that quietly skips `pub struct X { pub api_key: String }` is worse
/// than no guard: it reports success over an unexamined case.
fn secret_string_fields(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("//") || trimmed.starts_with('*') {
        return Vec::new();
    }

    let mut out = Vec::new();
    for (index, _) in trimmed.match_indices(':') {
        let after = trimmed[index + 1..].trim_start();
        // `String` only — a field already migrated to SecretString must not
        // match, and neither must `Vec<String>` or a `HashMap<String, _>`.
        let is_plain_string = after.starts_with("String,")
            || after.starts_with("String}")
            || after.starts_with("String ")
            || after == "String"
            || after.starts_with("Option<String>");
        if !is_plain_string {
            continue;
        }

        // Walk backwards over the identifier immediately before the colon.
        let before = &trimmed[..index];
        let name: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if name.is_empty() {
            continue;
        }
        if field_is_secret_material(&name) {
            out.push(name);
        }
    }
    out
}

/// Every non-scaffolding `.rs` file the scan will read, repo-relative.
///
/// Split out from [`findings`] so the anchor test can assert the traversal
/// actually reached the workspace. Once the unmigrated set is empty, "found
/// nothing" is the *correct* answer, so a findings-count anchor would have to
/// be deleted — and deleting it is exactly how a scan that silently stops
/// reading files starts passing every test above vacuously.
fn scanned_files() -> Vec<(PathBuf, String)> {
    let root = repo_root();
    let mut scan_dirs: Vec<PathBuf> = workspace_member_dirs()
        .iter()
        .map(|m| root.join(m))
        .collect();
    scan_dirs.push(root.join("src"));

    let mut files = Vec::new();
    for dir in &scan_dirs {
        rust_files_under(dir, &mut files);
    }
    files.sort();
    files.dedup();

    files
        .into_iter()
        .filter_map(|file| {
            let rel = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            (!is_test_or_build_scaffolding(&rel)).then_some((file, rel))
        })
        .collect()
}

fn findings() -> Vec<Finding> {
    let mut out = Vec::new();
    for (file, rel) in scanned_files() {
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();

        let mut current_struct: Option<String> = None;
        let mut struct_derives_debug = false;
        let mut in_test_mod = false;
        let mut test_depth: i32 = 0;

        for (idx, raw) in lines.iter().enumerate() {
            let line = raw.trim();

            if !in_test_mod && line.starts_with("#[cfg(test)]") {
                in_test_mod = true;
                test_depth = 0;
                continue;
            }
            if in_test_mod {
                test_depth += raw.matches('{').count() as i32;
                test_depth -= raw.matches('}').count() as i32;
                if test_depth <= 0 && raw.contains('}') {
                    in_test_mod = false;
                }
                continue;
            }

            // Leaving a type body for an impl or a free function: fields
            // found after this point belong to neither.
            if line.starts_with("impl ") || line.starts_with("pub fn ") || line.starts_with("fn ") {
                current_struct = None;
                struct_derives_debug = false;
                continue;
            }

            if let Some(rest) = line
                .strip_prefix("pub struct ")
                .or_else(|| line.strip_prefix("struct "))
                .or_else(|| line.strip_prefix("pub enum "))
                .or_else(|| line.strip_prefix("enum "))
            {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                // Look back for a derive naming Debug.
                struct_derives_debug = lines[idx.saturating_sub(6)..idx]
                    .iter()
                    .any(|l| l.contains("derive") && l.contains("Debug"));
                current_struct = Some(name.clone());

                // A one-line struct declares its fields on this same line.
                if struct_derives_debug {
                    for field in secret_string_fields(rest) {
                        out.push(Finding {
                            file: rel.clone(),
                            line: idx + 1,
                            struct_name: name.clone(),
                            field,
                        });
                    }
                }
                continue;
            }

            if !struct_derives_debug {
                continue;
            }
            let Some(struct_name) = current_struct.as_ref() else {
                continue;
            };
            // Whole-line scan, so enum variants with inline braces
            // (`Basic { username: String, password: String }`) are covered.
            for field in secret_string_fields(raw) {
                out.push(Finding {
                    file: rel.clone(),
                    line: idx + 1,
                    struct_name: struct_name.clone(),
                    field,
                });
            }
        }
    }
    out.sort();
    out
}

#[test]
fn no_new_plaintext_secret_fields_on_debug_structs() {
    let recorded: BTreeSet<(&str, &str)> = NOT_YET_MIGRATED
        .iter()
        .map(|(file, name, _)| (*file, *name))
        .collect();

    let violations: Vec<String> = findings()
        .into_iter()
        .filter(|f| !recorded.contains(&(f.file.as_str(), f.struct_name.as_str())))
        .map(|f| {
            format!(
                "  {}:{}  struct {} field `{}`",
                f.file, f.line, f.struct_name, f.field
            )
        })
        .collect();

    assert!(
        violations.is_empty(),
        "Credential held in a plain `String` on a Debug-deriving struct.\n\n{}\n\n\
         Use `soulsystem_common::secrets::SecretString`, whose Debug, Display and \
         Serialize all redact — so an enclosing #[derive(Debug)] is safe without \
         its author having to think about it. If the field is a secret *name* or a \
         token *count* rather than credential material, add the field name to \
         NOT_SECRET_MATERIAL in tests/architecture_secret_types.rs. See \
         docs/security/SECURITY_INVARIANTS.md (INV-SEC-2).",
        violations.join("\n")
    );
}

/// An entry that no longer matches is stale: it grants a standing exception
/// nobody needs, and hides the next real one.
#[test]
fn every_recorded_entry_is_still_real() {
    let found: BTreeSet<(String, String)> = findings()
        .into_iter()
        .map(|f| (f.file, f.struct_name))
        .collect();

    let mut stale = Vec::new();
    for (file, name, reason) in NOT_YET_MIGRATED {
        assert!(
            reason.len() > 20,
            "{file}::{name} has a token justification; say why it is not \
             migrated yet"
        );
        if !found.contains(&((*file).to_owned(), (*name).to_owned())) {
            stale.push(format!("{file}::{name}"));
        }
    }

    assert!(
        stale.is_empty(),
        "these entries no longer hold a plaintext secret field: {stale:?}. \
         Remove them from NOT_YET_MIGRATED and lower NOT_YET_MIGRATED_BUDGET so \
         the ratchet keeps holding."
    );
}

/// Pinned in both directions — see the constant's documentation.
///
/// Now that the budget is `0`, "does not grow" and "does not shrink" are the
/// same assertion, so this is written as one equality rather than two
/// comparisons that clippy correctly calls degenerate for `usize`. The
/// two-way intent is unchanged: adding a struct to the list without raising
/// the budget fails, and so does leaving a stale budget behind after a
/// migration.
#[test]
fn the_unmigrated_set_does_not_grow() {
    let count = NOT_YET_MIGRATED.len();
    assert_eq!(
        count, NOT_YET_MIGRATED_BUDGET,
        "the recorded-unmigrated count is {count} but the budget is \
         {NOT_YET_MIGRATED_BUDGET}. If a struct was added to NOT_YET_MIGRATED, \
         use SecretString for it instead of extending the list; if one was \
         migrated, set NOT_YET_MIGRATED_BUDGET to {count}."
    );
}

/// The structs this sweep migrated must stay migrated.
#[test]
fn the_migrated_structs_use_the_secret_type() {
    for (rel, needle) in [
        ("src/ws_bridge.rs", "shared_secret: Option<SecretString>"),
        ("soul_llm/src/types.rs", "auth_token: Option<SecretString>"),
        (
            "soul_openclaw/src/lib.rs",
            "auth_token: Option<SecretString>",
        ),
    ] {
        let source = std::fs::read_to_string(repo_root().join(rel))
            .unwrap_or_else(|_| panic!("{rel} must be readable"));
        assert!(
            source.contains(needle),
            "{rel} no longer declares `{needle}`; a migrated credential was \
             reverted to a plain String"
        );
    }
}

/// If the scan stopped *reading files*, every test above would pass vacuously.
///
/// This used to anchor on the number of findings, which worked only while
/// unmigrated structs still existed. Now that the set is empty, "found zero
/// secret fields" is the correct answer — so the anchor asserts the traversal
/// reached the workspace instead. A scan that silently reads nothing is the
/// failure mode worth catching; a scan that reads everything and finds nothing
/// is the goal.
#[test]
fn the_scan_actually_reads_the_workspace() {
    let files = scanned_files();
    assert!(
        files.len() > 500,
        "the scan reached only {} source files; member parsing or directory \
         traversal is broken, not the workspace",
        files.len()
    );

    let rels: BTreeSet<&str> = files.iter().map(|(_, rel)| rel.as_str()).collect();
    for expected in [
        "avid/crates/avid-cortex/src/llm.rs",
        "synergie/src/config.rs",
        "soullink-brain/soullink-gateway/src/ws/protocol.rs",
        "src/ws_bridge.rs",
    ] {
        assert!(
            rels.contains(expected),
            "the scan did not reach {expected}; member parsing is broken"
        );
    }
}

/// The matcher still reports a plaintext credential when it sees one.
///
/// With `NOT_YET_MIGRATED` empty, nothing in the workspace exercises the
/// positive path any more. This pins it against fixtures so the guard cannot
/// degrade into one that reports "clean" because it stopped recognising
/// anything.
#[test]
fn the_matcher_still_recognises_a_plaintext_credential() {
    for (line, expected) in [
        ("    pub api_key: String,", vec!["api_key"]),
        ("    pub auth_token: Option<String>,", vec!["auth_token"]),
        ("    Bearer { token: String },", vec!["token"]),
        (
            "pub struct P { pub api_key: String, pub password: String }",
            vec!["api_key", "password"],
        ),
    ] {
        assert_eq!(
            secret_string_fields(line),
            expected,
            "the matcher stopped recognising: {line}"
        );
    }
}

/// A field named `max_tokens` is a budget, not a credential. Redacting it would
/// make configuration undebuggable while protecting nothing.
#[test]
fn token_accounting_fields_are_not_treated_as_credentials() {
    for accounting in [
        "max_tokens",
        "goal_token_budget",
        "tokens_per_minute_budget",
        "prompt_tokens",
        "total_tokens",
    ] {
        assert!(
            !field_is_secret_material(accounting),
            "`{accounting}` is token accounting, not credential material"
        );
    }
    for credential in ["auth_token", "api_key", "shared_secret", "bot_token"] {
        assert!(
            field_is_secret_material(credential),
            "`{credential}` is credential material and must be caught"
        );
    }
}

/// A secret *name* addresses a credential in the OS store; it is not the
/// credential. Redacting it would break configuration diagnostics.
#[test]
fn secret_names_are_not_treated_as_secret_material() {
    for name_field in ["secret_name", "secret_header", "token_url", "token_type"] {
        assert!(
            !field_is_secret_material(name_field),
            "`{name_field}` names or describes a secret rather than holding one"
        );
    }
}

/// A field already migrated must not still be reported, or the guard would
/// demand migrating it twice.
#[test]
fn an_already_migrated_field_is_not_flagged() {
    assert_eq!(
        secret_string_fields("    pub auth_token: Option<String>,"),
        vec!["auth_token"]
    );
    assert!(
        secret_string_fields("    pub auth_token: Option<SecretString>,").is_empty(),
        "a migrated field must not still be reported"
    );
    assert_eq!(
        secret_string_fields("    pub api_key: String,"),
        vec!["api_key"]
    );
    assert!(secret_string_fields("    pub api_key: SecretString,").is_empty());

    // The shapes an earlier version of this guard silently missed.
    assert_eq!(
        secret_string_fields("pub struct P { pub api_key: String, pub model: String }"),
        vec!["api_key"],
        "a one-line struct declares its fields on the declaration line"
    );
    assert_eq!(
        secret_string_fields("    Basic { username: String, password: String },"),
        vec!["password"],
        "an enum variant with inline braces holds fields too"
    );
    assert!(
        secret_string_fields("    pub tokens: Vec<String>,").is_empty(),
        "a collection of strings is not a credential field"
    );
}
