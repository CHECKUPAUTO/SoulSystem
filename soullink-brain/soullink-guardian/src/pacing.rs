//! Energy pacing — continuous PI controller for GPU power-limit.
//!
//! # Motivation
//!
//! The Phase 1 throttle is binary: below 78 °C → full 115 W; above 85 °C →
//! clamped 90 W. Under sustained load the GPU oscillates between these
//! states, which (a) hurts Ollama latency during the clamp step, and (b)
//! wastes energy by overshooting the thermal setpoint during recovery.
//!
//! A PI controller modulates the power-limit continuously so the GPU
//! settles at a **target temperature** with minimal oscillation. On the
//! T430 / RTX 4060 / Debian 13 target, the thermal system has slow
//! dynamics (several seconds per °C change), so a pure P+I (no D) with a
//! 2-second sample period is appropriate.
//!
//! # Design
//!
//! * **Setpoint**: 75 °C (configurable via `pacing_target_c`).
//! * **Output**: power-limit in W, clamped to [`power_min_w`, `power_max_w`]
//!   from NVML constraints.
//! * **Integral windup guard**: clamp the integral term to ±20 W-equivalent
//!   so a sustained over-temp doesn't drive the integral to infinity.
//! * **Output deadband**: don't re-issue an NVML write if the new target
//!   is within 3 W of the current. Avoids churn on micro-oscillations.
//! * **Fallback**: if temp exceeds `hard_cap_temp_c` (default 87 °C), the
//!   PI output is overridden with a hard `power_min_w` clamp until temp
//!   drops below `hard_cap_temp_c - 3` °C. This is the safety net.
//!
//! The controller is opt-in: when `PacingConfig::enabled = false`, it's
//! a no-op and the Phase 1 hysteresis runs as before.

use std::time::{Duration, Instant};

/// Decision returned by [`PiController::step`].
#[derive(Debug, PartialEq)]
pub enum PacingAction {
    /// Apply a new power-limit (W).
    Apply(u32),
    /// Within deadband — no-op.
    NoChange,
    /// Hard cap engaged: emergency clamp (always applied regardless of deadband).
    HardCap(u32),
}

#[derive(Debug, Clone)]
pub struct PacingConfig {
    pub enabled: bool,
    pub target_c: f32,
    pub hard_cap_temp_c: f32,
    pub kp: f32, // proportional gain (W/°C)
    pub ki: f32, // integral gain (W/°C·s)
    pub sample_period: Duration,
    pub output_deadband_w: u32,
    pub power_min_w: u32,
    pub power_max_w: u32,
    pub integral_clamp_w: f32,
}

impl Default for PacingConfig {
    fn default() -> Self {
        Self {
            enabled: false, // opt-in
            target_c: 75.0,
            hard_cap_temp_c: 87.0,
            kp: 3.0, // 1 °C over → subtract 3 W
            ki: 0.5, // sustained 1 °C over → additional 0.5 W/s
            sample_period: Duration::from_secs(2),
            output_deadband_w: 3,
            power_min_w: 70,
            power_max_w: 115,
            integral_clamp_w: 20.0,
        }
    }
}

pub struct PiController {
    cfg: PacingConfig,
    integral: f32, // accumulated error × time (W)
    last_sample_at: Option<Instant>,
    last_applied_w: u32,
    hard_cap_engaged: bool,
}

impl PiController {
    pub fn new(cfg: PacingConfig) -> Self {
        let initial = cfg.power_max_w;
        Self {
            cfg,
            integral: 0.0,
            last_sample_at: None,
            last_applied_w: initial,
            hard_cap_engaged: false,
        }
    }

    /// Take one temperature sample and return the action to take. `now` is
    /// passed explicitly so tests can drive virtual time.
    pub fn step(&mut self, temp_c: f32, now: Instant) -> PacingAction {
        if !self.cfg.enabled {
            return PacingAction::NoChange;
        }

        // ── 1. Hard cap gate ──────────────────────────────────────────
        if temp_c >= self.cfg.hard_cap_temp_c {
            self.hard_cap_engaged = true;
            self.integral = 0.0; // reset windup on emergency
            self.last_applied_w = self.cfg.power_min_w;
            self.last_sample_at = Some(now);
            return PacingAction::HardCap(self.cfg.power_min_w);
        }
        if self.hard_cap_engaged {
            if temp_c < self.cfg.hard_cap_temp_c - 3.0 {
                self.hard_cap_engaged = false;
                // Let PI resume from a clean state
            } else {
                // Still cooling back below release threshold
                self.last_sample_at = Some(now);
                return PacingAction::NoChange;
            }
        }

        // ── 2. Enforce sample period ──────────────────────────────────
        let dt = match self.last_sample_at {
            None => {
                self.last_sample_at = Some(now);
                return PacingAction::NoChange; // first call — seed only
            }
            Some(last) => {
                let elapsed = now.duration_since(last);
                if elapsed < self.cfg.sample_period {
                    return PacingAction::NoChange;
                }
                elapsed.as_secs_f32()
            }
        };
        self.last_sample_at = Some(now);

        // ── 3. PI computation ─────────────────────────────────────────
        // Error: positive when GPU too hot (temp > target)
        let error = temp_c - self.cfg.target_c;

        // Integrate error over time
        self.integral += error * dt;
        // Clamp integral to prevent windup
        self.integral = self
            .integral
            .clamp(-self.cfg.integral_clamp_w, self.cfg.integral_clamp_w);

        // Output: subtract correction from max power
        let correction = self.cfg.kp * error + self.cfg.ki * self.integral;
        let raw_output = self.cfg.power_max_w as f32 - correction;
        let clamped = raw_output
            .clamp(self.cfg.power_min_w as f32, self.cfg.power_max_w as f32)
            .round() as u32;

        // ── 4. Output deadband ────────────────────────────────────────
        let delta = clamped.abs_diff(self.last_applied_w);
        if delta < self.cfg.output_deadband_w {
            return PacingAction::NoChange;
        }

        self.last_applied_w = clamped;
        PacingAction::Apply(clamped)
    }

