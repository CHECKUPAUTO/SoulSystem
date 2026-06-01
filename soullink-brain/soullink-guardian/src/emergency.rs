//! EmergencyReflex — 100 ms scan loop for hardware hang detection.
//!
//! # Testing surface (Phase 2b)
//!
//! The actor is split into a `Core` containing all deterministic state and
//! pure-function transitions, and the `run` wrapper that ticks it from tokio.
//! `Core::scan_step` takes the current `Instant` so tests drive virtual time.
//! This is how we assert "no reset for 60 ticks of noisy throttle bits" or
//! "hang detected at exactly threshold_ms, not before".
//!
//! # Design
//!
//! * SM-clock-stall hang detection — legitimate reset path
//! * `throttle_reasons` observed but **never** used as XID signal (p1a)
//! * Real XID errors come from `KmsgReader` via the event bus; this actor
//!   reacts to them through `on_xid_event`, which filters by
//!   [`FATAL_XID_CODES`].
//! * **Rate-limit**: max one reset per [`RESET_RATE_LIMIT`], regardless of
//!   reason. Protects against reset loops when the underlying fault persists.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{
    process::Command,
    sync::{broadcast, mpsc},
    time,
};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use soullink_core::{validation::canonicalize_and_verify_script, GuardianEvent};
use soullink_nvml::NvmlBridge;

pub const XID_SCAN_INTERVAL_MS: u64 = 100;
pub const RESET_RATE_LIMIT: Duration = Duration::from_secs(5 * 60);

const THROTTLE_LOG_EVERY: u32 = 10;

/// XID codes that legitimately warrant a PCIe reset.
///
/// 43 = GPU stopped processing; 44 = graphics engine hung; 79 = double-bit
/// ECC; 119 = GSP RPC timeout. Any other code is logged but does not
/// trigger a reset — most non-fatal XIDs (e.g. 13 "Graphics Exception")
/// clear with a driver-internal recovery.
pub const FATAL_XID_CODES: &[u32] = &[43, 44, 79, 119];

#[derive(Debug, PartialEq, Eq)]
pub enum ScanOutcome {
    Idle,
    HangTracking,
    HangTriggered,
    ResetSuppressedByRateLimit,
    ResetInProgress,
}

// ─── Deterministic core ──────────────────────────────────────────────────────

pub struct Core {
    pub hang_threshold_ms: u64,
    pub prev_sm_clock: u32,
    pub stall_start: Option<Instant>,
    pub last_reset_at: Option<Instant>,
    pub reset_in_progress: bool,
    pub tick_counter: u32,
    pub prev_throttle: u64,
    /// GPU's maximum SM boost clock in MHz. If the current sm_clock is at
    /// or near this value, we treat it as "saturated" not "hung" — a
    /// saturated GPU is doing work, not stuck.
    /// Set to 0 to disable the saturation check (legacy behaviour: any
    /// non-zero stuck clock triggers).
    pub max_sm_clock_mhz: u32,
}

impl Core {
    pub fn new(hang_threshold_ms: u64) -> Self {
        Self {
            hang_threshold_ms,
            prev_sm_clock: 0,
            stall_start: None,
            last_reset_at: None,
            reset_in_progress: false,
            tick_counter: 0,
            prev_throttle: 0,
            max_sm_clock_mhz: 0,
        }
    }

    /// Set the GPU's max SM boost clock. Call this once at startup after
    /// querying NVML — avoids false-positive hang detection when the GPU
    /// is legitimately pinned at boost clock by a heavy workload.
    pub fn with_max_sm_clock(mut self, max_sm_clock_mhz: u32) -> Self {
        self.max_sm_clock_mhz = max_sm_clock_mhz;
        self
    }

