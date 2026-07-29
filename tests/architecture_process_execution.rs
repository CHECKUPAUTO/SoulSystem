//! Architecture guard for INV-EXEC-1 (see `docs/security/SECURITY_INVARIANTS.md`).
//!
//! Process execution is a security boundary: the approved path is
//! `soul_sandbox`, which applies seccomp, bounded output, a timeout and
//! process-group termination. Findings CRIT-001 and HIGH-002 both trace back to
//! the same root cause — a `Command` spawned somewhere nobody was looking, or a
//! security component that quietly lost its last caller.
//!
//! # Scope: every workspace member, not just the binary
//!
//! P0-1 pinned the root `soulsystem` binary crate only, because the rest of the
//! workspace had never been classified and enforcing over it would have
//! produced a wall of false positives. P1-8 does that classification: every
//! spawn site in every workspace member is now either allowlisted with a
//! category and a reason, or it fails this test.
//!
//! **What is deliberately out of scope.** Directories that are not workspace
//! members — `intel-integrations/ironclaw`, `openevolve`, `backlog`,
//! `os-agents`, `openclaw-evolution`, `soul-rsi`, `jit-agentic-engine`,
//! `soullink-node`, `turboquant`, `scirust-chronos-agent` — carry a further 112
//! spawn sites. They are not built by `cargo build --workspace`, so this guard
//! says nothing about them, and neither should anyone reading it. Claiming
//! otherwise would be the exact failure this file exists to prevent. Tracked as
//! P1-8-B.
//!
//! # The allowlist is a record, not an endorsement
//!
//! [`Category::UnsandboxedArbitraryCommand`] entries are **known problems**,
//! not blessed designs. They are listed so that (a) they cannot grow silently —
//! `unsandboxed_arbitrary_command_sites_do_not_grow` pins the count — and
//! (b) the register can state honestly how much unsandboxed execution the
//! workspace still contains. INV-EXEC-1 stays `PARTIAL` precisely because these
//! exist.
//!
//! When this test fails you have four honest options, in order of preference:
//!
//! 1. Route the new call through `soul_sandbox` and delete the bare `Command`.
//! 2. Replace the subprocess with a syscall or library call, as
//!    `SelfHealer::root_disk_used_percent` does instead of shelling out to `df`.
//! 3. If the call genuinely belongs outside the sandbox, add it to `ALLOWED`
//!    **with a category and a justification**, so the exception is reviewed
//!    rather than absorbed.
//! 4. If it is a new instance of unsandboxed arbitrary execution, do not add it.
//!    That is the thing being tracked down, not a pattern to extend.
//!
//! Do not silence this test by deleting it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Why a file is permitted to spawn a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    /// The file *is* an isolation boundary. Its `Command` is the sandboxed
    /// spawn — forbidding it would forbid the sandbox from existing.
    SandboxImplementation,
    /// A host-level operation that cannot be meaningfully confined (restarting
    /// a systemd unit, reading a device via a vendor tool). The control is that
    /// it is opt-in and takes no caller-controlled argv, not that it is
    /// confined.
    HostControlFixedArgv,
    /// Build or development tooling — `cargo`, `git` — invoked by
    /// self-modification and workflow paths. Runs on the host by design.
    DevTooling,
    /// Reachable production code that spawns a shell or a caller-influenced
    /// command outside any sandbox. **A known problem, recorded so it cannot
    /// grow silently.** Not an endorsement.
    UnsandboxedArbitraryCommand,
    /// Present but with no reachable non-test caller. Recorded so that wiring
    /// one up later is a visible change rather than a silent one.
    NoReachableCaller,
}

impl Category {
    fn as_str(self) -> &'static str {
        match self {
            Self::SandboxImplementation => "sandbox-implementation",
            Self::HostControlFixedArgv => "host-control-fixed-argv",
            Self::DevTooling => "dev-tooling",
            Self::UnsandboxedArbitraryCommand => "unsandboxed-arbitrary-command",
            Self::NoReachableCaller => "no-reachable-caller",
        }
    }
}

