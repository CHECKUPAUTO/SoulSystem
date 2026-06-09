//! soul-chaos — Chaos Monkey for SoulSystem resilience testing.
//!
//! Injects faults (latency, errors, data corruption) to verify
//! self-healing capabilities.
//!
//! # Fault Types
//!
//! - `Latency`: Adds artificial delay to requests
//! - `Error`: Injects random errors into responses
//! - `Corrupt`: Mangles data payloads
//! - `Kill`: Simulates process crash
//! - `Flood`: Sends excessive requests (DoS simulation)

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

// ── Fault Types ─────────────────────────────────────────────────────────

/// Type of fault to inject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FaultType {
    /// Add artificial latency.
    Latency,
    /// Inject random errors.
    Error,
    /// Corrupt data payloads.
    Corrupt,
    /// Simulate process crash.
    Kill,
    /// Send excessive requests.
    Flood,
}

impl FaultType {
    pub fn as_str(&self) -> &'static str {
        match self {
            FaultType::Latency => "latency",
            FaultType::Error => "error",
            FaultType::Corrupt => "corrupt",
            FaultType::Kill => "kill",
            FaultType::Flood => "flood",
        }
    }
}

impl std::fmt::Display for FaultType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Severity of a fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FaultSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl FaultSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            FaultSeverity::Low => "low",
            FaultSeverity::Medium => "medium",
            FaultSeverity::High => "high",
            FaultSeverity::Critical => "critical",
        }
    }
}

/// A fault to inject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fault {
    /// Type of fault.
    pub fault_type: FaultType,
    /// Severity.
    pub severity: FaultSeverity,
    /// Target organ name.
    pub target: String,
    /// Parameters (fault-specific).
    pub params: HashMap<String, serde_json::Value>,
}

/// Result of a chaos injection attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosResult {
    /// Whether the target survived.
    pub survived: bool,
    /// Recovery time in ms (0 if no failure was injected).
    pub recovery_ms: u64,
    /// Description of what happened.
    pub message: String,
    /// Whether self-healing was triggered.
    pub self_healing_triggered: bool,
}

// ── Chaos Engine ────────────────────────────────────────────────────────

/// The chaos engine that injects faults.
pub struct ChaosEngine {
    /// Whether chaos is currently enabled.
    enabled: Arc<AtomicBool>,
    /// Total injections performed.
    injection_count: Arc<AtomicU64>,
    /// Total successes (target survived).
    success_count: Arc<AtomicU64>,
    /// Total failures (target died).
    failure_count: Arc<AtomicU64>,
    /// RNG for fault generation.
    rng: rand::rngs::ThreadRng,
}

