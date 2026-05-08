//! DecisionEngine — Reactor consuming MPSC events from all actors.
//!
//! ## Phase 1 security change
//!
//! The old [`apply_throttle`] method invoked `sh -c <str>` with the command
//! string read from TOML. It has been rewritten to call NVML directly via
//! [`NvmlBridge::set_power_limit_w`] / [`NvmlBridge::reset_power_limit`].
//! **No shell, no subprocess, no external CLI.**
//!
//! ## Phase 2b testability
//!
//! Throttle policy (hysteresis + cooldown) lives in [`ThrottlePolicy`], a
//! pure struct that returns [`ThrottleDecision`] for any (temp_c, stress,
//! now) triple. The async `DecisionEngine` merely applies those decisions
//! to NVML. Unit tests drive `ThrottlePolicy` with virtual time.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use soullink_core::{GuardianEvent, SystemSnapshot};
use soullink_nvml::NvmlBridge;

use crate::{
    config::GuardianConfig,
    pacing::{PacingAction, PacingConfig, PiController},
    ws_client::OrchestratorClient,
};

pub const THROTTLE_COOLDOWN: Duration = Duration::from_secs(30);
pub const STRESS_HYSTERESIS_DEADBAND: f64 = 0.1;

#[derive(Debug, PartialEq, Eq)]
pub enum ThrottleDecision {
    /// No change — either already in desired state, or cooldown forbids it.
    NoChange,
    /// Engage throttle (lower power limit).
    Engage,
    /// Release throttle (restore power limit).
    Release,
}

/// Reason a decision was produced — helps tests pinpoint which branch fired.
#[derive(Debug, PartialEq, Eq)]
pub enum Trigger {
    Temperature,
    Stress,
    None,
}

/// Pure hysteresis + cooldown logic. No NVML, no async.
pub struct ThrottlePolicy {
    pub throttle_active:      bool,
    pub last_change_at:       Option<Instant>,
    pub throttle_released_at: Option<Instant>,
    pub throttle_on_temp:     f32,
    pub throttle_off_temp:    f32,
    pub stress_threshold:     f64,
}

impl ThrottlePolicy {
    pub fn new(on_temp: f32, off_temp: f32, stress_threshold: f64) -> Self {
        assert!(off_temp < on_temp,
                "off_temp ({off_temp}) must be below on_temp ({on_temp}) for hysteresis");
        Self {
            throttle_active: false,
            last_change_at: None,
            throttle_released_at: None,
            throttle_on_temp: on_temp,
            throttle_off_temp: off_temp,
            stress_threshold,
        }
    }

    /// Evaluate a thermal reading. Returns the decision to apply; caller
    /// must call [`mark_applied`] on success.
    pub fn evaluate_thermal(&self, temp_c: f32, now: Instant) -> (ThrottleDecision, Trigger) {
        if temp_c >= self.throttle_on_temp && !self.throttle_active {
            return (ThrottleDecision::Engage, Trigger::Temperature);
        }
        if temp_c <= self.throttle_off_temp && self.throttle_active {
            if !self.cooldown_expired(now) {
                return (ThrottleDecision::NoChange, Trigger::Temperature);
            }
            return (ThrottleDecision::Release, Trigger::Temperature);
        }
        (ThrottleDecision::NoChange, Trigger::None)
    }

    /// Evaluate a stress reading. Hysteresis uses a 0.1 deadband.
    pub fn evaluate_stress(&self, stress: f64, now: Instant) -> (ThrottleDecision, Trigger) {
        if stress > self.stress_threshold && !self.throttle_active {
            return (ThrottleDecision::Engage, Trigger::Stress);
        }
        if stress < self.stress_threshold - STRESS_HYSTERESIS_DEADBAND
            && self.throttle_active
            && self.throttle_released_at.is_none()
        {
            if !self.cooldown_expired(now) {
                return (ThrottleDecision::NoChange, Trigger::Stress);
            }
            return (ThrottleDecision::Release, Trigger::Stress);
        }
        (ThrottleDecision::NoChange, Trigger::None)
    }

    fn cooldown_expired(&self, now: Instant) -> bool {
        match self.last_change_at {
            None => true,
            Some(last) => now.duration_since(last) >= THROTTLE_COOLDOWN,
        }
    }

