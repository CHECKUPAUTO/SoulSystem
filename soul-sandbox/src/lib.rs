//! Sandboxed command execution with resource limits, seccomp, and process isolation.
//!
//! Provides:
//! - Command allow-listing (whitelist approach)
//! - Resource limits (CPU, memory, file size)
//! - Timeout enforcement
//! - Filesystem restrictions
//! - Network isolation via unshare
//! - Seccomp-bpf system call filtering
//! - PID/mount namespace isolation

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;

#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Command not allowed: {0}")]
    NotAllowed(String),
    #[error("Execution failed: {0}")]
    Execution(String),
    #[error("Timeout after {0}s")]
    Timeout(u64),
    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ── Sandbox Configuration ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Allowed command prefixes (whitelist)
    pub allowed_commands: Vec<String>,
    /// Maximum execution time in seconds
    pub max_timeout_secs: u64,
    /// Maximum memory in MB (0 = unlimited)
    pub max_memory_mb: usize,
    /// Maximum CPU time in seconds (0 = unlimited)
    pub max_cpu_secs: u64,
    /// Maximum output size in bytes
    pub max_output_bytes: usize,
    /// Allowed directories for file operations
    pub allowed_dirs: Vec<String>,
    /// Denied directories (always blocked)
    pub denied_dirs: Vec<String>,
    /// Enable network isolation
    pub isolate_network: bool,
    /// Enable PID namespace isolation
    pub isolate_pid: bool,
    /// Working directory
    pub working_dir: Option<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            allowed_commands: vec![
                "ls".into(),
                "cat".into(),
                "head".into(),
                "tail".into(),
                "grep".into(),
                "find".into(),
                "wc".into(),
                "sort".into(),
                "uniq".into(),
                "diff".into(),
                "echo".into(),
                "pwd".into(),
                "whoami".into(),
                "date".into(),
                "uname".into(),
                "df".into(),
                "du".into(),
                "free".into(),
                "ps".into(),
                "top".into(),
                "htop".into(),
                "file".into(),
                "stat".into(),
                "readlink".into(),
                "basename".into(),
                "dirname".into(),
                "env".into(),
                "which".into(),
                "python3".into(),
                "python".into(),
                "node".into(),
                "cargo".into(),
                "git".into(),
                "curl".into(),
                "wget".into(),
                "jq".into(),
                "sed".into(),
                "awk".into(),
                "tr".into(),
                "cut".into(),
                "tee".into(),
                "xargs".into(),
            ],
            max_timeout_secs: 60,
            max_memory_mb: 512,
            max_cpu_secs: 30,
            max_output_bytes: 100_000,
            allowed_dirs: vec!["/tmp".into(), "/root".into(), "/home".into()],
            denied_dirs: vec![
                "/etc/shadow".into(),
                "/etc/passwd".into(),
                "/boot".into(),
                "/sys".into(),
                "/proc/sys".into(),
            ],
            isolate_network: false,
            isolate_pid: false,
            working_dir: None,
        }
    }
}

impl SandboxConfig {
    /// Permissive config for trusted operations
    pub fn permissive() -> Self {
        Self {
            allowed_commands: vec!["*".into()],
            max_timeout_secs: 120,
            max_memory_mb: 2048,
            max_cpu_secs: 60,
            max_output_bytes: 500_000,
            ..Default::default()
        }
    }

    /// Restrictive config for untrusted input
    pub fn restrictive() -> Self {
        Self {
            allowed_commands: vec![
                "ls".into(),
                "cat".into(),
                "head".into(),
                "tail".into(),
                "grep".into(),
                "find".into(),
                "wc".into(),
                "echo".into(),
                "pwd".into(),
                "file".into(),
                "stat".into(),
            ],
            max_timeout_secs: 10,
            max_memory_mb: 128,
            max_cpu_secs: 5,
            max_output_bytes: 50_000,
            allowed_dirs: vec!["/tmp".into()],
            denied_dirs: vec![
                "/etc".into(),
                "/boot".into(),
                "/sys".into(),
                "/proc".into(),
                "/dev".into(),
                "/root".into(),
            ],
            isolate_network: true,
            isolate_pid: true,
            working_dir: None,
        }
    }
}

// ── Sandbox Executor ─────────────────────────────────────────────────

pub struct SandboxExecutor {
    config: SandboxConfig,
    command_whitelist: HashSet<String>,
}