/// Files in workspace members permitted to spawn a process.
///
/// Paths are repo-relative and use forward slashes.
const ALLOWED: &[(&str, Category, &str)] = &[
    // ── The sandboxes themselves ─────────────────────────────────────────
    (
        "bound-system/src/lib.rs",
        Category::SandboxImplementation,
        "BoundSystem is the bubblewrap + seccomp confinement itself: `bwrap`, \
         and the `sh -c` it wraps. Forbidding this would forbid the sandbox.",
    ),
    (
        "soul_sandbox/src/lib.rs",
        Category::SandboxImplementation,
        "The approved execution path referenced by INV-EXEC-1. This is the \
         spawn every other caller is supposed to route through.",
    ),
    (
        "avid/crates/avid-sandbox/src/runner.rs",
        Category::SandboxImplementation,
        "AVID's `unshare`-based confinement for untrusted extraction scripts.",
    ),
    (
        "avid/crates/avid-sandbox/src/limits.rs",
        Category::SandboxImplementation,
        "`use std::process::Command` for the rlimit wrapper applied to the \
         sandboxed child.",
    ),
    (
        "soul-kernel/src/sandbox/mod.rs",
        Category::SandboxImplementation,
        "soul-kernel's own confinement path (copy-in, build, run). Separate \
         from soul_sandbox because soul-kernel ships as its own binary.",
    ),
    // ── Host control, fixed argv ─────────────────────────────────────────
    (
        "src/self_healer.rs",
        Category::HostControlFixedArgv,
        "RestartService invokes `systemctl restart <unit>`. Gated behind \
         ProcessControl::Enabled (default Disabled) and the unit name is \
         validated by is_safe_service_name. Not sandboxed because restarting a \
         host unit is inherently a host-level operation; the control is that it \
         is opt-in, not that it is confined.",
    ),
    (
        "soul_monitor/src/lib.rs",
        Category::HostControlFixedArgv,
        "Read-only host telemetry: `nvidia-smi`, `ss`, `ping`, `df`, `cat` on \
         fixed /proc paths. No caller-controlled argv. Should migrate to \
         syscalls as SelfHealer did for `df` — tracked, not blocking.",
    ),
    (
        "synergie/src/scanner/runtime.rs",
        Category::HostControlFixedArgv,
        "Read-only host inventory: `ss -Htnlp`, `ss -Hunlp`, `systemctl`. \
         Fixed argv, output parsed only.",
    ),
    (
        "soullink-brain/soullink-gateway/src/cli/banner.rs",
        Category::HostControlFixedArgv,
        "`stty` to read terminal width for CLI banner layout. Fixed argv.",
    ),
    (
        "soul_tools/src/lib.rs",
        Category::HostControlFixedArgv,
        "`which` to probe for a binary's presence. Fixed argv; the tool's \
         actual execution path is permission-gated elsewhere.",
    ),
    (
        "pty-terminal/src/lib.rs",
        Category::HostControlFixedArgv,
        "`tmux` session management behind `/api/pty/*`, plus `which tmux`. A \
         PTY multiplexer is the feature, not an escape from it; the control is \
         that the listener is authenticated and Exec-scoped (P1-9 / P1-9-B).",
    ),
    (
        "soul_bridges/src/lib.rs",
        Category::HostControlFixedArgv,
        "`docker` lifecycle calls and `ps` for container health. Fixed \
         subcommands; container names come from configuration, not requests.",
    ),
    (
        "soullink-brain/soullink-crypto-bridge/src/lib.rs",
        Category::HostControlFixedArgv,
        "`node` running a pinned bridge script path. Fixed argv.",
    ),
    // ── Build and development tooling ────────────────────────────────────
    (
        "soul-automodify/src/lib.rs",
        Category::DevTooling,
        "`cargo` build/test to validate a self-modification before it is \
         applied. Host-level by design: the point is to compile the real tree.",
    ),
    (
        "soul_evolution/src/optimizer.rs",
        Category::DevTooling,
        "`cargo` build/bench plus running the produced binary to score a \
         candidate optimisation.",
    ),
    (
        "soul-kernel/src/autocode/mod.rs",
        Category::DevTooling,
        "`git add` / `git commit` to record a generated change.",
    ),
    (
        "soullink-brain/soullink-workflow/src/git_worktree.rs",
        Category::DevTooling,
        "`git worktree` lifecycle for isolated workflow checkouts.",
    ),
    // ── Known unsandboxed arbitrary execution ────────────────────────────
    // These are the P1-8 findings, not P1-8's approvals. See the module header.
    (
        "soullink-brain/soullink-workflow/src/node.rs",
        Category::UnsandboxedArbitraryCommand,
        "A workflow node runs `sh -c <command>` with the command taken from \
         the workflow definition. Unsandboxed. Should route through \
         soul_sandbox.",
    ),
    (
        "soullink-brain/soullink-workflow/src/conditions.rs",
        Category::UnsandboxedArbitraryCommand,
        "A workflow condition evaluates `sh -c <command>`. Same problem as \
         node.rs, same fix.",
    ),
    (
        "soullink-brain/soullink-actions/src/shell.rs",
        Category::UnsandboxedArbitraryCommand,
        "The brain's shell action spawns `sh` directly rather than through \
         soul_sandbox.",
    ),
    (
        "soul-kernel/src/action/mod.rs",
        Category::UnsandboxedArbitraryCommand,
        "Kernel actions run `sh -c <cmd>`, `systemctl`, `iptables`, `chmod` \
         and a fixed `/usr/local/bin/claudex` path. The `sh -c` sites take \
         caller-supplied commands outside any sandbox.",
    ),
    (
        "soul-kernel/src/perception/mod.rs",
        Category::UnsandboxedArbitraryCommand,
        "Perception probes shell out via `sh` alongside fixed-argv `df` and \
         `systemctl` calls.",
    ),
    (
        "soul_automation/src/lib.rs",
        Category::UnsandboxedArbitraryCommand,
        "Automation steps run `sh` outside the sandbox.",
    ),
    (
        "soul-bridge/src/ccos.rs",
        Category::UnsandboxedArbitraryCommand,
        "Spawns a configured child binary with caller-influenced argv.",
    ),
    (
        "soul-bridge/src/octasoma.rs",
        Category::UnsandboxedArbitraryCommand,
        "Spawns a configured child binary with caller-influenced argv.",
    ),
    (
        "soullink-brain/soullink-orchestrator/src/main.rs",
        Category::UnsandboxedArbitraryCommand,
        "Runs `python3` against an orchestrator-supplied script.",
    ),
    (
        "soullink-orchestrator-standalone/src/routes/spawn.rs",
        Category::UnsandboxedArbitraryCommand,
        "An HTTP route spawns `python3`. Reachable from a request, which makes \
         this the highest-priority entry in this category.",
    ),
    (
        "soul-kernel/src/parallel/mod.rs",
        Category::UnsandboxedArbitraryCommand,
        "`sync` and `systemctl` invoked from the parallel executor without \
         going through the kernel's own sandbox module.",
    ),
    (
        "soul_gateway/src/providers/imessage.rs",
        Category::UnsandboxedArbitraryCommand,
        "`osascript` for iMessage delivery, with message text reaching the \
         AppleScript. macOS-only and disabled unless configured, but the \
         quoting boundary is exactly the kind this guard exists to surface.",
    ),
    // ── Present, no reachable caller ─────────────────────────────────────
    (
        "src/compute_backend.rs",
        Category::NoReachableCaller,
        "GPU capability probing (`nvidia-smi`, `rocm-smi`, `vulkaninfo`). \
         Read-only, fixed argv, no caller-controlled input. Currently has no \
         non-test caller at all (finding LOW-005, NOT_REACHABLE).",
    ),
];

