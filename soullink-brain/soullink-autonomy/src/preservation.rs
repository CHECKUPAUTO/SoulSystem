//! Preservation Instinct — Self-preservation for the autonomous digital entity.
//!
//! Detects danger patterns and triggers automatic defensive responses:
//! - High load → throttle
//! - Cascading errors → isolate + emergency save
//! - OOM risk → memory dump + graceful degrade
//! - Connection loss → local fallback mode
//!
//! Part of SoulLink Autonomy v14 — organism roadmap item #2.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

// ── Danger signals ─────────────────────────────────────────────────────────

/// Severity level for danger signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DangerLevel {
    /// Normal operation
    Normal = 0,
    /// Elevated — monitor closely
    Elevated = 1,
    /// High — take preventive action
    High = 2,
    /// Critical — emergency response
    Critical = 3,
    /// Fatal — last-resort save and exit
    Fatal = 4,
}

/// Type of danger detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DangerType {
    HighCpuLoad {
        percent: f64,
    },
    HighMemoryPressure {
        percent: f64,
    },
    CascadingErrors {
        count: u64,
        window_secs: u64,
    },
    OutOfMemory {
        requested_mb: u64,
        available_mb: u64,
    },
    ConnectionLoss {
        endpoint: String,
        duration_secs: u64,
    },
    DiskFull {
        percent: f64,
    },
    TemperatureSpike {
        current_c: f64,
        threshold_c: f64,
    },
}

/// A detected danger event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DangerEvent {
    pub level: DangerLevel,
    pub danger_type: DangerType,
    pub timestamp_ms: i64,
    pub description: String,
}

// ── Defense responses ──────────────────────────────────────────────────────

/// Automatic defense action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DefenseAction {
    /// Reduce non-critical operations
    Throttle { factor: f64 },
    /// Save critical state and enter degraded mode
    EmergencySave { reason: String },
    /// Dump memory to disk before possible OOM
    MemoryDump { target_path: String },
    /// Switch to offline/local-only mode
    LocalFallback,
    /// Kill non-essential child processes
    KillNonEssential { pids: Vec<u32> },
    /// Request external help
    DistressSignal { message: String },
    /// Graceful shutdown sequence
    GracefulShutdown { reason: String },
    /// Restart a systemd service or child process
    RestartService { name: String },
    /// Clear a cache directory
    ClearCache { path: String },
    /// Rotate logs (archive + truncate)
    RotateLogs { path: String },
    /// Prune old data (checkpoints, memory, etc.)
    PruneOldData { path: String, older_than_secs: u64 },
}

// ── Preservation Engine ────────────────────────────────────────────────────

/// Self-preservation engine — the digital "fight or flight" response.
pub struct Preservation {
    /// Current danger level
    danger_level: RwLock<DangerLevel>,
    /// Recent danger events (ring buffer)
    events: RwLock<Vec<DangerEvent>>,
    /// Count of cascading errors in current window
    error_count: AtomicU64,
    /// Timestamp of last error window reset
    error_window_start: RwLock<Instant>,
    /// Whether emergency save has been triggered
    emergency_saved: AtomicBool,
    /// Whether we're in degraded/offline mode
    degraded_mode: AtomicBool,
    /// Count of defensive actions taken
    action_count: AtomicU64,
    /// Configuration
    config: PreservationConfig,
}

#[derive(Debug, Clone)]
pub struct PreservationConfig {
    /// Max errors before declaring cascade (per window)
    pub error_threshold: u64,
    /// Error window duration
    pub error_window_secs: u64,
    /// CPU percent threshold for danger
    pub cpu_danger_pct: f64,
    /// Memory percent threshold for danger
    pub mem_danger_pct: f64,
    /// Disk percent threshold for danger
    pub disk_danger_pct: f64,
    /// Temperature threshold (Celsius)
    pub temp_danger_c: f64,
    /// Max time without connection before declaring offline
    pub connection_timeout_secs: u64,
}

impl Default for PreservationConfig {
    fn default() -> Self {
        Self {
            error_threshold: 10,
            error_window_secs: 60,
            cpu_danger_pct: 90.0,
            mem_danger_pct: 95.0,
            disk_danger_pct: 95.0,
            temp_danger_c: 85.0,
            connection_timeout_secs: 300,
        }
    }
}

impl Preservation {
    pub fn new(config: PreservationConfig) -> Arc<Self> {
        Arc::new(Self {
            danger_level: RwLock::new(DangerLevel::Normal),
            events: RwLock::new(Vec::with_capacity(100)),
            error_count: AtomicU64::new(0),
            error_window_start: RwLock::new(Instant::now()),
            emergency_saved: AtomicBool::new(false),
            degraded_mode: AtomicBool::new(false),
            action_count: AtomicU64::new(0),
            config,
        })
    }

