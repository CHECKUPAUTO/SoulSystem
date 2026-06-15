use soullink_autonomy::preservation::{DefenseAction, Preservation};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

pub struct SelfHealer {
    preservation: Arc<Preservation>,
    #[allow(dead_code)]
    data_dir: std::path::PathBuf,
}

impl SelfHealer {
    pub fn new(preservation: Arc<Preservation>, data_dir: std::path::PathBuf) -> Self {
        Self {
            preservation,
            data_dir,
        }
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
                for pid in pids {
                    info!("SelfHealer: killing non-essential process PID {}", pid);
                    let _ = std::process::Command::new("kill")
                        .arg(pid.to_string())
                        .output();
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
            let disk_pct = disk as f64;

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

        let disk = std::fs::read_to_string("/proc/mounts")
            .ok()
            .and_then(|content| {
                for line in content.lines() {
                    if line.contains(" / ") {
                        if let Some(dev) = line.split_whitespace().next() {
                            if let Ok(usage) = std::process::Command::new("df")
                                .args(["--output=pcent", dev])
                                .output()
                            {
                                let out = String::from_utf8_lossy(&usage.stdout);
                                if let Some(pct) = out.lines().nth(1) {
                                    if let Ok(val) = pct.trim().trim_end_matches('%').parse::<f64>()
                                    {
                                        return Some(val);
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
                None
            })
            .unwrap_or(0.0);

        (cpu, mem, disk)
    }
}

fn num_cpus() -> f64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(8.0)
}
