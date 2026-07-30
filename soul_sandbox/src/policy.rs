use std::collections::BTreeSet;
use std::time::Duration;

use crate::types::ThreatKind;

// Binaires qui sont *interdits* même en whitelist stricte (ils peuvent
// bypasse n'importe quel filtre via leur syntaxe propre).
pub const BANNED_BINARIES: &[&str] = &[
    "sudo", "su", "doas", "pkexec", "bash", "sh", "zsh", "fish", "csh", "ksh",
];

// Patterns dangereux.
pub const FORBIDDEN_PATTERNS: &[(&str, ThreatKind)] = &[
    ("rm -rf /", ThreatKind::DestructiveRecursive),
    ("rm -rf /*", ThreatKind::DestructiveRecursive),
    ("rm -rf ~", ThreatKind::DestructiveRecursive),
    ("rm -rf $HOME", ThreatKind::DestructiveRecursive),
    (":(){ :|:& };:", ThreatKind::ForkBomb),
    (" > /dev/sd", ThreatKind::RawDiskWrite),
    ("of=/dev/sd", ThreatKind::RawDiskWrite),
    ("dd if=", ThreatKind::RawDiskWrite),
    ("mkfs", ThreatKind::RawDiskWrite),
    ("tee /etc/", ThreatKind::SystemConfigWrite),
    (" > /etc/", ThreatKind::SystemConfigWrite),
    (" > /boot/", ThreatKind::SystemConfigWrite),
    (" > /sys/", ThreatKind::SystemConfigWrite),
    (" > /proc/", ThreatKind::SystemConfigWrite),
    (" > /var/", ThreatKind::SystemConfigWrite),
    (" > /root/", ThreatKind::SystemConfigWrite),
    ("| sh", ThreatKind::DownloadExec),
    ("| bash", ThreatKind::DownloadExec),
    ("| sh\n", ThreatKind::DownloadExec),
    ("| sudo", ThreatKind::ShellEscape),
    ("|su -", ThreatKind::ShellEscape),
    ("| nc ", ThreatKind::ShellEscape),
];

// Tokens shell-bypass.
pub const SHELL_BYPASS_TOKENS: &[(&str, ThreatKind)] = &[
    ("bash -c", ThreatKind::ShellBypass),
    ("sh -c", ThreatKind::ShellBypass),
    ("zsh -c", ThreatKind::ShellBypass),
    ("eval ", ThreatKind::EvalSource),
    ("eval(", ThreatKind::EvalSource),
    ("source ", ThreatKind::EvalSource),
    (". /", ThreatKind::EvalSource),
    ("exec ", ThreatKind::ShellBypass),
    ("env ", ThreatKind::ShellBypass),
    ("xargs ", ThreatKind::ShellBypass),
    ("-delete", ThreatKind::ShellBypass),
    (" -exec ", ThreatKind::ShellBypass),
];

// Chemins sensibles — interdiction d'accès (en lecture ou écriture).
pub const SENSITIVE_PATHS: &[&str] = &[
    "/etc/",
    "/etc",
    "/boot/",
    "/boot",
    "/proc/",
    "/proc",
    "/sys/",
    "/sys",
    "/var/",
    "/var",
    "/root/",
    "/root",
    "/dev/sd",
    "/dev/nvme",
    "/dev/hd",
    "/dev/vd",
    "/usr/lib/",
    "/usr/lib64/",
    "/lib/",
    "/lib64/",
    "/sbin/",
    "/bin/",
    "/var/log/",
    "/var/lib/sudo",
    "/etc/shadow",
    "/etc/passwd",
    "/etc/sudoers",
    "/etc/ssh/",
    "/root/.ssh/",
];