    /// Record that a decision was applied successfully at `now`.
    pub fn mark_applied(&mut self, decision: ThrottleDecision, now: Instant) {
        match decision {
            ThrottleDecision::Engage => {
                self.throttle_active = true;
                self.last_change_at = Some(now);
                self.throttle_released_at = None;
            }
            ThrottleDecision::Release => {
                self.throttle_active = false;
                self.last_change_at = Some(now);
                self.throttle_released_at = Some(now);
            }
            ThrottleDecision::NoChange => {}
        }
    }

    /// Called periodically to advance recovery state. Returns `true` when
    /// the 60-second observation window has elapsed.
    pub fn recovery_elapsed(&mut self, now: Instant) -> bool {
        if let Some(released_at) = self.throttle_released_at {
            if !self.throttle_active && now.duration_since(released_at) > Duration::from_secs(60) {
                self.throttle_released_at = None;
                return true;
            }
        }
        false
    }
}

pub struct DecisionEngine {
    rx:     mpsc::Receiver<GuardianEvent>,
    ws:     OrchestratorClient,
    state:  Arc<ArcSwap<SystemSnapshot>>,
    config: GuardianConfig,
    nvml:   Arc<NvmlBridge>,

    policy:           ThrottlePolicy,
    pi:               PiController,
    xid_accumulator:  u64,
    decision_count:   u64,
    /// Phase 6b-audit-triage v2: when `set_power_limit_w` fails (e.g.
    /// permission denied on some driver/systemd combinations), we mark
    /// the timestamp here and skip retries for `POWER_LIMIT_RETRY_BACKOFF`
    /// to avoid log spam (21 k+ WARN/day observed pre-fix).
    power_limit_failed_at: Option<Instant>,
}

/// Phase 6b-audit-triage v2: back-off window after a power-limit failure.
/// 5 min mirrors the reset-rate-limit constant in `emergency.rs`.
const POWER_LIMIT_RETRY_BACKOFF: Duration = Duration::from_secs(300);

impl DecisionEngine {
    pub fn new(
        rx:     mpsc::Receiver<GuardianEvent>,
        ws:     OrchestratorClient,
        state:  Arc<ArcSwap<SystemSnapshot>>,
        config: GuardianConfig,
        nvml:   Arc<NvmlBridge>,
    ) -> Self {
        let policy = ThrottlePolicy::new(85.0, 78.0, config.emergency_stress_threshold);
        let (power_min, power_max) = nvml.power_range_w();
        let pi_cfg = PacingConfig {
            enabled:         config.pacing_enabled,
            target_c:        config.pacing_target_c,
            hard_cap_temp_c: config.pacing_hard_cap_c,
            power_min_w:     power_min,
            power_max_w:     power_max,
            ..PacingConfig::default()
        };
        let pi = PiController::new(pi_cfg);
        Self {
            rx, ws, state, config, nvml,
            policy,
            pi,
            xid_accumulator: 0,
            decision_count: 0,
            power_limit_failed_at: None,
        }
    }