    /// Record an error. If threshold exceeded, returns defensive actions.
    pub async fn record_error(&self) -> Option<Vec<DefenseAction>> {
        let count = self.error_count.fetch_add(1, Ordering::Relaxed) + 1;
        let window_start = *self.error_window_start.read().await;

        if window_start.elapsed() > Duration::from_secs(self.config.error_window_secs) {
            // Reset window
            self.error_count.store(1, Ordering::Relaxed);
            *self.error_window_start.write().await = Instant::now();
            return None;
        }

        if count >= self.config.error_threshold {
            warn!(
                "Preservation: cascading error detected — {} errors in window",
                count
            );
            self.add_event(
                DangerLevel::Critical,
                DangerType::CascadingErrors {
                    count,
                    window_secs: self.config.error_window_secs,
                },
            )
            .await;
            Some(self.escalate(DangerLevel::Critical).await)
        } else if count >= self.config.error_threshold / 2 {
            self.add_event(
                DangerLevel::Elevated,
                DangerType::CascadingErrors {
                    count,
                    window_secs: self.config.error_window_secs,
                },
            )
            .await;
            None // Just monitoring at elevated
        } else {
            None
        }
    }

    /// Check CPU/memory/disk and respond if thresholds crossed.
    pub async fn check_resources(
        &self,
        cpu_pct: f64,
        mem_pct: f64,
        disk_pct: f64,
    ) -> Option<Vec<DefenseAction>> {
        let mut actions = Vec::new();

        if cpu_pct > self.config.cpu_danger_pct {
            self.add_event(
                DangerLevel::High,
                DangerType::HighCpuLoad { percent: cpu_pct },
            )
            .await;
            actions.push(DefenseAction::Throttle { factor: 0.5 });
        }
        if mem_pct > self.config.mem_danger_pct {
            self.add_event(
                DangerLevel::Critical,
                DangerType::HighMemoryPressure { percent: mem_pct },
            )
            .await;
            actions.push(DefenseAction::MemoryDump {
                target_path: "/tmp/soullink-emergency-dump.json".into(),
            });
            actions.push(DefenseAction::Throttle { factor: 0.3 });
        }
        if disk_pct > self.config.disk_danger_pct {
            self.add_event(
                DangerLevel::High,
                DangerType::DiskFull { percent: disk_pct },
            )
            .await;
            actions.push(DefenseAction::EmergencySave {
                reason: format!("Disk at {:.1}%", disk_pct),
            });
        }

        if !actions.is_empty() {
            let level = if mem_pct > self.config.mem_danger_pct {
                DangerLevel::Critical
            } else {
                DangerLevel::High
            };
            self.escalate_inner(level).await;
        }

        if actions.is_empty() {
            None
        } else {
            Some(actions)
        }
    }

    /// Report connection loss and trigger local fallback if needed.
    pub async fn report_disconnect(
        &self,
        endpoint: &str,
        duration_secs: u64,
    ) -> Option<Vec<DefenseAction>> {
        if duration_secs > self.config.connection_timeout_secs {
            self.add_event(
                DangerLevel::Critical,
                DangerType::ConnectionLoss {
                    endpoint: endpoint.to_string(),
                    duration_secs,
                },
            )
            .await;
            Some(vec![DefenseAction::LocalFallback])
        } else {
            None
        }
    }