/// Per-execution resource ceilings, applied with `setrlimit(2)` in the child
/// between `fork` and `exec` (INV-EXEC-4).
///
/// # Why rlimits and not cgroups
///
/// The roadmap named cgroups. cgroup v2 can only bound a process the caller can
/// place in a cgroup it controls, which needs either root or an explicitly
/// delegated subtree (`Delegate=yes` on the unit, or a rootless systemd user
/// slice). SoulSystem is routinely run as an unprivileged user in environments
/// with no delegation — the same class of host that already defeats
/// `CLONE_NEWUSER` for network isolation. `setrlimit` needs no privilege at all
/// to *lower* a limit and is inherited across `exec`, so it is the bound that
/// actually applies on the hosts we run on. cgroups remain the better mechanism
/// where they are available; this does not implement them.
///
/// # What each limit does and does not do
///
/// Every field is `Option`: `None` leaves the inherited limit untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// `RLIMIT_AS` — maximum virtual address space, in bytes.
    ///
    /// Bounds `mmap`/`brk` rather than resident memory, so a process that
    /// reserves large sparse mappings without touching them is still refused.
    /// That is the conservative direction for a sandbox, but it is why the
    /// default is generous: runtimes that reserve big virtual arenas up front
    /// (Go, the JVM, sanitiser builds) can need far more address space than
    /// they ever fault in.
    pub max_address_space_bytes: Option<u64>,

    /// `RLIMIT_NPROC` — maximum processes, **counted per real UID**.
    ///
    /// Deliberately `None` by default. `RLIMIT_NPROC` is not per-process-tree:
    /// the kernel compares it against every process owned by the same real UID
    /// on the host. On any shared-UID host — a developer workstation, a CI
    /// runner — the ambient count already exceeds a fork-bomb-useful ceiling,
    /// so a non-`None` default would make the first `fork` of an ordinary
    /// command fail with `EAGAIN` while doing nothing to bound a determined
    /// caller. Enable it only when the sandbox runs under a dedicated UID; for
    /// shared-UID hosts the correct control is a cgroup `pids.max`, which this
    /// does not implement.
    pub max_processes: Option<u64>,

    /// `RLIMIT_NOFILE` — maximum open file descriptors.
    pub max_open_files: Option<u64>,

    /// cgroup v2 `pids.max` — the process-TREE task ceiling `RLIMIT_NPROC`
    /// cannot express on a shared-UID host (P1-12). Applied only when a
    /// delegated subtree is available (`SOUL_SANDBOX_CGROUP_DIR`); otherwise
    /// the sandbox reports the fallback once and relies on rlimits. The
    /// default contains a fork bomb without constraining ordinary builds.
    pub cgroup_pids_max: Option<u64>,

    /// cgroup v2 `memory.max` — resident memory, which `RLIMIT_AS` (address
    /// space) does not express. `None` by default: a wrong memory ceiling
    /// OOM-kills legitimate work, so the operator opts in per policy.
    pub cgroup_memory_max_bytes: Option<u64>,

    /// `RLIMIT_CPU` — maximum CPU seconds, summed across the process's threads.
    ///
    /// This is CPU time, not wall time, and is therefore not a substitute for
    /// [`SandboxPolicy::timeout`]: a process that sleeps forever burns no CPU.
    /// The default is deliberately looser than the default 30 s wall timeout so
    /// that an ordinary single-threaded overrun is terminated by the timeout
    /// path — which produces a proper verdict — rather than by `SIGXCPU`, while
    /// a thread-parallel CPU burner is still bounded.
    pub max_cpu_seconds: Option<u64>,

    /// `RLIMIT_FSIZE` — maximum size of any single file the process writes.
    ///
    /// Bounds a disk-fill via one large file. It does not bound many small
    /// files; nothing here does.
    pub max_file_size_bytes: Option<u64>,

    /// `RLIMIT_CORE` — maximum core dump size. Zero by default: a core dump of
    /// a sandboxed process would write process memory to disk outside the
    /// sandbox's control.
    pub max_core_bytes: Option<u64>,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_address_space_bytes: Some(4 * 1024 * 1024 * 1024),
            max_processes: None,
            max_open_files: Some(256),
            max_cpu_seconds: Some(60),
            max_file_size_bytes: Some(256 * 1024 * 1024),
            max_core_bytes: Some(0),
            cgroup_pids_max: Some(512),
            cgroup_memory_max_bytes: None,
        }
    }
}