    pub fn scan_step(&mut self, sm_clock: u32, throttle_reasons: u64, now: Instant) -> ScanOutcome {
        if self.reset_in_progress {
            return ScanOutcome::ResetInProgress;
        }
        self.tick_counter = self.tick_counter.wrapping_add(1);

        // ── Phase 6b-audit-triage v2: refined hang detection ─────────────
        //
        // A real GPU hang has a very specific signature: the SM clock is
        // stuck AT A LOW VALUE. Not stuck at 2460 MHz (that's moderate
        // load), not stuck at 2775 MHz (that's boost saturation). A hang
        // is when the driver has lost command queue progress and the
        // clock collapses to a base state.
        //
        // The v1 fix (95% max threshold) was too restrictive: it only
        // caught "pinned at boost" and still flagged intermediate loads
        // like 2460 MHz as hang. That produced the 1740 false positives
        // observed in production 2026-04-17 through 2026-04-19.
        //
        // v2 applies TWO complementary guards:
        //
        //   Guard A — boost range (80% of max)
        //     If sm_clock >= 80% × max_sm_clock_mhz, the GPU is doing
        //     heavy work. Not a hang regardless of stall duration.
        //     80% of 2775 MHz = 2220 MHz. Covers Ollama's boost oscillation
        //     (2460-2790 MHz observed) entirely.
        //
        //   Guard B — hang floor (500 MHz absolute)
        //     A plausible hang has sm_clock < MIN_HANG_CLOCK_MHZ. Real
        //     hangs in NVIDIA driver logs show sm_clock dropping to 0
        //     or a few hundred MHz. Anything above 500 MHz is active
        //     work at worst, not a hang.
        //
        // We trigger hang tracking only when BOTH A-negated AND B apply:
        //     clock stuck  AND  outside boost range  AND  below hang floor
        //
        // When max_sm_clock_mhz is 0 (NVML unavailable), guard A cannot
        // evaluate; we fall back to checking guard B alone (still safer
        // than the legacy behaviour, which triggered on any non-zero
        // stagnant clock).
        const MIN_HANG_CLOCK_MHZ: u32 = 500;

        let in_boost_range =
            self.max_sm_clock_mhz > 0 && sm_clock >= (self.max_sm_clock_mhz * 80 / 100);
        let plausibly_hung = sm_clock < MIN_HANG_CLOCK_MHZ && sm_clock > 0;

        let outcome = if sm_clock == self.prev_sm_clock && !in_boost_range && plausibly_hung {
            match self.stall_start {
                None => {
                    self.stall_start = Some(now);
                    ScanOutcome::HangTracking
                }
                Some(start) => {
                    let elapsed_ms = now.duration_since(start).as_millis() as u64;
                    if elapsed_ms >= self.hang_threshold_ms {
                        if let Some(last) = self.last_reset_at {
                            if now.duration_since(last) < RESET_RATE_LIMIT {
                                return ScanOutcome::ResetSuppressedByRateLimit;
                            }
                        }
                        self.stall_start = None;
                        ScanOutcome::HangTriggered
                    } else {
                        ScanOutcome::HangTracking
                    }
                }
            }
        } else {
            self.stall_start = None;
            ScanOutcome::Idle
        };
        self.prev_sm_clock = sm_clock;

        // Throttle-reasons: observational only
        if throttle_reasons != self.prev_throttle {
            debug!(
                bits = format_args!("0b{:016b}", throttle_reasons),
                prev = format_args!("0b{:016b}", self.prev_throttle),
                "throttle reasons changed"
            );
            self.prev_throttle = throttle_reasons;
        }
        if throttle_reasons != 0 && self.tick_counter % THROTTLE_LOG_EVERY == 0 {
            debug!(
                bits = format_args!("0b{:016b}", throttle_reasons),
                "GPU clock limited (informational)"
            );
        }

        outcome
    }

    pub fn should_reset_on_xid(&self, code: u32, now: Instant) -> bool {
        if !FATAL_XID_CODES.contains(&code) {
            return false;
        }
        if let Some(last) = self.last_reset_at {
            if now.duration_since(last) < RESET_RATE_LIMIT {
                return false;
            }
        }
        true
    }

    pub fn mark_reset_at(&mut self, now: Instant) {
        self.last_reset_at = Some(now);
    }
}

// ─── Async wrapper ───────────────────────────────────────────────────────────

pub struct EmergencyReflex {
    nvml: Arc<NvmlBridge>,
    tx: mpsc::Sender<GuardianEvent>,
    shutdown: broadcast::Receiver<()>,
    reset_script: PathBuf,
    core: Core,
    verified_script: Option<PathBuf>,
    /// Phase 6b audit triage: kill-switch. When false, `scan()` skips the
    /// SM-clock stall heuristic entirely. XID-based reset (authoritative)
    /// via `on_xid_event` is unaffected.
    hang_detector_enabled: bool,
}