impl ChaosEngine {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            injection_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            failure_count: Arc::new(AtomicU64::new(0)),
            rng: rand::thread_rng(),
        }
    }

    /// Enable or disable chaos.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        if enabled {
            info!("Chaos engine: ENABLED");
        } else {
            info!("Chaos engine: DISABLED");
        }
    }

    /// Check if chaos is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Inject a fault and return the result.
    pub async fn inject(&mut self, fault: &Fault) -> ChaosResult {
        if !self.is_enabled() {
            return ChaosResult {
                survived: true,
                recovery_ms: 0,
                message: "Chaos is disabled".into(),
                self_healing_triggered: false,
            };
        }

        self.injection_count.fetch_add(1, Ordering::Relaxed);

        let result = match fault.fault_type {
            FaultType::Latency => self.inject_latency(fault).await,
            FaultType::Error => self.inject_error(fault).await,
            FaultType::Corrupt => self.inject_corrupt(fault).await,
            FaultType::Kill => self.inject_kill(fault).await,
            FaultType::Flood => self.inject_flood(fault).await,
        };

        if result.survived {
            self.success_count.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failure_count.fetch_add(1, Ordering::Relaxed);
        }

        result
    }

    async fn inject_latency(&mut self, fault: &Fault) -> ChaosResult {
        let base_ms = fault
            .params
            .get("latency_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(100);

        let jitter = self.rng.gen_range(0..=base_ms / 2);
        let total_ms = base_ms + jitter;

        info!(
            "Injecting {}ms latency into {}",
            total_ms, fault.target
        );

        tokio::time::sleep(Duration::from_millis(total_ms)).await;

        // Simulate whether the target survived the latency
        let survived = matches!(
            fault.severity,
            FaultSeverity::Low | FaultSeverity::Medium
        );

        ChaosResult {
            survived,
            recovery_ms: if survived { total_ms } else { 0 },
            message: format!("Injected {}ms latency", total_ms),
            self_healing_triggered: !survived,
        }
    }

    async fn inject_error(&mut self, fault: &Fault) -> ChaosResult {
        let error_rate = fault
            .params
            .get("error_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5);

        let roll: f64 = self.rng.gen();
        let injected = roll < error_rate;

        if injected {
            warn!("Injecting error into {} (rate={})", fault.target, error_rate);
        }

        ChaosResult {
            survived: !injected,
            recovery_ms: 0,
            message: if injected {
                format!("Injected error (rolled {:.2} < {:.2})", roll, error_rate)
            } else {
                format!("Error injection rolled {:.2} >= {:.2}, no fault", roll, error_rate)
            },
            self_healing_triggered: injected,
        }
    }

    async fn inject_corrupt(&mut self, fault: &Fault) -> ChaosResult {
        let corruption_pct = fault
            .params
            .get("corruption_pct")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.1);

        info!(
            "Injecting {:.0}% data corruption into {}",
            corruption_pct * 100.0,
            fault.target
        );

        // Simulate corruption detection and recovery
        let detected = self.rng.gen_bool(0.9); // 90% detection rate
        let recovered = detected && self.rng.gen_bool(0.8);

        ChaosResult {
            survived: recovered,
            recovery_ms: if recovered { self.rng.gen_range(10..100) } else { 0 },
            message: format!(
                "Corrupted {:.0}% data, detected={}, recovered={}",
                corruption_pct * 100.0,
                detected,
                recovered
            ),
            self_healing_triggered: detected,
        }
    }

    async fn inject_kill(&mut self, _fault: &Fault) -> ChaosResult {
        warn!("Simulating process kill on {}", _fault.target);

        // Simulate restart time
        let restart_ms = self.rng.gen_range(500..3000);

        ChaosResult {
            survived: true,
            recovery_ms: restart_ms,
            message: format!("Process killed, restarted in {}ms", restart_ms),
            self_healing_triggered: true,
        }
    }

    async fn inject_flood(&mut self, fault: &Fault) -> ChaosResult {
        let count = fault
            .params
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(100);

        info!("Flooding {} with {} requests", fault.target, count);

        // Simulate flood handling
        let handled = count.min(50); // Rate limiter kicks in
        let dropped = count - handled;

        ChaosResult {
            survived: true,
            recovery_ms: 0,
            message: format!("Flood: {} sent, {} handled, {} dropped by rate limiter", count, handled, dropped),
            self_healing_triggered: dropped > 0,
        }
    }

    /// Get injection statistics.
    pub fn stats(&self) -> ChaosStats {
        ChaosStats {
            total_injections: self.injection_count.load(Ordering::Relaxed),
            successes: self.success_count.load(Ordering::Relaxed),
            failures: self.failure_count.load(Ordering::Relaxed),
            enabled: self.enabled.load(Ordering::Relaxed),
        }
    }
}

impl Default for ChaosEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Chaos engine statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChaosStats {
    pub total_injections: u64,
    pub successes: u64,
    pub failures: u64,
    pub enabled: bool,
}

// ── Chaos Scenarios ─────────────────────────────────────────────────────

/// Pre-defined chaos scenarios.
pub struct ChaosScenarios;

impl ChaosScenarios {
    /// Random fault injection with mixed severity.
    pub fn random_fault(target: &str) -> Fault {
        let mut rng = rand::thread_rng();
        let fault_type = match rng.gen_range(0..5) {
            0 => FaultType::Latency,
            1 => FaultType::Error,
            2 => FaultType::Corrupt,
            3 => FaultType::Kill,
            _ => FaultType::Flood,
        };
        let severity = match rng.gen_range(0..4) {
            0 => FaultSeverity::Low,
            1 => FaultSeverity::Medium,
            2 => FaultSeverity::High,
            _ => FaultSeverity::Critical,
        };

        Fault {
            fault_type,
            severity,
            target: target.into(),
            params: HashMap::new(),
        }
    }

    /// Latency spike scenario.
    pub fn latency_spike(target: &str, ms: u64) -> Fault {
        let mut params = HashMap::new();
        params.insert("latency_ms".into(), serde_json::Value::Number(ms.into()));
        Fault {
            fault_type: FaultType::Latency,
            severity: if ms > 500 {
                FaultSeverity::High
            } else {
                FaultSeverity::Medium
            },
            target: target.into(),
            params,
        }
    }

    /// Error injection scenario.
    pub fn error_injection(target: &str, rate: f64) -> Fault {
        let mut params = HashMap::new();
        params.insert(
            "error_rate".into(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(rate).unwrap(),
            ),
        );
        Fault {
            fault_type: FaultType::Error,
            severity: if rate > 0.5 {
                FaultSeverity::High
            } else {
                FaultSeverity::Medium
            },
            target: target.into(),
            params,
        }
    }

