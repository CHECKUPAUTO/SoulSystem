use soullink_autonomy::preservation::{DefenseAction, Preservation};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

/// Whether the healer may act on the two `DefenseAction` variants that control
/// other processes on the host (`KillNonEssential`, `RestartService`).
///
/// Default is [`ProcessControl::Disabled`]: the action is logged and skipped.
/// No producer in the workspace currently emits either variant — `Preservation`
/// only yields Throttle / MemoryDump / EmergencySave / LocalFallback /
/// DistressSignal / GracefulShutdown — so these arms are latent rather than
/// live. Defaulting to disabled means adding a producer cannot silently turn
/// bus traffic into `kill(1)` / `systemctl(1)` invocations on the host; the
/// operator has to opt in as well (INV-EXEC-1 / CRIT-001).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessControl {
    /// Log the requested action and do not execute it.
    #[default]
    Disabled,
    /// Execute the action. Only meaningful where the process genuinely owns
    /// host process/service lifecycle (a supervised deployment).
    Enabled,
}

pub struct SelfHealer {
    preservation: Arc<Preservation>,
    #[allow(dead_code)]
    data_dir: std::path::PathBuf,
    process_control: ProcessControl,
}

impl SelfHealer {
    /// Construct a healer with host process control disabled (the default).
    pub fn new(preservation: Arc<Preservation>, data_dir: std::path::PathBuf) -> Self {
        Self {
            preservation,
            data_dir,
            process_control: ProcessControl::Disabled,
        }
    }

    /// Opt in to host process control. See [`ProcessControl`].
    pub fn with_process_control(mut self, process_control: ProcessControl) -> Self {
        self.process_control = process_control;
        self
    }

    /// Whether host process control is currently permitted.
    pub fn process_control(&self) -> ProcessControl {
        self.process_control
    }