impl EmergencyReflex {
    pub fn new(
        nvml: Arc<NvmlBridge>,
        tx: mpsc::Sender<GuardianEvent>,
        shutdown: broadcast::Receiver<()>,
        reset_script: PathBuf,
        hang_threshold_ms: u64,
    ) -> Self {
        Self::with_options(nvml, tx, shutdown, reset_script, hang_threshold_ms, true)
    }

    /// Like `new` but lets the caller explicitly choose whether the SM
    /// clock hang heuristic is armed. Set to false to silence false
    /// positives while keeping the XID path active.
    pub fn with_options(
        nvml: Arc<NvmlBridge>,
        tx: mpsc::Sender<GuardianEvent>,
        shutdown: broadcast::Receiver<()>,
        reset_script: PathBuf,
        hang_threshold_ms: u64,
        hang_detector_enabled: bool,
    ) -> Self {
        // Read the GPU's max boost SM clock once at boot. Feeding it into
        // the Core prevents the hang detector from firing on a GPU that's
        // simply pinned at boost by Ollama / brain workloads.
        let max_sm = nvml.max_clock_sm_mhz();
        if max_sm == 0 {
            tracing::warn!(
                "NVML did not return max SM clock — saturation check disabled, \
                 hang detector will fall back to legacy behaviour"
            );
        } else {
            tracing::info!(
                max_sm_clock_mhz = max_sm,
                "hang detector: saturation check enabled"
            );
        }
        if !hang_detector_enabled {
            tracing::warn!(
                "hang_detector_enabled=false — SM-clock stall heuristic DISABLED. \
                 XID-based reset via /dev/kmsg remains active."
            );
        }
        Self {
            nvml,
            tx,
            shutdown,
            reset_script,
            core: Core::new(hang_threshold_ms).with_max_sm_clock(max_sm),
            verified_script: None,
            hang_detector_enabled,
        }
    }

    pub async fn run(mut self) {
        info!(
            scan_ms = XID_SCAN_INTERVAL_MS,
            hang_ms = self.core.hang_threshold_ms,
            max_sm_clock_mhz = self.core.max_sm_clock_mhz,
            "EmergencyReflex started (V13.5-p2b: Core/Scanner split, rate-limited resets)"
        );

        match canonicalize_and_verify_script(&self.reset_script) {
            Ok(p) => {
                info!(path = %p.display(), "reset script verified at startup");
                self.verified_script = Some(p);
            }
            Err(e) => {
                error!(
                    %e,
                    script = %self.reset_script.display(),
                    "reset script failed startup verification — hang reset will fail"
                );
            }
        }

        let mut ticker = time::interval(Duration::from_millis(XID_SCAN_INTERVAL_MS));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => { self.scan().await; }
                _ = self.shutdown.recv() => { info!("EmergencyReflex: shutdown"); break; }
            }
        }
    }

    async fn scan(&mut self) {
        if !self.hang_detector_enabled {
            return;
        }
        let sm_clock = self.nvml.clock_sm_mhz();
        let throttle = self.nvml.throttle_reasons();
        let now = Instant::now();

        match self.core.scan_step(sm_clock, throttle, now) {
            ScanOutcome::HangTriggered => {
                warn!(sm_clock_mhz = sm_clock, "GPU HANG — SM clock stalled");
                let _ = self.tx.try_send(GuardianEvent::GpuHang {
                    duration_ms: self.core.hang_threshold_ms,
                });
                self.core.reset_in_progress = true;
                self.execute_reset(Uuid::new_v4()).await;
                self.core.reset_in_progress = false;
                self.core.mark_reset_at(Instant::now());
            }
            ScanOutcome::ResetSuppressedByRateLimit => {
                warn!(
                    sm_clock_mhz = sm_clock,
                    "GPU hang detected but reset suppressed — rate limit active (<5 min since last reset)"
                );
            }
            _ => {}
        }
    }

    /// Handle a real XID event from [`crate::kmsg_reader::KmsgReader`].
    pub async fn on_xid_event(&mut self, code: u32, count: u64) {
        let now = Instant::now();
        if self.core.should_reset_on_xid(code, now) {
            error!(code, count, "XID FATAL — executing PCIe reset");
            self.core.reset_in_progress = true;
            self.execute_reset(Uuid::new_v4()).await;
            self.core.reset_in_progress = false;
            self.core.mark_reset_at(Instant::now());
        } else if FATAL_XID_CODES.contains(&code) {
            warn!(
                code,
                count, "XID fatal code received but reset suppressed by rate-limit"
            );
        } else {
            debug!(code, count, "XID non-fatal — driver will recover");
        }
    }

    async fn execute_reset(&mut self, corr: Uuid) {
        let script = match canonicalize_and_verify_script(&self.reset_script) {
            Ok(p) => p,
            Err(e) => {
                error!(%e, %corr, "reset ABORTED — script hardening check failed");
                let _ = self.tx.try_send(GuardianEvent::ResetCompleted {
                    success: false,
                    elapsed_ms: 0,
                });
                return;
            }
        };
        warn!(script = %script.display(), %corr, "executing atomic PCIe reset");

        let start = Instant::now();
        let result = Command::new(&script).output().await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                let success = output.status.success();
                if success {
                    info!(elapsed_ms, %corr, "PCIe reset OK");
                } else {
                    error!(
                        elapsed_ms, %corr,
                        code = output.status.code().unwrap_or(-1),
                        stderr = %String::from_utf8_lossy(&output.stderr),
                        "PCIe reset FAILED"
                    );
                }
                let _ = self.tx.try_send(GuardianEvent::ResetCompleted {
                    success,
                    elapsed_ms,
                });
            }
            Err(e) => {
                error!(%e, %corr, "reset script execution error");
                let _ = self.tx.try_send(GuardianEvent::ResetCompleted {
                    success: false,
                    elapsed_ms,
                });
            }
        }
    }
}

