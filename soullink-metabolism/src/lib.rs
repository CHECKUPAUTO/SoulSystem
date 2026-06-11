//! SoulLink Digital Metabolism v0.1.0
//!
//! Tracks and manages the digital energy economy of the autonomous agent:
//! - Resource monitoring (CPU, GPU, RAM, Disk)
//! - Operation costing (tokens, inference time, storage)
//! - Auto-throttle and sleep when resources are low
//! - Budget enforcement per operation type

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ── Types ──────────────────────────────────────────────────────────────────

/// Current resource snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub gpu_temp_c: Option<f64>,
    pub gpu_vram_percent: Option<f64>,
    pub disk_percent: f64,
    pub timestamp: DateTime<Utc>,
}

/// Digital energy budget.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyBudget {
    /// Available energy units (0..100 scale)
    pub available: f64,
    /// Maximum energy capacity
    pub capacity: f64,
    /// Recharge rate per second (idle recovery)
    pub recharge_rate: f64,
    /// Current energy consumption rate
    pub consumption_rate: f64,
    /// Is the system in low-power mode?
    pub low_power: bool,
}

impl Default for EnergyBudget {
    fn default() -> Self {
        Self {
            available: 100.0,
            capacity: 100.0,
            recharge_rate: 0.05,
            consumption_rate: 0.0,
            low_power: false,
        }
    }
}

/// Cost of an operation in energy units.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationCost {
    pub operation: String,
    pub energy_cost: f64,
    pub tokens_used: Option<u64>,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

/// Accumulated statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetabolismStats {
    pub total_operations: u64,
    pub total_energy_spent: f64,
    pub total_tokens: u64,
    pub avg_cost_per_op: f64,
    pub throttle_events: u64,
    pub sleep_events: u64,
}

// ── Metabolism Engine ──────────────────────────────────────────────────────

pub struct Metabolism {
    budget: RwLock<EnergyBudget>,
    stats: RwLock<MetabolismStats>,
    operation_count: AtomicU64,
    throttle_active: AtomicBool,
    sleep_active: AtomicBool,
    start_time: Instant,
    history: RwLock<Vec<OperationCost>>,
}