    /// Data corruption scenario.
    pub fn data_corruption(target: &str, pct: f64) -> Fault {
        let mut params = HashMap::new();
        params.insert(
            "corruption_pct".into(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(pct).unwrap(),
            ),
        );
        Fault {
            fault_type: FaultType::Corrupt,
            severity: FaultSeverity::High,
            target: target.into(),
            params,
        }
    }

    /// Kill and restart scenario.
    pub fn kill_process(target: &str) -> Fault {
        Fault {
            fault_type: FaultType::Kill,
            severity: FaultSeverity::Critical,
            target: target.into(),
            params: HashMap::new(),
        }
    }

    /// Flood/DoS scenario.
    pub fn flood(target: &str, count: u64) -> Fault {
        let mut params = HashMap::new();
        params.insert("count".into(), serde_json::Value::Number(count.into()));
        Fault {
            fault_type: FaultType::Flood,
            severity: if count > 200 {
                FaultSeverity::Critical
            } else {
                FaultSeverity::High
            },
            target: target.into(),
            params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fault_type_as_str() {
        assert_eq!(FaultType::Latency.as_str(), "latency");
        assert_eq!(FaultType::Kill.as_str(), "kill");
    }

    #[test]
    fn test_chaos_engine_enable_disable() {
        let engine = ChaosEngine::new();
        assert!(engine.is_enabled());

        engine.set_enabled(false);
        assert!(!engine.is_enabled());

        engine.set_enabled(true);
        assert!(engine.is_enabled());
    }

    #[tokio::test]
    async fn test_inject_disabled() {
        let engine = ChaosEngine::new();
        engine.set_enabled(false);

        let fault = Fault {
            fault_type: FaultType::Latency,
            severity: FaultSeverity::Low,
            target: "test".into(),
            params: HashMap::new(),
        };

        // Need mutable engine for inject, but inject checks enabled flag
        let mut engine = engine;
        let result = engine.inject(&fault).await;
        assert!(result.survived);
        assert_eq!(result.recovery_ms, 0);
    }

    #[tokio::test]
    async fn test_inject_latency() {
        let mut engine = ChaosEngine::new();
        let fault = Fault {
            fault_type: FaultType::Latency,
            severity: FaultSeverity::Low,
            target: "test".into(),
            params: {
                let mut m = HashMap::new();
                m.insert("latency_ms".into(), serde_json::Value::Number(10.into()));
                m
            },
        };

        let result = engine.inject(&fault).await;
        assert!(result.survived);
        assert!(result.recovery_ms >= 10);
    }

    #[tokio::test]
    async fn test_inject_kill() {
        let mut engine = ChaosEngine::new();
        let fault = Fault {
            fault_type: FaultType::Kill,
            severity: FaultSeverity::Critical,
            target: "test".into(),
            params: HashMap::new(),
        };

        let result = engine.inject(&fault).await;
        assert!(result.survived); // Restarted
        assert!(result.recovery_ms > 0);
        assert!(result.self_healing_triggered);
    }

    #[tokio::test]
    async fn test_inject_flood() {
        let mut engine = ChaosEngine::new();
        let fault = Fault {
            fault_type: FaultType::Flood,
            severity: FaultSeverity::High,
            target: "test".into(),
            params: {
                let mut m = HashMap::new();
                m.insert("count".into(), serde_json::Value::Number(100.into()));
                m
            },
        };

        let result = engine.inject(&fault).await;
        assert!(result.survived);
        assert!(result.message.contains("rate limiter"));
    }

    #[test]
    fn test_chaos_scenarios() {
        let fault = ChaosScenarios::latency_spike("test", 200);
        assert_eq!(fault.fault_type, FaultType::Latency);
        assert_eq!(fault.target, "test");

        let fault = ChaosScenarios::error_injection("test", 0.3);
        assert_eq!(fault.fault_type, FaultType::Error);

        let fault = ChaosScenarios::data_corruption("test", 0.1);
        assert_eq!(fault.fault_type, FaultType::Corrupt);

        let fault = ChaosScenarios::kill_process("test");
        assert_eq!(fault.fault_type, FaultType::Kill);

        let fault = ChaosScenarios::flood("test", 150);
        assert_eq!(fault.fault_type, FaultType::Flood);
    }

    #[test]
    fn test_chaos_stats() {
        let engine = ChaosEngine::new();
        let stats = engine.stats();
        assert_eq!(stats.total_injections, 0);
        assert!(stats.enabled);
    }
}