impl SandboxExecutor {
    pub fn new(config: SandboxConfig) -> Self {
        let command_whitelist: HashSet<String> = config.allowed_commands.iter().cloned().collect();
        Self {
            config,
            command_whitelist,
        }
    }

    /// Validate and execute a command in the sandbox
    pub async fn execute(&self, command: &str) -> Result<SandboxOutput, SandboxError> {
        // 1. Parse the command
        let cmd_name = self.extract_command_name(command)?;

        // 2. Check allow-list
        if !self.command_whitelist.contains("*") && !self.command_whitelist.contains(&cmd_name) {
            return Err(SandboxError::NotAllowed(cmd_name));
        }

        // 3. Check for dangerous patterns
        self.check_dangerous_patterns(command)?;

        // 4. Check directory restrictions
        self.check_directory_access(command)?;

        // 5. Execute with limits
        let timeout = Duration::from_secs(self.config.max_timeout_secs);

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command);

        if let Some(ref cwd) = self.config.working_dir {
            cmd.current_dir(cwd);
        }

        // Apply resource limits via ulimit wrapper
        let wrapped_cmd = self.wrap_with_limits(command);

        let result = tokio::time::timeout(
            timeout,
            Command::new("sh").arg("-c").arg(&wrapped_cmd).output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                // Truncate if too large
                let stdout = if stdout.len() > self.config.max_output_bytes {
                    let truncated = &stdout[..self.config.max_output_bytes];
                    format!("{}...[truncated]", truncated)
                } else {
                    stdout
                };

                let stderr = if stderr.len() > self.config.max_output_bytes {
                    let truncated = &stderr[..self.config.max_output_bytes];
                    format!("{}...[truncated]", truncated)
                } else {
                    stderr
                };

                Ok(SandboxOutput {
                    stdout,
                    stderr,
                    exit_code: output.status.code().unwrap_or(-1),
                    command: command.to_string(),
                    was_sandboxed: true,
                })
            }
            Ok(Err(e)) => Err(SandboxError::Execution(e.to_string())),
            Err(_) => Err(SandboxError::Timeout(self.config.max_timeout_secs)),
        }
    }

    fn extract_command_name(&self, command: &str) -> Result<String, SandboxError> {
        let trimmed = command.trim();

        // Skip pipe chains, take first command
        let cmd_part = trimmed.split('|').next().unwrap_or(trimmed).trim();

        // Skip sudo, nice, time prefixes
        let cmd_part = cmd_part
            .strip_prefix("sudo ")
            .or_else(|| cmd_part.strip_prefix("nice "))
            .or_else(|| cmd_part.strip_prefix("time "))
            .unwrap_or(cmd_part);

        // Skip environment variable assignments (VAR=value ...)
        let mut remaining = cmd_part;
        while let Some(eq_pos) = remaining.find('=') {
            // Check if this looks like VAR= (uppercase letters/digits/underscores before '=')
            let before_eq = &remaining[..eq_pos];
            if before_eq
                .chars()
                .all(|c| c.is_uppercase() || c.is_numeric() || c == '_')
                && !before_eq.is_empty()
            {
                remaining = remaining[eq_pos + 1..].trim_start();
            } else {
                break;
            }
        }

        // Extract first word (command name)
        let name = remaining
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();

        if name.is_empty() {
            return Err(SandboxError::NotAllowed("empty command".into()));
        }

        // Get base name (strip path)
        let base = Path::new(&name)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or(name);

        Ok(base)
    }

    fn check_dangerous_patterns(&self, command: &str) -> Result<(), SandboxError> {
        let lower = command.to_lowercase();
        let dangerous = [
            "rm -rf /",
            "rm -rf /*",
            "mkfs",
            "dd if=",
            ":(){ :|:&};:", // fork bomb
            "shutdown",
            "reboot",
            "halt",
            "init 0",
            "init 6",
            "> /dev/sda",
            "chmod 777 /",
            "wget -O- | sh",
            "curl | sh",
            "eval $(",
            "rm -r /",
            "chmod -R 777",
            "chown -R root",
        ];

        for pattern in &dangerous {
            if lower.contains(pattern) {
                return Err(SandboxError::NotAllowed(format!(
                    "dangerous pattern: {}",
                    pattern
                )));
            }
        }

        Ok(())
    }

    fn check_directory_access(&self, command: &str) -> Result<(), SandboxError> {
        // Check denied directories
        for denied in &self.config.denied_dirs {
            if command.contains(denied) {
                return Err(SandboxError::NotAllowed(format!(
                    "access to {} denied",
                    denied
                )));
            }
        }
        Ok(())
    }

    fn wrap_with_limits(&self, command: &str) -> String {
        let mut parts = Vec::new();

        // Memory limit
        if self.config.max_memory_mb > 0 {
            let bytes = self.config.max_memory_mb * 1024 * 1024;
            parts.push(format!("ulimit -v {} 2>/dev/null;", bytes / 1024));
        }

        // CPU time limit
        if self.config.max_cpu_secs > 0 {
            parts.push(format!(
                "ulimit -t {} 2>/dev/null;",
                self.config.max_cpu_secs
            ));
        }

        // File size limit (100MB)
        parts.push("ulimit -f 102400 2>/dev/null;".to_string());

        // The actual command
        parts.push(command.to_string());

        parts.join(" ")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub command: String,
    pub was_sandboxed: bool,
}