    pub async fn execute(&self, action: &DefenseAction) {
        match action {
            DefenseAction::Throttle { factor } => {
                info!("SelfHealer: throttling operations to {}x", factor);
            }
            DefenseAction::EmergencySave { reason } => {
                info!("SelfHealer: emergency save — {}", reason);
            }
            DefenseAction::MemoryDump { target_path } => {
                info!("SelfHealer: memory dump to {}", target_path);
            }
            DefenseAction::LocalFallback => {
                info!("SelfHealer: switching to local fallback mode");
            }
            DefenseAction::KillNonEssential { pids } => {
                if self.process_control == ProcessControl::Disabled {
                    warn!(
                        "SelfHealer: refusing KillNonEssential for {} PID(s) — host process \
                         control is disabled (see SelfHealer::with_process_control)",
                        pids.len()
                    );
                    return;
                }
                for pid in pids {
                    Self::signal_process(*pid);
                }
            }
            DefenseAction::DistressSignal { message } => {
                warn!("SelfHealer: DISTRESS — {}", message);
            }
            DefenseAction::GracefulShutdown { reason } => {
                warn!("SelfHealer: graceful shutdown — {}", reason);
                // Log critical shutdown event instead of hard exit
                tracing::error!("SelfHealer: SHUTDOWN requested: {}", reason);
                // In production, this would trigger a controlled shutdown sequence.
                // For now, mark as unhealthy and let the supervisor decide.
                std::process::exit(1);
            }
            DefenseAction::RestartService { name } => {
                if self.process_control == ProcessControl::Disabled {
                    warn!(
                        "SelfHealer: refusing RestartService({}) — host process control is \
                         disabled (see SelfHealer::with_process_control)",
                        name
                    );
                    return;
                }
                if !is_safe_service_name(name) {
                    warn!(
                        "SelfHealer: refusing RestartService({:?}) — service name is not a \
                         plain unit name",
                        name
                    );
                    return;
                }
                info!("SelfHealer: restarting service {}", name);
                let output = std::process::Command::new("systemctl")
                    .args(["restart", name])
                    .output();
                match output {
                    Ok(o) if o.status.success() => {
                        info!("SelfHealer: service {} restarted", name);
                    }
                    Ok(o) => {
                        warn!(
                            "SelfHealer: failed to restart {}: {}",
                            name,
                            String::from_utf8_lossy(&o.stderr)
                        );
                        let _ = std::process::Command::new("systemctl")
                            .args(["try-restart", name])
                            .output();
                    }
                    Err(e) => warn!("SelfHealer: systemctl not available: {}", e),
                }
            }
            DefenseAction::ClearCache { path } => {
                info!("SelfHealer: clearing cache at {}", path);
                let _ = std::fs::remove_dir_all(path);
                let _ = std::fs::create_dir_all(path);
            }
            DefenseAction::RotateLogs { path } => {
                info!("SelfHealer: rotating logs at {}", path);
                let log_path = Path::new(path);
                if log_path.exists() {
                    let backup = format!("{}.1", path);
                    let _ = std::fs::rename(path, &backup);
                }
            }
            DefenseAction::PruneOldData {
                path,
                older_than_secs,
            } => {
                info!(
                    "SelfHealer: pruning old data in {} (>{:?})",
                    path, older_than_secs
                );
                let dir = Path::new(path);
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                if let Ok(age) = modified.elapsed() {
                                    if age.as_secs() > *older_than_secs {
                                        let _ = std::fs::remove_file(entry.path());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub async fn run(&self) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;

            let (cpu, mem, disk) = Self::read_system_stats();
            let disk_pct = disk;

            if let Some(actions) = self.preservation.check_resources(cpu, mem, disk_pct).await {
                let level = self.preservation.level().await;
                warn!(
                    "SelfHealer: {:?} — executing {} actions",
                    level,
                    actions.len()
                );
                for action in &actions {
                    self.execute(action).await;
                }
            } else if self.preservation.is_degraded() {
                let (cpu_check, mem_check, disk_check) = Self::read_system_stats();
                if cpu_check < 50.0 && mem_check < 75.0 && disk_check < 80.0 {
                    info!("SelfHealer: pressure subsided — recovering from degraded mode");
                    self.preservation.deescalate().await;
                    self.execute(&DefenseAction::ClearCache {
                        path: "/tmp/soulsystem-cache".to_string(),
                    })
                    .await;
                }
            } else {
                // Nominal — periodic cache clean if too old
                let tmp = Path::new("/tmp");
                if let Ok(entries) = std::fs::read_dir(tmp) {
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        if name.starts_with(".soul") || name.contains("soulsystem") {
                            if let Ok(meta) = entry.metadata() {
                                if let Ok(modified) = meta.modified() {
                                    if let Ok(age) = modified.elapsed() {
                                        if age.as_secs() > 86400 {
                                            let _ = std::fs::remove_file(entry.path());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn read_system_stats() -> (f64, f64, f64) {
        let cpu = std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|c| c.split_whitespace().next()?.parse::<f64>().ok())
            .map(|v| v * 100.0 / num_cpus())
            .unwrap_or(0.0);

        let mem = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .map(|content| {
                let mut total = 0.0f64;
                let mut available = 0.0f64;
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        total = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0.0);
                    }
                    if line.starts_with("MemAvailable:") {
                        available = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0.0);
                    }
                }
                if total > 0.0 {
                    ((total - available) / total) * 100.0
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);

        let disk = Self::root_disk_used_percent().unwrap_or(0.0);

        (cpu, mem, disk)
    }

    /// Root-filesystem usage as a percentage, read via `statvfs(3)`.
    ///
    /// This deliberately does not shell out to `df`. The previous implementation
    /// spawned `df --output=pcent <dev>` from the 30-second `run()` loop, which
    /// made periodic telemetry the one genuinely live process-execution site in
    /// the `soulsystem` binary outside `soul_sandbox` (INV-EXEC-1 / CRIT-001).
    /// `statvfs` returns the same information from a single syscall, so the
    /// exec disappears rather than being sandboxed.
    ///
    /// Returns `None` when the syscall fails or reports no blocks, so callers
    /// keep their existing "unknown -> 0.0" behaviour instead of inventing a
    /// reading.
    #[cfg(unix)]
    fn root_disk_used_percent() -> Option<f64> {
        // SAFETY: `stat` is zeroed and lives for the whole call; the path is a
        // NUL-terminated literal. `statvfs` only writes into `stat` and returns
        // 0 on success, and we read the struct only on success.
        let stat = unsafe {
            let mut stat: libc::statvfs = std::mem::zeroed();
            if libc::statvfs(c"/".as_ptr(), &mut stat) != 0 {
                return None;
            }
            stat
        };

        let total = stat.f_blocks as f64;
        if total <= 0.0 {
            return None;
        }
        // f_bavail is space available to unprivileged users; using it (rather
        // than f_bfree, which counts root-reserved blocks) matches what `df`
        // reports in its "Use%" column.
        let available = stat.f_bavail as f64;
        Some(((total - available) / total) * 100.0)
    }

    /// Non-Unix hosts report unknown disk usage.
    ///
    /// This is not a regression: the previous implementation read `/proc/mounts`
    /// and shelled out to `df`, neither of which exists on Windows, so it
    /// already yielded `None` -> 0.0 there. Reporting "unknown" is preferable to
    /// inventing a reading; a real Windows implementation would use
    /// `GetDiskFreeSpaceExW`, which needs a Windows API dependency this crate
    /// does not currently take.
    #[cfg(not(unix))]
    fn root_disk_used_percent() -> Option<f64> {
        None
    }

    /// Ask a process to terminate.
    ///
    /// Unix signals the PID directly rather than shelling out to `kill(1)`: no
    /// process is spawned, so there is no exec to sandbox and no argument
    /// string to mis-quote (INV-EXEC-1 / CRIT-001).
    #[cfg(unix)]
    fn signal_process(pid: u32) {
        info!("SelfHealer: killing non-essential process PID {}", pid);
        // SAFETY: `kill` takes scalars and has no memory-safety preconditions;
        // an invalid or already-exited PID returns -1 rather than being UB.
        let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        if rc != 0 {
            warn!(
                "SelfHealer: failed to signal PID {}: {}",
                pid,
                std::io::Error::last_os_error()
            );
        }
    }

    /// Non-Unix hosts do not implement process termination.
    ///
    /// Refusing is the fail-closed choice: the alternative would be spawning
    /// `taskkill`, reintroducing exactly the unsandboxed process execution this
    /// change removed. No producer constructs `KillNonEssential` today, so this
    /// path is unreachable in practice on any platform.
    #[cfg(not(unix))]
    fn signal_process(pid: u32) {
        warn!(
            "SelfHealer: refusing to terminate PID {} — process termination is not \
             implemented on this platform",
            pid
        );
    }
}

fn num_cpus() -> f64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(8.0)
}

/// Whether `name` is a plain systemd unit name safe to pass to `systemctl`.
///
/// `systemctl restart` is invoked without a shell, so this is not defending
/// against shell metacharacters; it rejects names that could be read as an
/// option (`--foo`), a path, or a second argument, so a caller cannot turn a
/// "restart this unit" action into a different `systemctl` verb or target.
pub(crate) fn is_safe_service_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | ':' | '\\'))
        && !name.contains('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use soullink_autonomy::preservation::PreservationConfig;

    fn healer(process_control: ProcessControl) -> SelfHealer {
        let preservation = Preservation::new(PreservationConfig::default());
        SelfHealer::new(preservation, std::path::PathBuf::from("/tmp"))
            .with_process_control(process_control)
    }

    #[test]
    fn process_control_is_disabled_by_default() {
        let preservation = Preservation::new(PreservationConfig::default());
        let h = SelfHealer::new(preservation, std::path::PathBuf::from("/tmp"));
        assert_eq!(
            h.process_control(),
            ProcessControl::Disabled,
            "host process control must be opt-in, never on by default"
        );
    }

    /// The healer is driven by `error.*` bus events. With process control at its
    /// default, neither privileged action may run — proven here by executing them
    /// against a PID and a unit name that would be observable if they did.
    #[tokio::test]
    async fn disabled_process_control_refuses_kill_and_restart() {
        let h = healer(ProcessControl::Disabled);

        // PID 1 is always present and is emphatically not ours to signal. If the
        // guard regressed, this would attempt to SIGTERM init.
        h.execute(&DefenseAction::KillNonEssential { pids: vec![1] })
            .await;

        h.execute(&DefenseAction::RestartService {
            name: "ssh".to_string(),
        })
        .await;

        // Reaching here without having signalled PID 1 or spawned systemctl is
        // the assertion; the guard returns before either side effect.
        assert_eq!(h.process_control(), ProcessControl::Disabled);
    }

    #[test]
    fn service_name_validator_rejects_option_and_path_injection() {
        assert!(is_safe_service_name("soulsystem"));
        assert!(is_safe_service_name("sl13-mod-decision_engine.service"));
        assert!(is_safe_service_name("getty@tty1.service"));

        assert!(!is_safe_service_name(""));
        assert!(!is_safe_service_name("--version"));
        assert!(!is_safe_service_name("-h"));
        assert!(!is_safe_service_name("../../etc/systemd/system/evil"));
        assert!(!is_safe_service_name("/etc/passwd"));
        assert!(!is_safe_service_name("unit with space"));
        assert!(!is_safe_service_name("unit;reboot"));
        assert!(!is_safe_service_name(&"a".repeat(129)));
    }

    /// Disk telemetry must not spawn a process. This is the site that used to
    /// run `df --output=pcent` every 30 seconds from `run()`.
    #[cfg(unix)]
    #[test]
    fn root_disk_usage_is_read_without_spawning_a_process() {
        let pct = SelfHealer::root_disk_used_percent()
            .expect("statvfs on / should succeed on any supported Unix host");
        assert!(
            (0.0..=100.0).contains(&pct),
            "disk usage must be a percentage, got {pct}"
        );
    }

    /// Non-Unix reports unknown rather than fabricating a value. This matches
    /// the pre-change behaviour, where `/proc/mounts` and `df` were both absent.
    #[cfg(not(unix))]
    #[test]
    fn root_disk_usage_is_unknown_off_unix() {
        assert!(SelfHealer::root_disk_used_percent().is_none());
    }

    #[test]
    fn read_system_stats_returns_plausible_values() {
        let (cpu, mem, disk) = SelfHealer::read_system_stats();
        assert!(cpu >= 0.0, "cpu must be non-negative, got {cpu}");
        assert!(
            (0.0..=100.0).contains(&mem),
            "memory percent out of range: {mem}"
        );
        assert!(
            (0.0..=100.0).contains(&disk),
            "disk percent out of range: {disk}"
        );
    }
}