/// How many files may currently hold [`Category::UnsandboxedArbitraryCommand`].
///
/// Pinned so the category cannot grow quietly. Lowering it as sites are fixed
/// is the intended direction; raising it should require saying so out loud in a
/// review.
const UNSANDBOXED_ARBITRARY_BUDGET: usize = 12;

/// Patterns that indicate a process spawn.
const SPAWN_PATTERNS: &[&str] = &[
    "std::process::Command",
    "tokio::process::Command",
    "Command::new",
    "libc::exec",
    "execve",
];

/// Occurrences that match [`SPAWN_PATTERNS`] but are not spawns.
///
/// `libc::SYS_execve` appears in `soul_sandbox`'s seccomp filter, where it is
/// the syscall number being *blocked*. Treating the seccomp policy that forbids
/// exec as itself an exec would force a bogus allowlist entry on the one file
/// most clearly doing the right thing.
const NOT_A_SPAWN: &[&str] = &["libc::SYS_execve", "libc::SYS_execveat"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Workspace member directories, parsed from the root manifest.
///
/// Read from `Cargo.toml` rather than hardcoded so that adding a member to the
/// workspace brings it under this guard automatically — a new crate should not
/// have to be remembered into the scan.
fn workspace_member_dirs() -> Vec<String> {
    let manifest = std::fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("root Cargo.toml must be readable");

    let members_block = manifest
        .split_once("members = [")
        .expect("root manifest declares workspace members")
        .1;
    let members_block = members_block
        .split_once(']')
        .expect("the members list is terminated")
        .0;

    let mut dirs = Vec::new();
    for line in members_block.lines() {
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
            // `target` holds build output, not source we are responsible for.
            if name == "target" || name == "node_modules" || name == ".git" {
                continue;
            }
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Whether a repo-relative path is test or build scaffolding.
///
/// Excluded by path rule rather than by allowlist entry: an integration test
/// that shells out to `git` to set up a fixture is not a production execution
/// path, and requiring a justification for each would bury the real entries.
fn is_test_or_build_scaffolding(rel: &str) -> bool {
    rel.ends_with("build.rs")
        || rel.contains("/tests/")
        || rel.starts_with("tests/")
        || rel.contains("/benches/")
        || rel.starts_with("benches/")
        || rel.contains("/examples/")
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

fn line_is_a_spawn(line: &str) -> Option<&'static str> {
    if NOT_A_SPAWN.iter().any(|exempt| line.contains(exempt)) {
        return None;
    }
    SPAWN_PATTERNS
        .iter()
        .find(|pattern| line.contains(**pattern))
        .copied()
}

/// Every production spawn site in every workspace member, as
/// `repo-relative path -> [(line, text)]`.
fn spawn_sites() -> BTreeMap<String, Vec<(usize, String)>> {
    let root = repo_root();
    let mut scan_dirs: Vec<PathBuf> = workspace_member_dirs()
        .iter()
        .map(|member| root.join(member))
        .collect();
    // The root crate's own source. Its manifest dir is the repo root, so it
    // cannot be walked wholesale without pulling in every other member.
    scan_dirs.push(root.join("src"));

    let mut files = Vec::new();
    for dir in &scan_dirs {
        rust_files_under(dir, &mut files);
    }
    files.sort();
    files.dedup();

    let mut sites = BTreeMap::new();
    for file in files {
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        if is_test_or_build_scaffolding(&rel) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&file) else {
            continue;
        };
        let hits: Vec<(usize, String)> = production_lines(&source)
            .into_iter()
            .filter(|(_, line)| line_is_a_spawn(line).is_some())
            .collect();
        if !hits.is_empty() {
            sites.insert(rel, hits);
        }
    }
    sites
}

#[test]
fn no_unapproved_process_execution_in_any_workspace_member() {
    let sites = spawn_sites();
    assert!(
        !sites.is_empty(),
        "found no spawn sites at all — the scan is broken, not the workspace"
    );

    let allowed: BTreeSet<&str> = ALLOWED.iter().map(|(p, _, _)| *p).collect();
    let mut violations = Vec::new();

    for (rel, hits) in &sites {
        if allowed.contains(rel.as_str()) {
            continue;
        }
        for (lineno, line) in hits {
            violations.push(format!("  {rel}:{lineno}\n    {}", line.trim()));
        }
    }

    assert!(
        violations.is_empty(),
        "Unapproved process execution found in a workspace member.\n\n{}\n\n\
         Process execution must go through `soul_sandbox` (INV-EXEC-1). Route it \
         through the sandbox, replace it with a syscall/library call, or add the \
         file to ALLOWED in tests/architecture_process_execution.rs with a \
         category and a justification. See docs/security/SECURITY_INVARIANTS.md \
         and finding CRIT-001.",
        violations.join("\n")
    );
}

/// An allowlist entry that no longer spawns anything is stale: it grants a
/// standing exception nobody needs, and hides the next real one.
#[test]
fn every_allowlist_entry_is_still_justified() {
    let root = repo_root();
    let mut stale = Vec::new();
    let mut missing = Vec::new();

    for (rel, _category, reason) in ALLOWED {
        assert!(
            reason.len() > 40,
            "allowlist entry {rel} has a token justification; say why the call \
             belongs outside the sandbox or do not allowlist it"
        );

        let path = root.join(rel);
        if !path.exists() {
            missing.push(*rel);
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("allowlisted file must be readable");
        let spawns = production_lines(&source)
            .iter()
            .any(|(_, line)| line_is_a_spawn(line).is_some());
        if !spawns {
            stale.push(*rel);
        }
    }

    assert!(
        missing.is_empty(),
        "these allowlist entries do not exist: {missing:?}. Remove them from \
         ALLOWED in tests/architecture_process_execution.rs."
    );
    assert!(
        stale.is_empty(),
        "these files are allowlisted for process execution but no longer spawn a \
         process: {stale:?}. Remove them from ALLOWED so the exception does not \
         outlive its reason."
    );
}

/// The allowlist must not accumulate duplicate entries for the same file, which
/// would let two different justifications coexist for one exception.
#[test]
fn the_allowlist_has_no_duplicate_entries() {
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for (rel, _, _) in ALLOWED {
        if !seen.insert(*rel) {
            duplicates.push(*rel);
        }
    }
    assert!(
        duplicates.is_empty(),
        "duplicate ALLOWED entries: {duplicates:?}"
    );
}

/// Unsandboxed arbitrary execution is what INV-EXEC-1 is about. The allowlist
/// records the existing sites so they are visible; this pins the count so the
/// category cannot grow while nobody is looking.
#[test]
fn unsandboxed_arbitrary_command_sites_do_not_grow() {
    let count = ALLOWED
        .iter()
        .filter(|(_, category, _)| *category == Category::UnsandboxedArbitraryCommand)
        .count();

    assert!(
        count <= UNSANDBOXED_ARBITRARY_BUDGET,
        "{count} files are allowlisted as {} but the budget is \
         {UNSANDBOXED_ARBITRARY_BUDGET}. These are known problems being tracked \
         down, not a pattern to extend: route the new call through soul_sandbox \
         instead. If a site was genuinely fixed, lower the budget in the same \
         change.",
        Category::UnsandboxedArbitraryCommand.as_str()
    );

    // Ratchet: if sites are fixed, the budget must come down with them, or it
    // stops constraining anything.
    assert!(
        count >= UNSANDBOXED_ARBITRARY_BUDGET,
        "only {count} files remain in {} but the budget is still \
         {UNSANDBOXED_ARBITRARY_BUDGET}. Lower UNSANDBOXED_ARBITRARY_BUDGET to \
         {count} so the ratchet keeps holding.",
        Category::UnsandboxedArbitraryCommand.as_str()
    );
}

/// The scan must actually reach the crates the allowlist talks about. If the
/// member parsing broke, every test above would pass vacuously.
#[test]
fn the_scan_reaches_crates_across_the_workspace() {
    let sites = spawn_sites();

    for expected in [
        "bound-system/src/lib.rs",
        "soul_sandbox/src/lib.rs",
        "soul-kernel/src/action/mod.rs",
        "soullink-brain/soullink-workflow/src/node.rs",
        "src/self_healer.rs",
    ] {
        assert!(
            sites.contains_key(expected),
            "the scan did not reach {expected}; workspace member parsing is \
             broken and the guard would pass vacuously"
        );
    }

    let crates_touched: BTreeSet<&str> = sites
        .keys()
        .map(|rel| rel.split('/').next().unwrap_or(rel))
        .collect();
    assert!(
        crates_touched.len() >= 10,
        "the scan only reached {} top-level directories; expected the guard to \
         span the workspace",
        crates_touched.len()
    );
}

/// The seccomp policy that *blocks* exec must not be misread as performing one.
#[test]
fn the_seccomp_filter_is_not_mistaken_for_a_spawn() {
    let sites = spawn_sites();
    assert!(
        !sites.contains_key("soul_sandbox/src/seccomp.rs"),
        "soul_sandbox/src/seccomp.rs was flagged as a spawn site, but its \
         `libc::SYS_execve` references are the syscall numbers being denied. \
         Forcing an allowlist entry there would misrepresent the one file most \
         clearly doing the right thing."
    );

    let seccomp = std::fs::read_to_string(repo_root().join("soul_sandbox/src/seccomp.rs"))
        .expect("soul_sandbox/src/seccomp.rs must be readable");
    assert!(
        seccomp.contains("SYS_execve"),
        "the exemption above is only sound while the filter still names \
         SYS_execve; if it stopped, the exemption would be hiding a real spawn"
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