// ─── Unit tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn t(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    /// **Regression test for the 2026-04-17 20:29 incident.**
    ///
    /// Scenario: `throttle_reasons` bits flip wildly 60 times in a row
    /// (normal under load), SM clock is healthy. Expected: zero
    /// `HangTriggered` outcomes.
    #[test]
    fn regression_20h29_throttle_bits_never_trigger_reset() {
        let mut core = Core::new(30_000);
        let t0 = Instant::now();
        for i in 0..60u32 {
            let sm_clock = 1800 + (i % 3) as u32 * 10; // varying — never stalled
            let throttle_bits = 0x20 | (i as u64 & 0x7F); // chaotic noise
            let outcome = core.scan_step(sm_clock, throttle_bits, t(t0, i as u64 * 100));
            assert_ne!(
                outcome,
                ScanOutcome::HangTriggered,
                "tick {i}: throttle-bits-only must never trigger reset"
            );
        }
    }

    /// **Regression test for the 2026-04-18 saturation false-positive.**
    ///
    /// Production observed: Ollama running full-blast on RTX 4060, SM
    /// clock pinned at 2775/2790 MHz for minutes. Pre-fix: 1740 false
    /// "GPU HANG" events in 7 days.
    #[test]
    fn regression_2026_04_18_sm_saturated_is_not_hang() {
        // RTX 4060: max boost SM = 2775 MHz
        let mut core = Core::new(30_000).with_max_sm_clock(2775);
        let t0 = Instant::now();
        for i in 0..60u32 {
            let outcome = core.scan_step(2775, 0, t(t0, i as u64 * 500));
            assert_ne!(
                outcome,
                ScanOutcome::HangTriggered,
                "tick {i}: sm_clock at max boost must never trigger hang reset"
            );
        }
        // 2790 MHz (transient boost above nominal):
        let mut core2 = Core::new(30_000).with_max_sm_clock(2775);
        for i in 0..60u32 {
            let outcome = core2.scan_step(2790, 0, t(t0, i as u64 * 500));
            assert_ne!(outcome, ScanOutcome::HangTriggered);
        }
    }

    /// **Regression test for the 2026-04-19 15:28 incident.**
    ///
    /// Even after the v1 fix (95% saturation threshold), the detector
    /// still fired on sm_clock=2460 MHz (88% of max) — which is normal
    /// moderate load, not a hang. v2 adds a 500 MHz absolute floor:
    /// anything above 500 MHz is never a hang.
    ///
    /// This test pins the clock at 2460 MHz (the exact value observed
    /// in production 2026-04-18 20:35 and 2026-04-19 15:28) for 60
    /// consecutive ticks. Must never trigger.
    #[test]
    fn regression_2026_04_19_2460_mhz_is_not_a_hang() {
        let mut core = Core::new(30_000).with_max_sm_clock(2775);
        let t0 = Instant::now();
        for i in 0..60u32 {
            let outcome = core.scan_step(2460, 0, t(t0, i as u64 * 500));
            assert_ne!(
                outcome,
                ScanOutcome::HangTriggered,
                "tick {i}: sm_clock at 2460 MHz (moderate load) must not trigger hang"
            );
        }
    }

    /// Any work-range clock (1800, 2000, 2460 MHz etc.) is above the
    /// 500 MHz hang floor → v2 treats it as legitimate work.
    #[test]
    fn work_range_clocks_never_trigger_hang() {
        for &clock in &[700u32, 1000, 1500, 1800, 2000, 2460] {
            let mut core = Core::new(1000).with_max_sm_clock(2775);
            let t0 = Instant::now();
            for i in 0..15 {
                let out = core.scan_step(clock, 0, t(t0, i * 100));
                assert_ne!(
                    out,
                    ScanOutcome::HangTriggered,
                    "clock {clock} MHz tick {i}: above 500 MHz floor, must not hang"
                );
            }
        }
    }

    /// A truly hung GPU: clock stuck at a low value (< 500 MHz floor).
    /// This IS a hang and must trigger.
    #[test]
    fn low_clock_stuck_below_floor_triggers_hang() {
        let mut core = Core::new(1000).with_max_sm_clock(2775);
        let t0 = Instant::now();
        // 300 MHz: below 500 MHz floor AND below 80% of 2775 (=2220).
        // Both guards agree: plausible hang.
        assert_eq!(core.scan_step(300, 0, t0), ScanOutcome::HangTracking);
        for i in 1..10 {
            let out = core.scan_step(300, 0, t(t0, i * 100));
            assert_eq!(out, ScanOutcome::HangTracking, "tick {i}");
        }
        let out = core.scan_step(300, 0, t(t0, 1100));
        assert_eq!(out, ScanOutcome::HangTriggered);
    }

    /// Edge case: clock exactly at the 500 MHz boundary. By strict
    /// inequality `sm_clock < MIN_HANG_CLOCK_MHZ`, 500 MHz is NOT a hang.
    #[test]
    fn clock_at_floor_boundary_not_hang() {
        let mut core = Core::new(1000).with_max_sm_clock(2775);
        let t0 = Instant::now();
        for i in 0..15 {
            let out = core.scan_step(500, 0, t(t0, i * 100));
            assert_ne!(
                out,
                ScanOutcome::HangTriggered,
                "clock exactly at MIN_HANG_CLOCK_MHZ=500 must not trigger"
            );
        }
    }

    /// Below boundary: 499 MHz stuck DOES trigger (hang territory).
    #[test]
    fn clock_just_below_floor_triggers_hang() {
        let mut core = Core::new(1000).with_max_sm_clock(2775);
        let t0 = Instant::now();
        core.scan_step(499, 0, t0);
        for i in 1..10 {
            let out = core.scan_step(499, 0, t(t0, i * 100));
            assert_eq!(out, ScanOutcome::HangTracking);
        }
        let out = core.scan_step(499, 0, t(t0, 1100));
        assert_eq!(out, ScanOutcome::HangTriggered);
    }

    /// When NVML returns max_clock=0, guard A (boost range) is disabled
    /// but guard B (hang floor) still applies. A clock stuck at high
    /// values is NOT triggered (safer than v1 legacy): we only trigger
    /// on clocks genuinely below 500 MHz.
    #[test]
    fn zero_max_clock_still_respects_hang_floor() {
        let mut core = Core::new(1000); // max_sm_clock=0 (NVML unavailable)
        let t0 = Instant::now();
        // 2775 MHz stuck: guard B says plausibly_hung=false → no trigger
        for i in 0..15 {
            let out = core.scan_step(2775, 0, t(t0, i * 100));
            assert_ne!(
                out,
                ScanOutcome::HangTriggered,
                "max_clock=0 but 2775 MHz is above floor — must not trigger"
            );
        }
        // But 300 MHz stuck still triggers (guard B fires alone)
        let mut core2 = Core::new(1000);
        let t0 = Instant::now();
        core2.scan_step(300, 0, t0);
        for i in 1..10 {
            core2.scan_step(300, 0, t(t0, i * 100));
        }
        let out = core2.scan_step(300, 0, t(t0, 1100));
        assert_eq!(out, ScanOutcome::HangTriggered);
    }

    #[test]
    fn sm_clock_changing_resets_stall_timer() {
        let mut core = Core::new(1000).with_max_sm_clock(2775);
        let t0 = Instant::now();
        core.scan_step(300, 0, t0);
        core.scan_step(300, 0, t(t0, 400));
        core.scan_step(300, 0, t(t0, 800));
        core.scan_step(350, 0, t(t0, 900));
        let out = core.scan_step(350, 0, t(t0, 1000));
        assert_eq!(out, ScanOutcome::HangTracking);
        let out = core.scan_step(350, 0, t(t0, 1999));
        assert_eq!(out, ScanOutcome::HangTracking);
        let out = core.scan_step(350, 0, t(t0, 2100));
        assert_eq!(out, ScanOutcome::HangTriggered);
    }

    #[test]
    fn sm_clock_zero_never_triggers_hang() {
        // Device absent / driver unloaded — sm_clock=0 means we have no
        // reading at all, must not produce a reset loop
        let mut core = Core::new(100).with_max_sm_clock(2775);
        let t0 = Instant::now();
        for i in 0..50u64 {
            let out = core.scan_step(0, 0, t(t0, i * 100));
            assert_ne!(
                out,
                ScanOutcome::HangTriggered,
                "SM clock=0 at tick {i} must not trigger reset"
            );
        }
    }

    #[test]
    fn rate_limit_suppresses_rapid_second_hang() {
        let mut core = Core::new(1000).with_max_sm_clock(2775);
        let t0 = Instant::now();
        // Use 300 MHz (plausibly hung) to drive the actual trigger path
        core.scan_step(300, 0, t0);
        for i in 1..=10 {
            core.scan_step(300, 0, t(t0, i * 100));
        }
        let out = core.scan_step(300, 0, t(t0, 1100));
        assert_eq!(out, ScanOutcome::HangTriggered);
        // Simulate a reset just happened
        core.mark_reset_at(t(t0, 1100));
        // Immediately another stall on a fresh core — clock bumps then
        // re-stalls. Must be suppressed by rate-limit.
        core.scan_step(305, 0, t(t0, 1200));
        for i in 13..=22 {
            core.scan_step(305, 0, t(t0, i * 100));
        }
        let out = core.scan_step(305, 0, t(t0, 2300));
        assert_eq!(out, ScanOutcome::ResetSuppressedByRateLimit);
    }

    #[test]
    fn rate_limit_allows_reset_after_window() {
        let mut core = Core::new(1000).with_max_sm_clock(2775);
        let t0 = Instant::now();
        core.mark_reset_at(t0);

        // Advance past the 5 min window
        let later = t0 + RESET_RATE_LIMIT + Duration::from_secs(1);

        // 300 MHz stuck: plausibly hung path
        core.scan_step(300, 0, later);
        for i in 1..=10 {
            core.scan_step(300, 0, later + Duration::from_millis(i * 100));
        }
        let out = core.scan_step(300, 0, later + Duration::from_millis(1200));
        assert_eq!(out, ScanOutcome::HangTriggered);
    }

    // ── XID ────────────────────────────────────────────────────────────

    #[test]
    fn fatal_xid_triggers_reset() {
        let core = Core::new(30_000);
        let now = Instant::now();
        for code in FATAL_XID_CODES {
            assert!(
                core.should_reset_on_xid(*code, now),
                "XID {code} should be fatal"
            );
        }
    }

    #[test]
    fn non_fatal_xid_does_not_trigger_reset() {
        let core = Core::new(30_000);
        let now = Instant::now();
        for code in [13, 31, 32, 38, 61, 62, 63] {
            assert!(
                !core.should_reset_on_xid(code, now),
                "XID {code} should not reset"
            );
        }
    }

    #[test]
    fn fatal_xid_rate_limited() {
        let mut core = Core::new(30_000);
        let t0 = Instant::now();
        core.mark_reset_at(t0);
        assert!(
            !core.should_reset_on_xid(43, t0 + Duration::from_secs(60)),
            "fatal XID within rate-limit must not trigger reset"
        );
        assert!(
            core.should_reset_on_xid(43, t0 + RESET_RATE_LIMIT + Duration::from_secs(1)),
            "fatal XID after window must be allowed"
        );
    }

    #[test]
    fn reset_in_progress_blocks_further_scans() {
        let mut core = Core::new(1000);
        core.reset_in_progress = true;
        let out = core.scan_step(1800, 0, Instant::now());
        assert_eq!(out, ScanOutcome::ResetInProgress);
    }
}