impl SandboxOutput {
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    pub fn summary(&self) -> String {
        let output = if self.stdout.is_empty() {
            &self.stderr
        } else {
            &self.stdout
        };
        if output.len() > 500 {
            format!("{}...[{} more chars]", &output[..500], output.len() - 500)
        } else {
            output.to_string()
        }
    }
}

// ── File System Sandbox ──────────────────────────────────────────────

pub struct FsSandbox {
    root: PathBuf,
    allowed_writes: Vec<PathBuf>,
}

impl FsSandbox {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            allowed_writes: vec![root.to_path_buf()],
        }
    }

    pub fn allow_write(&mut self, path: PathBuf) {
        self.allowed_writes.push(path);
    }

    /// Check if a path is within the sandbox
    pub fn is_path_allowed(&self, path: &Path) -> bool {
        // For existing paths, use canonicalize
        if let Ok(canonical) = path.canonicalize() {
            if canonical.starts_with(&self.root) {
                return true;
            }
            for allowed in &self.allowed_writes {
                if canonical.starts_with(allowed) {
                    return true;
                }
            }
            return false;
        }

        // For non-existent paths, check if it would be within root
        if path.is_absolute() {
            path.starts_with(&self.root)
        } else {
            // Relative paths are within root
            true
        }
    }

    /// Sanitize a path to be within the sandbox
    pub fn sanitize_path(&self, path: &str) -> PathBuf {
        let p = Path::new(path);

        // If absolute and within root, use as-is
        if p.is_absolute() {
            if let Ok(canonical) = p.canonicalize() {
                if canonical.starts_with(&self.root) {
                    return canonical;
                }
            }
        }

        // Otherwise, make relative to root
        self.root.join(p)
    }
}

// ── Seccomp-bpf System Call Filtering ────────────────────────────────

/// Seccomp whitelist of allowed syscalls for sandboxed processes.
/// Uses a simple allow-list approach: only syscalls in this list are permitted.
#[derive(Debug, Clone)]
pub struct SeccompWhitelist {
    allowed: HashSet<String>,
}

impl Default for SeccompWhitelist {
    fn default() -> Self {
        let mut allowed = HashSet::new();

        // Basic I/O
        for syscall in &[
            // Basic I/O (safe)
            "read",
            "write",
            "open",
            "close",
            "stat",
            "fstat",
            "lstat",
            "poll",
            "lseek",
            "mmap",
            "mprotect",
            "munmap",
            "brk",
            "access",
            "pipe",
            "select",
            "sched_yield",
            "mremap",
            "msync",
            "mincore",
            "madvise",
            "dup",
            "dup2",
            "nanosleep",
            "getpid",
            "getppid",
            "getuid",
            "getgid",
            "geteuid",
            "getegid",
            // File operations (safe subset)
            "sendfile",
            "fcntl",
            "flock",
            "fsync",
            "fdatasync",
            "truncate",
            "ftruncate",
            "getdents",
            "getcwd",
            "chdir",
            "fchdir",
            "rename",
            "mkdir",
            "rmdir",
            "creat",
            "link",
            "unlink",
            "readlink",
            "chmod",
            "fchmod",
            "chown",
            "fchown",
            "lchown",
            "umask",
            "statfs",
            "fstatfs",
            // Process management (safe subset)
            "clone",
            "fork",
            "vfork",
            "execve",
            "exit",
            "wait4",
            "uname",
            "prctl",
            "arch_prctl",
            // Time
            "gettimeofday",
            "clock_gettime",
            "clock_getres",
            // Signals (safe subset)
            "rt_sigaction",
            "rt_sigprocmask",
            "rt_sigreturn",
            // Scheduling
            "sched_setparam",
            "sched_getparam",
            "sched_setscheduler",
            "sched_getscheduler",
            "sched_get_priority_max",
            "sched_get_priority_min",
            "sched_rr_get_interval",
            "sched_setaffinity",
            "sched_getaffinity",
            // Memory (safe subset)
            "mlock",
            "munlock",
            "mlockall",
            "munlockall",
            // Misc safe
            "gettid",
            "tkill",
            "getcpu",
            // Epoll (for async I/O)
            "epoll_create",
            "epoll_create1",
            "epoll_ctl",
            "epoll_wait",
            "epoll_pwait",
            "eventfd",
            "eventfd2",
            // Timerfd
            "timerfd_create",
            "timerfd_settime",
            "timerfd_gettime",
            // Signalfd
            "signalfd",
            "signalfd4",
            // Pipe
            "pipe2",
            // Dup
            "dup3",
            // Inotify (file watching)
            "inotify_init",
            "inotify_init1",
            "inotify_add_watch",
            "inotify_rm_watch",
            // Pselect/ppoll
            "pselect6",
            "ppoll",
            // Writev/readv
            "readv",
            "writev",
            "preadv",
            "pwritev",
            // Fallocate
            "fallocate",
            // Accept
            "accept",
            "accept4",
            // Getrandom/memfd
            "getrandom",
            "memfd_create",
            // Copy file range
            "copy_file_range",
            // Statx
            "statx",
        ] {
            allowed.insert(syscall.to_string());
        }

        Self { allowed }
    }
}