    pub fn last_applied_w(&self) -> u32 {
        self.last_applied_w
    }
    pub fn integral(&self) -> f32 {
        self.integral
    }
    pub fn hard_cap_engaged(&self) -> bool {
        self.hard_cap_engaged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_cfg() -> PacingConfig {
        PacingConfig {
            enabled: true,
            ..PacingConfig::default()
        }
    }

    fn t(base: Instant, secs: f32) -> Instant {
        base + Duration::from_millis((secs * 1000.0) as u64)
    }

    #[test]
    fn disabled_always_nochange() {
        let mut pi = PiController::new(PacingConfig::default());
        let now = Instant::now();
        assert_eq!(pi.step(90.0, now), PacingAction::NoChange);
    }

    #[test]
    fn first_call_seeds_only() {
        let mut pi = PiController::new(enabled_cfg());
        assert_eq!(pi.step(78.0, Instant::now()), PacingAction::NoChange);
    }

    #[test]
    fn below_setpoint_no_correction() {
        let mut pi = PiController::new(enabled_cfg());
        let t0 = Instant::now();
        pi.step(60.0, t0); // seed
                           // Cool temperature: correction pushes power_limit above max → clamped.
                           // Output stays at max — no change vs initial (115).
        let out = pi.step(60.0, t(t0, 3.0));
        assert_eq!(out, PacingAction::NoChange);
    }

    #[test]
    fn above_setpoint_reduces_power() {
        let mut pi = PiController::new(enabled_cfg());
        let t0 = Instant::now();
        pi.step(75.0, t0); // seed at setpoint
                           // 80°C → 5°C over → kp*5 = 15 W correction (plus small integral)
        let out = pi.step(80.0, t(t0, 3.0));
        match out {
            PacingAction::Apply(w) => {
                assert!((70..115).contains(&w), "expected reduced power, got {w}");
                assert!(w <= 100, "expected ≥15W correction applied, got {w}");
            }
            other => panic!("expected Apply, got {other:?}"),
        }
    }

    #[test]
    fn hard_cap_engages_above_threshold() {
        let mut pi = PiController::new(enabled_cfg());
        let t0 = Instant::now();
        let out = pi.step(88.0, t0);
        assert_eq!(out, PacingAction::HardCap(70));
        assert!(pi.hard_cap_engaged());
        assert_eq!(pi.integral(), 0.0, "integral reset on hard cap");
    }

    #[test]
    fn hard_cap_releases_with_hysteresis() {
        let mut pi = PiController::new(enabled_cfg());
        let t0 = Instant::now();
        pi.step(88.0, t0);
        // 85°C: still above cap - 3 (84°C), stays engaged
        let out = pi.step(85.0, t(t0, 3.0));
        assert_eq!(out, PacingAction::NoChange);
        assert!(pi.hard_cap_engaged());

        // 83°C: below cap - 3, release. But still hot vs target (75) so
        // PI applies a correction. Seed the sample clock first.
        let _ = pi.step(83.0, t(t0, 5.0));
        let out = pi.step(83.0, t(t0, 8.0));
        assert!(!pi.hard_cap_engaged());
        // Output is either a correction-reducing Apply or NoChange if
        // deadband swallows it (both are valid post-release behaviours)
        assert!(matches!(
            out,
            PacingAction::Apply(_) | PacingAction::NoChange
        ));
    }

    #[test]
    fn output_deadband_suppresses_small_changes() {
        let mut pi = PiController::new(PacingConfig {
            output_deadband_w: 10,
            ..enabled_cfg()
        });
        let t0 = Instant::now();
        pi.step(75.0, t0); // seed at setpoint

        // Tiny over-temp: correction < deadband → NoChange
        let out = pi.step(75.5, t(t0, 3.0));
        assert_eq!(out, PacingAction::NoChange);
    }

    #[test]
    fn integral_windup_clamped() {
        let mut pi = PiController::new(PacingConfig {
            integral_clamp_w: 5.0,
            ki: 1.0,
            ..enabled_cfg()
        });
        let t0 = Instant::now();
        pi.step(80.0, t0); // seed

        // Drive 20 °C over-temp for 10 sample periods (20 s)
        for i in 1..=10 {
            let _ = pi.step(95.0_f32.min(86.9), t(t0, i as f32 * 2.0));
            // cap at 86.9 to avoid hard-cap short-circuit
        }
        // Integral must not exceed clamp
        let i = pi.integral();
        assert!(
            i.abs() <= 5.0 + 0.001,
            "integral {i} should be clamped to ±5"
        );
    }

    #[test]
    fn settles_near_setpoint_over_time() {
        // Simulate a cooling trajectory: 85°C drifting down over 30 s.
        // Output should trend towards max power as temp crosses setpoint.
        let mut pi = PiController::new(enabled_cfg());
        let t0 = Instant::now();
        pi.step(85.0, t0);

        let mut trace: Vec<u32> = vec![];
        for i in 1..=15 {
            // Temperature linearly cools from 85 to 70 over 30 s
            let temp = 85.0 - (i as f32 * 1.0);
            if let PacingAction::Apply(w) = pi.step(temp, t(t0, i as f32 * 2.0)) {
                trace.push(w);
            }
        }
        // The last applied should be higher than the first (recovering)
        if trace.len() >= 2 {
            assert!(
                trace.last().unwrap() >= trace.first().unwrap(),
                "output should recover as temp drops: {trace:?}"
            );
        }
    }
}