    pub async fn run(&mut self) {
        info!("🧠 DecisionEngine started — Reactor pattern, NVML-only throttling");

        while let Some(event) = self.rx.recv().await {
            self.decision_count += 1;

            match event {
                GuardianEvent::ThermalReading { temp_c, power_w: _, throttle: _, correlation: _ } => {
                    if self.config.pacing_enabled {
                        // Phase 5: PI controller path
                        match self.pi.step(temp_c, Instant::now()) {
                            PacingAction::HardCap(watts) => {
                                warn!(temp_c, watts, "PI hard-cap engaged");
                                let _ = self.apply_exact_power(watts).await;
                            }
                            PacingAction::Apply(watts) => {
                                info!(temp_c, watts, integral = self.pi.integral(),
                                      "PI pacing adjust");
                                let _ = self.apply_exact_power(watts).await;
                            }
                            PacingAction::NoChange => {}
                        }
                    } else {
                        // Phase 1 hysteresis path (unchanged)
                        let (decision, trigger) = self.policy.evaluate_thermal(temp_c, Instant::now());
                        match decision {
                            ThrottleDecision::Engage => {
                                warn!(temp_c, ?trigger, threshold = self.policy.throttle_on_temp,
                                      "THERMAL THROTTLE engaged");
                                if self.apply_throttle(true).await {
                                    self.policy.mark_applied(decision, Instant::now());
                                }
                            }
                            ThrottleDecision::Release => {
                                info!(temp_c, ?trigger, threshold = self.policy.throttle_off_temp,
                                      "throttle released");
                                if self.apply_throttle(false).await {
                                    self.policy.mark_applied(decision, Instant::now());
                                }
                            }
                            ThrottleDecision::NoChange => {}
                        }
                    }
                }

                GuardianEvent::MetricsBatch { snapshot } => {
                    self.ws.push_snapshot(&snapshot).await;
                }

                GuardianEvent::XidError { code, count } => {
                    self.xid_accumulator = count;
                    if self.xid_accumulator >= 3 {
                        error!(code, count, "XID CRITICAL — 3+ errors accumulated");
                    } else {
                        warn!(code, count, "XID error detected");
                    }
                }

                GuardianEvent::GpuHang { duration_ms } => {
                    error!(duration_ms, "GPU HANG signaled to DecisionEngine");
                }

                GuardianEvent::ResetCompleted { success, elapsed_ms } => {
                    if success {
                        info!(elapsed_ms, "PCIe reset completed — state reset");
                        self.xid_accumulator = 0;
                        self.policy.throttle_active = false;
                    } else {
                        error!(elapsed_ms, "PCIe reset FAILED — manual escalation required");
                    }
                }

                GuardianEvent::StressUpdate { index, action } => {
                    let now = Instant::now();
                    let (decision, trigger) = self.policy.evaluate_stress(index, now);
                    match decision {
                        ThrottleDecision::Engage => {
                            warn!(index, %action, ?trigger, "stress throttle engaged");
                            if self.apply_throttle(true).await {
                                self.policy.mark_applied(decision, now);
                            }
                        }
                        ThrottleDecision::Release => {
                            info!(index, %action, ?trigger, "stress throttle released");
                            if self.apply_throttle(false).await {
                                self.policy.mark_applied(decision, now);
                            }
                        }
                        ThrottleDecision::NoChange => {}
                    }

                    if self.policy.recovery_elapsed(now) {
                        info!("Load recovery: 60s stable — full capacity restored");
                    }
                    // Phase 1: Zero-Disk — state exposed via HTTP /state
                }
            }
        }

        error!("DecisionEngine: MPSC channel closed — unexpected shutdown");
    }

    /// Apply (or release) GPU power throttle via NVML FFI. Returns `true`
    /// on success so the caller knows whether to update policy state.
    async fn apply_throttle(&mut self, throttle: bool) -> bool {
        let watts = if throttle {
            self.config.throttle_power_limit_w
        } else {
            self.config.reset_power_limit_w
        };
        self.apply_exact_power(watts).await
    }

