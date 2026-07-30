//! Manifest-consistency guard (LOW-002).
//!
//! The root binary declared `version = "0.6.0"` while `[workspace.package]`
//! carried `13.5.0`, so `soulsystem --version` reported a number that matched
//! no release the workspace ever produced. That is a supply-chain legibility
//! problem more than a vulnerability: an operator who reports "0.6.0 is
//! misbehaving" cannot be mapped to a commit, and an advisory naming a
//! version cannot be matched against what is deployed.
//!
//! The fix was `version.workspace = true`. This guard exists because the fix
//! is one line that a future edit can silently undo.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// The root package inherits its version rather than restating it.
///
/// Checked against the manifest text, not `cargo metadata`: metadata reports
/// the *resolved* value, which is identical whether it was inherited or
/// hand-copied correctly today — and a hand-copied value is exactly what
/// drifts tomorrow. The point is the mechanism, not this moment's number.
#[test]
fn the_root_package_inherits_the_workspace_version() {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("root Cargo.toml must be readable");

    let package_block = manifest
        .split_once("\n[package]")
        .expect("root manifest declares a [package] section")
        .1;
    // Stop at the next top-level section so we only read [package]'s own keys.
    let package_block = package_block
        .split_once("\n[")
        .map(|(before, _)| before)
        .unwrap_or(package_block);

    assert!(
        package_block.contains("version.workspace = true")
            || package_block.contains("version = { workspace = true }"),
        "the root [package] must inherit its version from [workspace.package] \
         (LOW-002). Found instead:\n{}\nA literal version here drifts from the \
         workspace and makes `soulsystem --version` unmappable to a release.",
        package_block
            .lines()
            .find(|l| l.trim_start().starts_with("version"))
            .unwrap_or("(no version key at all)")
    );
}

/// `[workspace.package]` still carries a concrete version for members to
/// inherit — the previous check passes vacuously if this one is missing.
#[test]
fn the_workspace_declares_a_version_to_inherit() {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("root Cargo.toml must be readable");
    let ws = manifest
        .split_once("[workspace.package]")
        .expect("[workspace.package] must exist for members to inherit from")
        .1;
    let version_line = ws
        .lines()
        .find(|l| l.trim_start().starts_with("version"))
        .expect("[workspace.package] declares a version");
    assert!(
        version_line.contains('"'),
        "[workspace.package] version must be a concrete literal, found: {version_line}"
    );
}