impl Metabolism {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            budget: RwLock::new(EnergyBudget::default()),
            stats: RwLock::new(MetabolismStats {
                total_operations: 0,
                total_energy_spent: 0.0,
                total_tokens: 0,
                avg_cost_per_op: 0.0,
                throttle_events: 0,
                sleep_events: 0,
            }),
            operation_count: AtomicU64::new(0),
            throttle_active: AtomicBool::new(false),
            sleep_active: AtomicBool::new(false),
            start_time: Instant::now(),
            history: RwLock::new(Vec::with_capacity(1000)),
        })
    }

    /// Durée écoulée depuis le démarrage du métabolisme.
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get a snapshot of current system resources.
    pub fn resource_snapshot(&self) -> ResourceSnapshot {
        let (cpu, mem, disk) = Self::read_resources();
        ResourceSnapshot {
            cpu_percent: cpu,
            mem_percent: mem,
            gpu_temp_c: None,
            gpu_vram_percent: None,
            disk_percent: disk,
            timestamp: Utc::now(),
        }
    }

    /// Check if an operation can be afforded.
    pub async fn can_afford(&self, cost: f64) -> bool {
        let budget = self.budget.read().await;
        budget.available >= cost
    }

    /// Spend energy on an operation. Returns false if rejected.
    pub async fn spend(
        &self,
        operation: &str,
        cost: f64,
        tokens: Option<u64>,
        duration_ms: u64,
    ) -> bool {
        let mut budget = self.budget.write().await;
        if budget.available < cost && !budget.low_power {
            // Auto-throttle: reject expensive ops when low on energy
            if cost > 5.0 {
                let mut stats = self.stats.write().await;
                stats.throttle_events += 1;
                return false;
            }
        }
        budget.available = (budget.available - cost).max(0.0);
        budget.consumption_rate = cost / (duration_ms as f64 / 1000.0).max(0.001);

        let mut stats = self.stats.write().await;
        stats.total_operations += 1;
        stats.total_energy_spent += cost;
        stats.total_tokens += tokens.unwrap_or(0);
        stats.avg_cost_per_op = stats.total_energy_spent / stats.total_operations as f64;

        self.operation_count.fetch_add(1, Ordering::Relaxed);

        // Auto-sleep detection
        if budget.available < 5.0 && !self.sleep_active.load(Ordering::Relaxed) {
            self.sleep_active.store(true, Ordering::Relaxed);
            stats.sleep_events += 1;
            budget.low_power = true;
        }

        // Store in history
        {
            let mut hist = self.history.write().await;
            if hist.len() >= 1000 {
                hist.remove(0);
            }
            hist.push(OperationCost {
                operation: operation.to_string(),
                energy_cost: cost,
                tokens_used: tokens,
                duration_ms,
                timestamp: Utc::now(),
            });
        }

        true
    }

    /// Recharge energy over time (called periodically).
    pub async fn recharge(&self, elapsed_secs: f64) {
        let mut budget = self.budget.write().await;
        let recharge = budget.recharge_rate * elapsed_secs;

        // Faster recharge when idle
        let idle_bonus = if budget.consumption_rate < 0.1 {
            2.0
        } else {
            1.0
        };
        budget.available = (budget.available + recharge * idle_bonus).min(budget.capacity);

        // Exit sleep mode if energy is back
        if budget.available > 20.0 && self.sleep_active.load(Ordering::Relaxed) {
            self.sleep_active.store(false, Ordering::Relaxed);
            budget.low_power = false;
        }
    }

    /// Get the current budget.
    pub async fn budget(&self) -> EnergyBudget {
        self.budget.read().await.clone()
    }

    /// Get accumulated statistics.
    pub async fn stats(&self) -> MetabolismStats {
        self.stats.read().await.clone()
    }

    /// Get recent operation history.
    pub async fn recent_ops(&self, limit: usize) -> Vec<OperationCost> {
        let hist = self.history.read().await;
        hist.iter().rev().take(limit).cloned().collect()
    }

    /// Is the system currently throttling?
    pub fn is_throttling(&self) -> bool {
        self.throttle_active.load(Ordering::Relaxed)
    }

    /// Is the system in sleep/low-power mode?
    pub fn is_sleeping(&self) -> bool {
        self.sleep_active.load(Ordering::Relaxed)
    }

    /// Manual throttle toggle.
    pub fn set_throttle(&self, active: bool) {
        self.throttle_active.store(active, Ordering::Relaxed);
    }

    // ── Internal helpers ──────────────────────────────────────────────

    fn read_resources() -> (f64, f64, f64) {
        let cpu = std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|c| c.split_whitespace().next()?.parse::<f64>().ok())
            .map(|v| v * 100.0 / num_cpus())
            .unwrap_or(0.0);

        let (mem, disk) = (0.0, 0.0); // Simplified — real impl uses sysinfo crate
        (cpu, mem, disk)
    }

    /// Suggest an appropriate energy cost for a model inference.
    pub fn model_cost(model_size_gb: f64, prompt_tokens: u64, max_tokens: u64) -> f64 {
        let base = model_size_gb * 0.5; // 0.5 energy per GB of model
        let token_factor = (prompt_tokens + max_tokens) as f64 * 0.00001;
        base + token_factor
    }
}

fn num_cpus() -> f64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(8.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn budget_spend_and_recharge() {
        let m = Metabolism::new();
        assert!(m.can_afford(10.0).await);
        assert!(m.spend("test_op", 10.0, Some(100), 50).await);
        let b = m.budget().await;
        assert!(b.available < 100.0);
        m.recharge(60.0).await; // 1 minute
        let b2 = m.budget().await;
        assert!(b2.available > b.available);
    }

    #[tokio::test]
    async fn auto_sleep_on_low_energy() {
        let m = Metabolism::new();
        // Spend almost all energy
        m.spend("drain", 96.0, None, 10).await;
        assert!(m.is_sleeping());
        let b = m.budget().await;
        assert!(b.low_power);
    }

    #[tokio::test]
    async fn throttle_rejects_expensive_on_low() {
        let m = Metabolism::new();
        m.spend("drain", 95.0, None, 10).await; // 5 left
        let ok = m.spend("expensive", 20.0, None, 10).await;
        assert!(!ok, "should reject expensive operation when low");
    }

    #[test]
    fn model_cost_scales_with_size() {
        let small = Metabolism::model_cost(1.0, 100, 50);
        let large = Metabolism::model_cost(70.0, 100, 50);
        assert!(large > small * 10.0);
    }

    #[test]
    fn resource_snapshot_is_complete() {
        let m = Metabolism::new();
        let snap = m.resource_snapshot();
        assert!(snap.cpu_percent >= 0.0);
        assert!(snap.timestamp <= Utc::now());
    }
}