    /// Recover from degraded mode — reset state when pressure subsides.
    pub fn recover(&self) {
        self.degraded_mode.store(false, Ordering::Relaxed);
        self.emergency_saved.store(false, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
    }

    /// De-escalate danger level if conditions improve.
    pub async fn deescalate(&self) {
        let mut current = self.danger_level.write().await;
        if *current > DangerLevel::Elevated {
            *current = DangerLevel::Elevated;
        } else if *current == DangerLevel::Elevated {
            *current = DangerLevel::Normal;
            self.recover();
        }
        info!("Preservation: de-escalated to {:?}", *current);
    }

    /// Get current danger level.
    pub async fn level(&self) -> DangerLevel {
        *self.danger_level.read().await
    }

    /// Whether in degraded/offline mode.
    pub fn is_degraded(&self) -> bool {
        self.degraded_mode.load(Ordering::Relaxed)
    }

    /// Emergency save has been triggered.
    pub fn has_emergency_saved(&self) -> bool {
        self.emergency_saved.load(Ordering::Relaxed)
    }

    /// Get recent danger events.
    pub async fn recent_events(&self, limit: usize) -> Vec<DangerEvent> {
        let events = self.events.read().await;
        events.iter().rev().take(limit).cloned().collect()
    }

    /// Total defensive actions taken.
    pub fn action_count(&self) -> u64 {
        self.action_count.load(Ordering::Relaxed)
    }

    /// Manually trigger self-destruct / graceful shutdown.
    pub async fn self_destruct(&self, reason: &str) -> Vec<DefenseAction> {
        self.add_event(
            DangerLevel::Fatal,
            DangerType::CascadingErrors {
                count: 999,
                window_secs: 0,
            },
        )
        .await;
        vec![
            DefenseAction::EmergencySave {
                reason: reason.to_string(),
            },
            DefenseAction::DistressSignal {
                message: format!("FATAL: {}", reason),
            },
            DefenseAction::GracefulShutdown {
                reason: reason.to_string(),
            },
        ]
    }

    // ── Internal ──────────────────────────────────────────────────────

    async fn add_event(&self, level: DangerLevel, danger_type: DangerType) {
        let event = DangerEvent {
            level,
            danger_type,
            description: String::new(), // filled below
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        };
        let event = DangerEvent {
            description: format!("{:?}", event.danger_type),
            ..event
        };
        let mut events = self.events.write().await;
        if events.len() >= 100 {
            events.remove(0);
        }
        let msg = format!("Preservation: {:?} — {:?}", level, event.danger_type);
        events.push(event);
        info!("{}", msg);
    }

    async fn escalate(&self, level: DangerLevel) -> Vec<DefenseAction> {
        self.escalate_inner(level).await;
        match level {
            DangerLevel::Critical => {
                self.emergency_saved.store(true, Ordering::Relaxed);
                self.action_count.fetch_add(1, Ordering::Relaxed);
                vec![
                    DefenseAction::EmergencySave {
                        reason: "Critical danger level".into(),
                    },
                    DefenseAction::Throttle { factor: 0.1 },
                ]
            }
            DangerLevel::Fatal => {
                self.action_count.fetch_add(1, Ordering::Relaxed);
                vec![
                    DefenseAction::EmergencySave {
                        reason: "Fatal danger — saving state".into(),
                    },
                    DefenseAction::DistressSignal {
                        message: "SoulLink FATAL — requesting intervention".into(),
                    },
                    DefenseAction::GracefulShutdown {
                        reason: "Fatal danger level reached".into(),
                    },
                ]
            }
            _ => {
                self.action_count.fetch_add(1, Ordering::Relaxed);
                vec![DefenseAction::Throttle { factor: 0.5 }]
            }
        }
    }

    async fn escalate_inner(&self, level: DangerLevel) {
        let mut current = self.danger_level.write().await;
        if level > *current {
            *current = level;
            if matches!(level, DangerLevel::Critical | DangerLevel::Fatal) {
                self.degraded_mode.store(true, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn error_cascade_detection() {
        let p = Preservation::new(PreservationConfig {
            error_threshold: 5,
            ..Default::default()
        });
        for _ in 0..4 {
            assert!(p.record_error().await.is_none());
        }
        let actions = p.record_error().await;
        assert!(actions.is_some());
        assert_eq!(p.level().await, DangerLevel::Critical);
    }

    #[tokio::test]
    async fn error_window_resets() {
        let p = Preservation::new(PreservationConfig {
            error_threshold: 5,
            error_window_secs: 0, // immediate reset
            ..Default::default()
        });
        p.record_error().await;
        // Window should have reset
        assert!(p.record_error().await.is_none());
    }

    #[tokio::test]
    async fn high_memory_triggers_critical() {
        let p = Preservation::new(PreservationConfig::default());
        let actions = p.check_resources(50.0, 98.0, 50.0).await;
        assert!(actions.is_some());
        let actions = actions.unwrap();
        assert!(actions
            .iter()
            .any(|a| matches!(a, DefenseAction::MemoryDump { .. })));
    }

    #[tokio::test]
    async fn degraded_mode_persists() {
        let p = Preservation::new(PreservationConfig::default());
        assert!(!p.is_degraded());
        p.escalate(DangerLevel::Critical).await;
        assert!(p.is_degraded());
    }

    #[test]
    fn config_defaults_are_sane() {
        let c = PreservationConfig::default();
        assert!(c.error_threshold > 0);
        assert!(c.cpu_danger_pct > 0.0);
        assert!(c.mem_danger_pct > 0.0);
    }
}