    /// Phase 5: apply an arbitrary wattage (PI controller output). Bypasses
    /// the throttle cooldown since PI output changes are gated by the
    /// controller's own `sample_period` + `output_deadband_w`.
    ///
    /// Phase 6b-audit-triage v2: if a previous call failed (permission
    /// denied on some systemd hardening configs), skip retries for
    /// `POWER_LIMIT_RETRY_BACKOFF` to avoid log spam. NVML call behaviour
    /// doesn't change in that window, retrying every 2s is pointless.
    async fn apply_exact_power(&mut self, watts: u32) -> bool {
        // Back-off gate: if we failed recently, don't even try.
        if let Some(failed_at) = self.power_limit_failed_at {
            if failed_at.elapsed() < POWER_LIMIT_RETRY_BACKOFF {
                return false;
            }
            // Back-off window elapsed; clear the marker and retry once.
            self.power_limit_failed_at = None;
        }

        let nvml = self.nvml.clone();
        let result = tokio::task::spawn_blocking(move || nvml.set_power_limit_w(watts)).await;

        match result {
            Ok(Ok(())) => {
                info!(watts, "NVML power-limit applied");
                true
            }
            Ok(Err(e)) => {
                // First failure: log. Subsequent failures within the
                // back-off window: silent (we return early above).
                error!(watts, %e, "NVML power-limit failed — backing off 5 min");
                self.power_limit_failed_at = Some(Instant::now());
                false
            }
            Err(e) => {
                error!(%e, "throttle task join failed");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(base: Instant, secs: u64) -> Instant {
        base + Duration::from_secs(secs)
    }

    #[test]
    #[should_panic(expected = "off_temp")]
    fn policy_refuses_inverted_thresholds() {
        // Misconfiguration must fail fast, not silently flap
        let _ = ThrottlePolicy::new(70.0, 85.0, 0.7);
    }

    #[test]
    fn thermal_engage_at_on_temp() {
        let policy = ThrottlePolicy::new(85.0, 78.0, 0.7);
        let now = Instant::now();
        let (d, trig) = policy.evaluate_thermal(85.0, now);
        assert_eq!(d, ThrottleDecision::Engage);
        assert_eq!(trig, Trigger::Temperature);
    }

    #[test]
    fn thermal_no_change_between_thresholds() {
        let policy = ThrottlePolicy::new(85.0, 78.0, 0.7);
        let now = Instant::now();
        // 80°C: below ON threshold, above OFF threshold — hysteresis hold
        let (d, _) = policy.evaluate_thermal(80.0, now);
        assert_eq!(d, ThrottleDecision::NoChange);
    }

    #[test]
    fn thermal_release_requires_cooldown() {
        let mut policy = ThrottlePolicy::new(85.0, 78.0, 0.7);
        let t0 = Instant::now();
        policy.mark_applied(ThrottleDecision::Engage, t0);

        // Immediately try to release at cool temperature — blocked by cooldown
        let (d, _) = policy.evaluate_thermal(70.0, t(t0, 10));
        assert_eq!(d, ThrottleDecision::NoChange);

        // After 30s cooldown, release is allowed
        let (d, _) = policy.evaluate_thermal(70.0, t(t0, 30));
        assert_eq!(d, ThrottleDecision::Release);
    }

    #[test]
    fn thermal_hysteresis_does_not_flap() {
        let mut policy = ThrottlePolicy::new(85.0, 78.0, 0.7);
        let t0 = Instant::now();
        // Engage
        policy.mark_applied(ThrottleDecision::Engage, t0);
        // At 80°C (between thresholds), nothing happens even after cooldown
        let (d, _) = policy.evaluate_thermal(80.0, t(t0, 60));
        assert_eq!(d, ThrottleDecision::NoChange);
        // At 79°C still nothing (above OFF)
        let (d, _) = policy.evaluate_thermal(79.0, t(t0, 61));
        assert_eq!(d, ThrottleDecision::NoChange);
        // Only at 78.0 exactly or below does release fire
        let (d, _) = policy.evaluate_thermal(78.0, t(t0, 62));
        assert_eq!(d, ThrottleDecision::Release);
    }

    #[test]
    fn stress_engage_at_threshold_plus() {
        let policy = ThrottlePolicy::new(85.0, 78.0, 0.7);
        let now = Instant::now();
        let (d, _) = policy.evaluate_stress(0.71, now);
        assert_eq!(d, ThrottleDecision::Engage);
    }

    #[test]
    fn stress_deadband_prevents_immediate_release() {
        let mut policy = ThrottlePolicy::new(85.0, 78.0, 0.7);
        let t0 = Instant::now();
        policy.mark_applied(ThrottleDecision::Engage, t0);

        // After cooldown: stress = 0.65 is inside the deadband (0.6..0.7),
        // must NOT release
        let (d, _) = policy.evaluate_stress(0.65, t(t0, 35));
        assert_eq!(d, ThrottleDecision::NoChange);
        // Only below threshold - deadband (0.6) does release fire
        let (d, _) = policy.evaluate_stress(0.59, t(t0, 36));
        assert_eq!(d, ThrottleDecision::Release);
    }

    #[test]
    fn recovery_window_elapses() {
        let mut policy = ThrottlePolicy::new(85.0, 78.0, 0.7);
        let t0 = Instant::now();
        policy.mark_applied(ThrottleDecision::Engage, t0);
        policy.mark_applied(ThrottleDecision::Release, t(t0, 30));

        assert!(!policy.recovery_elapsed(t(t0, 60)));  // only 30s elapsed
        assert!(!policy.recovery_elapsed(t(t0, 89)));  // 59s — not yet
        assert!(policy.recovery_elapsed(t(t0, 91)));   // 61s — recovery done
        // Subsequent calls don't re-fire
        assert!(!policy.recovery_elapsed(t(t0, 200)));
    }

    #[test]
    fn mark_applied_nochange_does_not_update_timestamp() {
        let mut policy = ThrottlePolicy::new(85.0, 78.0, 0.7);
        let t0 = Instant::now();
        policy.mark_applied(ThrottleDecision::NoChange, t0);
        assert!(policy.last_change_at.is_none());
        assert!(!policy.throttle_active);
    }
}
