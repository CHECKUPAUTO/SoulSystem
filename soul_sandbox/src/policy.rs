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