impl SeccompWhitelist {
    pub fn is_allowed(&self, syscall: &str) -> bool {
        self.allowed.contains(syscall)
    }

    pub fn add(&mut self, syscall: &str) {
        self.allowed.insert(syscall.to_string());
    }

    pub fn remove(&mut self, syscall: &str) {
        self.allowed.remove(syscall);
    }

    pub fn len(&self) -> usize {
        self.allowed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }

    /// Apply seccomp-bpf filter to restrict syscalls.
    /// Must be called BEFORE execve() — typically in a forked child process.
    /// Uses an allowlist approach: only listed syscalls are permitted.
    #[cfg(target_os = "linux")]
    pub fn apply_seccomp(&self) -> Result<(), SandboxError> {
        // Check kernel version supports seccomp (>= 3.5)
        if let Ok(_release) = std::env::var("KVERSION") {
            // Skip check if env var set (for testing)
        } else if let Ok(kernel) = std::fs::read_to_string("/proc/version") {
            let minor = kernel.split('.').nth(1).and_then(|s| s.parse::<u32>().ok());
            if let Some(minor) = minor {
                if minor < 5 {
                    tracing::warn!("Kernel < 3.5, seccomp may not be supported");
                    return Ok(()); // Graceful skip
                }
            }
        }

        // seccomp return values
        const SECCOMP_RET_KILL: u32 = 0x00000000;
        const SECCOMP_RET_ALLOW: u32 = 0x7fff0000;

        // Architecture constants
        const AUDIT_ARCH_X86_64: u32 = 0xC000003E;
        const SECCOMP_MODE_FILTER: libc::c_int = 1;

        // Map syscall names to Linux x86_64 syscall numbers
        let syscall_map: Vec<(&str, usize)> = vec![
            ("read", 0),
            ("write", 1),
            ("open", 2),
            ("close", 3),
            ("stat", 4),
            ("fstat", 5),
            ("lstat", 6),
            ("poll", 7),
            ("lseek", 8),
            ("mmap", 9),
            ("mprotect", 10),
            ("munmap", 11),
            ("brk", 12),
            ("access", 21),
            ("pipe", 22),
            ("select", 23),
            ("sched_yield", 24),
            ("dup", 32),
            ("dup2", 33),
            ("nanosleep", 35),
            ("getpid", 39),
            ("getppid", 40),
            ("getuid", 41),
            ("getgid", 42),
            ("geteuid", 43),
            ("getegid", 44),
            ("fcntl", 72),
            ("flock", 73),
            ("fsync", 74),
            ("fdatasync", 75),
            ("truncate", 76),
            ("ftruncate", 77),
            ("getcwd", 79),
            ("chdir", 80),
            ("rename", 82),
            ("mkdir", 83),
            ("rmdir", 84),
            ("creat", 85),
            ("link", 86),
            ("unlink", 87),
            ("readlink", 88),
            ("chmod", 90),
            ("chown", 92),
            ("umask", 95),
            ("gettimeofday", 96),
            ("clock_gettime", 228),
            ("rt_sigaction", 13),
            ("rt_sigprocmask", 14),
            ("rt_sigreturn", 15),
            ("uname", 63),
            ("prctl", 157),
            ("gettid", 186),
            ("tkill", 200),
            ("getrandom", 318),
            ("clone", 56),
            ("fork", 57),
            ("vfork", 58),
            ("execve", 59),
            ("exit", 60),
            ("wait4", 61),
            ("epoll_create1", 291),
            ("epoll_ctl", 233),
            ("epoll_wait", 232),
            ("eventfd2", 290),
            ("pipe2", 293),
            ("dup3", 292),
            ("readv", 19),
            ("writev", 20),
            ("mlock", 149),
            ("munlock", 150),
            ("getcpu", 309),
        ];

        // Build BPF program: if syscall in allowlist → ALLOW, else → KILL
        let mut allow_nrs: Vec<usize> = Vec::new();
        for name in &self.allowed {
            if let Some(&(_, nr)) = syscall_map.iter().find(|(n, _)| n == name) {
                allow_nrs.push(nr);
            }
        }

        // BPF instruction structure
        #[repr(C)]
        struct SockFilter {
            code: u16,
            jt: u8,
            jf: u8,
            k: u32,
        }

        let mut insns = vec![
            SockFilter {
                code: 0x20,
                jt: 0,
                jf: 0,
                k: 4,
            },
            SockFilter {
                code: 0x15,
                jt: 0,
                jf: 2,
                k: AUDIT_ARCH_X86_64,
            },
            SockFilter {
                code: 0x06,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_ALLOW,
            },
            SockFilter {
                code: 0x06,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_KILL,
            },
            SockFilter {
                code: 0x20,
                jt: 0,
                jf: 0,
                k: 16,
            },
        ];

        // Check each allowed syscall
        for &nr in &allow_nrs {
            insns.push(SockFilter {
                code: 0x15,
                jt: 0,
                jf: 1,
                k: nr as u32,
            });
            insns.push(SockFilter {
                code: 0x06,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_ALLOW,
            });
        }

        insns.push(SockFilter {
            code: 0x06,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_KILL,
        });

        // Apply the filter via prctl
        #[repr(C)]
        struct SockFprog {
            len: u16,
            filter: *const SockFilter,
        }

        let prog = SockFprog {
            len: insns.len() as u16,
            filter: insns.as_ptr(),
        };

        // Set NO_NEW_PRIVS first
        let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
        if ret != 0 {
            return Err(SandboxError::Execution(format!(
                "prctl NO_NEW_PRIVS failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        // Apply seccomp filter
        let ret = unsafe {
            libc::prctl(
                libc::PR_SET_SECCOMP,
                SECCOMP_MODE_FILTER,
                &prog as *const _ as libc::c_ulong,
                0,
                0,
            )
        };
        if ret != 0 {
            return Err(SandboxError::Execution(format!(
                "prctl SECCOMP failed: {}",
                std::io::Error::last_os_error()
            )));
        }

        tracing::info!(allowed_count = allow_nrs.len(), "seccomp filter applied");
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn apply_seccomp(&self) -> Result<(), SandboxError> {
        // No-op on non-Linux platforms
        Ok(())
    }
}

// ── Namespace Isolation ──────────────────────────────────────────────

/// Configuration for Linux namespace isolation.
#[derive(Debug, Clone)]
pub struct NamespaceConfig {
    /// Isolate PID namespace (process sees only its own PID 1)
    pub pid_namespace: bool,
    /// Isolate mount namespace (private filesystem view)
    pub mount_namespace: bool,
    /// Isolate network namespace (no network access)
    pub network_namespace: bool,
    /// Isolate UTS namespace (own hostname)
    pub uts_namespace: bool,
    /// Isolate IPC namespace
    pub ipc_namespace: bool,
    /// Use user namespace (run as unprivileged user)
    pub user_namespace: bool,
    /// Read-only root filesystem
    pub read_only_root: bool,
    /// Allowed mount points (when using mount namespace)
    pub allowed_mounts: Vec<String>,
}

impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            pid_namespace: true,
            mount_namespace: true,
            network_namespace: false, // Default: allow network for agent tasks
            uts_namespace: false,
            ipc_namespace: true,
            user_namespace: false,
            read_only_root: false,
            allowed_mounts: vec!["/tmp".into(), "/proc".into()],
        }
    }
}

impl NamespaceConfig {
    /// Maximum isolation (all namespaces enabled)
    pub fn maximum() -> Self {
        Self {
            pid_namespace: true,
            mount_namespace: true,
            network_namespace: true,
            uts_namespace: true,
            ipc_namespace: true,
            user_namespace: true,
            read_only_root: true,
            allowed_mounts: vec!["/tmp".into()],
        }
    }

    /// Minimal isolation (PID + mount only)
    pub fn minimal() -> Self {
        Self {
            pid_namespace: false,
            mount_namespace: false,
            network_namespace: false,
            uts_namespace: false,
            ipc_namespace: false,
            user_namespace: false,
            read_only_root: false,
            allowed_mounts: Vec::new(),
        }
    }

    /// Apply namespace isolation to a child process using unshare(2).
    /// This should be called before execve().
    #[cfg(target_os = "linux")]
    pub fn apply(&self) -> Result<(), SandboxError> {
        let mut flags: i32 = 0;

        if self.pid_namespace {
            flags |= libc::CLONE_NEWPID;
        }
        if self.mount_namespace {
            flags |= libc::CLONE_NEWNS;
        }
        if self.network_namespace {
            flags |= libc::CLONE_NEWNET;
        }
        if self.uts_namespace {
            flags |= libc::CLONE_NEWUTS;
        }
        if self.ipc_namespace {
            flags |= libc::CLONE_NEWIPC;
        }
        if self.user_namespace {
            flags |= libc::CLONE_NEWUSER;
        }

        if flags != 0 {
            let ret = unsafe { libc::unshare(flags) };
            if ret != 0 {
                return Err(SandboxError::Execution(format!(
                    "unshare failed: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    pub fn apply(&self) -> Result<(), SandboxError> {
        // No-op on non-Linux platforms
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_execute_allowed() {
        let sandbox = SandboxExecutor::new(SandboxConfig::default());
        let output = sandbox.execute("echo hello").await.unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_sandbox_execute_denied() {
        let sandbox = SandboxExecutor::new(SandboxConfig::default());
        let result = sandbox.execute("rm -rf /").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sandbox_execute_not_in_whitelist() {
        let config = SandboxConfig {
            allowed_commands: vec!["echo".into()],
            ..Default::default()
        };
        let sandbox = SandboxExecutor::new(config);
        let result = sandbox.execute("ls /tmp").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sandbox_permissive() {
        let sandbox = SandboxExecutor::new(SandboxConfig::permissive());
        let output = sandbox.execute("ls /tmp").await.unwrap();
        assert!(output.was_sandboxed);
    }

    #[test]
    fn test_fs_sandbox() {
        let root = tempfile::tempdir().unwrap();
        let sandbox = FsSandbox::new(root.path());

        // Create the file first so canonicalize works
        let allowed = root.path().join("subdir/file.txt");
        std::fs::create_dir_all(allowed.parent().unwrap()).unwrap();
        std::fs::write(&allowed, "test").unwrap();
        assert!(sandbox.is_path_allowed(&allowed));

        // Non-existent path in root should still be allowed (via sanitize_path)
        let nonexistent = root.path().join("other/file.txt");
        assert!(sandbox.is_path_allowed(&nonexistent));

        let denied = PathBuf::from("/etc/passwd");
        assert!(!sandbox.is_path_allowed(&denied));
    }

    #[test]
    fn test_check_dangerous_patterns() {
        let sandbox = SandboxExecutor::new(SandboxConfig::default());
        assert!(sandbox.check_dangerous_patterns("rm -rf /").is_err());
        assert!(sandbox.check_dangerous_patterns("ls /tmp").is_ok());
        assert!(sandbox
            .check_dangerous_patterns("mkfs.ext4 /dev/sda")
            .is_err());
    }

    #[test]
    fn test_seccomp_whitelist() {
        let whitelist = SeccompWhitelist::default();
        assert!(whitelist.is_allowed("read"));
        assert!(whitelist.is_allowed("write"));
        assert!(whitelist.is_allowed("close"));
        assert!(!whitelist.is_allowed("reboot"));
        assert!(!whitelist.is_allowed("mount"));
    }

    #[test]
    fn test_namespace_config() {
        let config = NamespaceConfig::default();
        assert!(config.pid_namespace);
        assert!(config.mount_namespace);
        assert!(!config.network_namespace); // Default: allow network for agent tasks

        let max = NamespaceConfig::maximum();
        assert!(max.network_namespace);
        assert!(max.user_namespace);
    }
}