impl ResourceLimits {
    /// Leave every inherited limit untouched.
    ///
    /// For callers that manage bounds themselves (a cgroup, a container
    /// runtime) and would otherwise apply two conflicting ceilings.
    pub fn unlimited() -> Self {
        Self {
            max_address_space_bytes: None,
            max_processes: None,
            max_open_files: None,
            max_cpu_seconds: None,
            max_file_size_bytes: None,
            max_core_bytes: None,
            cgroup_pids_max: None,
            cgroup_memory_max_bytes: None,
        }
    }

    /// Whether any ceiling is set.
    /// Whether any cgroup-expressible limit is requested.
    pub fn wants_cgroup(&self) -> bool {
        self.cgroup_pids_max.is_some() || self.cgroup_memory_max_bytes.is_some()
    }

    pub fn is_unlimited(&self) -> bool {
        *self == Self::unlimited()
    }
}

#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// Si Some, SEULES ces binaires sont autorisées.
    pub whitelist: Option<BTreeSet<String>>,
    /// Timeout par commande.
    pub timeout: Duration,
    /// Si vrai, journalise toutes les exécutions.
    pub log_all: bool,
    /// Si vrai, refuse l'accès aux chemins sensibles.
    pub block_sensitive_paths: bool,
    /// Si vrai, refuse les shell-bypass.
    pub block_shell_bypass: bool,
    /// Profil seccomp-BPF. `SandboxPolicy::default()` always sets this to
    /// `Some("default")` — every sandboxed execution runs under an active
    /// seccomp filter unless a caller explicitly opts out (`Some("unconfined")`
    /// or a custom profile), so isolation can never silently degrade to
    /// string-filtering-only.
    pub seccomp_profile: Option<String>,
    /// If true (the default), the sandboxed process attempts to run in its
    /// own network namespace with no configured interfaces — no network
    /// access at all, including loopback. Best-effort: on hosts that
    /// restrict unprivileged network-namespace creation (e.g. Ubuntu
    /// 23.10+'s default AppArmor policy, including standard GitHub Actions
    /// runners), isolation setup fails and the command still runs with host
    /// networking rather than refusing to execute — see
    /// `Sandbox::apply_sandbox_pre_exec` for why this is intentionally not
    /// fail-closed, unlike seccomp.
    pub network_isolated: bool,
    /// Maximum bytes captured from stdout (and, separately, stderr) per
    /// execution. Output beyond this cap is dropped rather than
    /// accumulated, bounding worst-case memory use regardless of how much
    /// a sandboxed command writes.
    pub max_output_bytes: usize,
    /// CPU, memory, file-descriptor and file-size ceilings applied to the
    /// child with `setrlimit(2)` before `exec` (INV-EXEC-4). See
    /// [`ResourceLimits`] for what each bound does, and for why this is
    /// `setrlimit` rather than cgroups.
    pub resource_limits: ResourceLimits,

    /// Working directory for the child, or the parent's cwd if `None`.
    ///
    /// Exists because callers that had one before they were sandboxed would
    /// otherwise lose it silently: a command that ran in `/srv/app` and now
    /// runs in whatever directory the daemon happens to be in does not fail,
    /// it does the wrong thing quietly. Carrying it explicitly is the
    /// difference between a migration and a regression.
    ///
    /// This is *not* a containment boundary. It sets where the child starts,
    /// not where it can reach — nothing stops it opening an absolute path
    /// elsewhere. Confinement is `SENSITIVE_PATHS`, seccomp and the rlimits;
    /// treating a cwd as a jail is a classic way to believe in a boundary that
    /// was never there.
    pub working_dir: Option<std::path::PathBuf>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            whitelist: None,
            timeout: Duration::from_secs(30),
            log_all: true,
            block_sensitive_paths: true,
            block_shell_bypass: true,
            seccomp_profile: Some("default".to_string()),
            network_isolated: true,
            max_output_bytes: 2 * 1024 * 1024,
            resource_limits: ResourceLimits::default(),
            working_dir: None,
        }
    }
}

impl SandboxPolicy {
    pub fn strict(binaries: &[&str]) -> Self {
        let mut set = BTreeSet::new();
        for b in binaries {
            if !BANNED_BINARIES.contains(b) {
                set.insert((*b).to_string());
            }
        }
        Self {
            whitelist: Some(set),
            ..Default::default()
        }
    }
}
