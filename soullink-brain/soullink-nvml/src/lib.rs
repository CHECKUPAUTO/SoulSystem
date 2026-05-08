//! NVML FFI bridge — zero subprocess, direct calls to `libnvidia-ml.so.1`.
//!
//! # Phase 1 security
//!
//! `set_power_limit_w()` replaces the old `nvidia-smi -pl N` subprocess. No
//! shell, no subprocess, no injection surface.
//!
//! # Phase 1-p1a hotfix
//!
//! `throttle_reasons()` returns `current_throttle_reasons().bits()`. It is
//! an **informational** bitmask and must never be used as an XID error
//! signal — see `soullink-guardian/src/emergency.rs` for the post-mortem.
//!
//! # Phase 2 testability
//!
//! `NvmlBridge` gains a `mock()` constructor behind `cfg(any(test, feature =
//! "mock"))`. Guardian actors keep their `Arc<NvmlBridge>` signature and
//! tests configure the mock via `mock_mut`. Real NVML reads are unchanged.

use std::sync::Arc;

use nvml_wrapper::{
    enum_wrappers::device::{Clock, TemperatureSensor},
    Nvml,
};
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
pub enum NvmlError {
    #[error("NVML init failed: {0}")]
    Init(String),
    #[error("device {0} not found")]
    DeviceNotFound(u32),
    #[error("NVML call failed: {0}")]
    Call(String),
    #[error("power-limit {req} W out of range [{min}, {max}]")]
    PowerLimitOutOfRange { req: u32, min: u32, max: u32 },
}

enum BridgeImpl {
    Real(RealBridge),
    #[cfg(any(test, feature = "mock"))]
    Mock(Arc<std::sync::Mutex<MockState>>),
}

/// Real NVML-backed bridge with a cached `Device` handle.
///
/// # Safety rationale for the lifetime transmute
///
/// `nvml_wrapper::Device<'a>` borrows from a `Nvml`. We want to cache the
/// `Device` to avoid re-issuing `device_by_index()` on every read (measured
/// cost on production T430: ~95 µs per tick of 8 reads → ~775 µs/tick).
///
/// The lifetime on `Device<'_>` exists to prevent the `Nvml` from being
/// dropped while handles are outstanding. In our case the `Nvml` is held
/// inside the same struct via an `Arc` and both fields are dropped together
/// when `RealBridge` drops — so the `Device` cannot outlive its `Nvml`.
///
/// We store the `Device` with a synthetic `'static` bound, but the `Device`
/// is **never** handed out to callers by reference — all reads return owned
/// values (`u32`, `f32`, etc.) taken from the cached handle. The `Device`
/// object itself stays buried inside `RealBridge`.
///
/// The two invariants that make this sound:
///   1. `nvml: Arc<Nvml>` outlives the cached `Device` (same struct, drop
///      order guaranteed by field order: `device` before `nvml`).
///   2. `Device<'_>` is `Send + Sync` per nvml-wrapper docs — no external
///      aliasing concern.
struct RealBridge {
    /// Cached Device handle. MUST be dropped before `nvml`.
    ///
    /// Wrapped in `Option` so `Drop` can explicitly take it before the Arc
    /// is released (defensive, even though field order already guarantees
    /// correct ordering).
    device: std::mem::ManuallyDrop<nvml_wrapper::Device<'static>>,
    nvml:   Arc<Nvml>,
    device_index: u32,
}

impl RealBridge {
    fn new(nvml: Arc<Nvml>, device_index: u32) -> Result<Self, NvmlError> {
        let dev_borrowed = nvml
            .device_by_index(device_index)
            .map_err(|_| NvmlError::DeviceNotFound(device_index))?;

        // SAFETY: see RealBridge doc. The Arc<Nvml> is held in the same
        // struct; Rust drop order (fields in declaration order) ensures
        // `device` drops before `nvml`. The synthetic 'static is never
        // exposed.
        let dev_static: nvml_wrapper::Device<'static> = unsafe {
            std::mem::transmute::<nvml_wrapper::Device<'_>, nvml_wrapper::Device<'static>>(
                dev_borrowed,
            )
        };

        Ok(Self {
            device: std::mem::ManuallyDrop::new(dev_static),
            nvml,
            device_index,
        })
    }

    #[inline]
    fn dev(&self) -> &nvml_wrapper::Device<'static> {
        &self.device
    }
}

impl Drop for RealBridge {
    fn drop(&mut self) {
        // Explicitly drop the Device first (releases its FFI handle), then
        // the Arc<Nvml>. Without this, ManuallyDrop would leak the handle.
        unsafe { std::mem::ManuallyDrop::drop(&mut self.device); }
        // `self.nvml` drops automatically after this function returns.
    }
}

/// Configurable mock state — only compiled in tests or with `feature = "mock"`.
#[cfg(any(test, feature = "mock"))]
#[derive(Debug, Clone)]
pub struct MockState {
    pub gpu_temp_c:       f32,
    pub gpu_util_pct:     u32,
    pub vram_used_mib:    u64,
    pub vram_total_mib:   u64,
    pub power_w:          f32,
    pub fan_speed_pct:    u32,
    pub clock_sm_mhz:     u32,
    pub clock_mem_mhz:    u32,
    pub power_limit_w:    f32,
    pub throttle_reasons: u64,
    pub next_set_error:   Option<String>,
    pub set_power_calls:  Vec<u32>,
    pub process_count:     u32,
    pub ecc_corrected:     u64,
    pub ecc_uncorrected:  u64,
    pub persistence_enabled: bool,
    pub pci_rx_kb:        u64,
    pub pci_tx_kb:        u64,
    pub board_part:       String,
    pub serial:           String,
    pub uuid:             String,
}

#[cfg(any(test, feature = "mock"))]
impl Default for MockState {
    fn default() -> Self {
        Self {
            gpu_temp_c:       40.0,
            gpu_util_pct:     0,
            vram_used_mib:    512,
            vram_total_mib:   8188,
            power_w:          20.0,
            fan_speed_pct:    30,
            clock_sm_mhz:     1800,
            clock_mem_mhz:    8000,
            power_limit_w:    115.0,
            throttle_reasons: 0,
            next_set_error:   None,
            set_power_calls:  Vec::new(),
            process_count:     0,
            ecc_corrected:     0,
            ecc_uncorrected:   0,
            persistence_enabled: true,
            pci_rx_kb:         0,
            pci_tx_kb:         0,
            board_part:        "RTX 4060".to_string(),
            serial:            "MOCK-SN-001".to_string(),
            uuid:              "GPU-MOCK-001".to_string(),
        }
    }
}

#[derive(Clone)]
pub struct NvmlBridge {
    inner:            Arc<BridgeImpl>,
    device_index:     u32,
    power_min_mw:     u32,
    power_max_mw:     u32,
    power_default_mw: u32,
}

unsafe impl Send for NvmlBridge {}
unsafe impl Sync for NvmlBridge {}

impl NvmlBridge {
    pub fn init(device_index: u32) -> Result<Self, NvmlError> {
        let nvml = Nvml::init().map_err(|e| NvmlError::Init(e.to_string()))?;
        let nvml_arc = Arc::new(nvml);

        let (min_mw, max_mw, default_mw) = match nvml_arc.device_by_index(device_index) {
            Ok(d) => {
                let constraints = d
                    .power_management_limit_constraints()
                    .map_err(|e| NvmlError::Call(format!("power constraints: {e}")))?;
                let default_mw = d
                    .power_management_limit_default()
                    .unwrap_or(constraints.max_limit);
                (constraints.min_limit, constraints.max_limit, default_mw)
            }
            Err(_) => return Err(NvmlError::DeviceNotFound(device_index)),
        };

        let real = RealBridge::new(nvml_arc, device_index)?;

        info!(
            device_index,
            power_min_w = min_mw / 1000,
            power_max_w = max_mw / 1000,
            power_default_w = default_mw / 1000,
            "NVML initialised — direct GPU FFI with cached Device handle"
        );

        Ok(Self {
            inner: Arc::new(BridgeImpl::Real(real)),
            device_index,
            power_min_mw: min_mw,
            power_max_mw: max_mw,
            power_default_mw: default_mw,
        })
    }

    /// Mock bridge with default state. Only available under test or `feature = "mock"`.
    #[cfg(any(test, feature = "mock"))]
    pub fn mock() -> Self {
        Self {
            inner: Arc::new(BridgeImpl::Mock(Arc::new(std::sync::Mutex::new(
                MockState::default(),
            )))),
            device_index:     0,
            power_min_mw:     70_000,
            power_max_mw:     115_000,
            power_default_mw: 115_000,
        }
    }

    #[cfg(any(test, feature = "mock"))]
    pub fn mock_mut<F: FnOnce(&mut MockState)>(&self, f: F) {
        match &*self.inner {
            BridgeImpl::Mock(s) => {
                let mut guard = s.lock().expect("mock state poisoned");
                f(&mut guard);
            }
            BridgeImpl::Real(_) => panic!("mock_mut called on real NvmlBridge"),
        }
    }

    #[cfg(any(test, feature = "mock"))]
    pub fn mock_snapshot(&self) -> MockState {
        match &*self.inner {
            BridgeImpl::Mock(s) => s.lock().expect("mock state poisoned").clone(),
            BridgeImpl::Real(_) => panic!("mock_snapshot called on real NvmlBridge"),
        }
    }

    #[cfg(any(test, feature = "mock"))]
    fn mock_read<T>(&self, f: impl FnOnce(&MockState) -> T) -> Option<T> {
        match &*self.inner {
            BridgeImpl::Mock(s) => Some(f(&s.lock().expect("mock state poisoned"))),
            BridgeImpl::Real(_) => None,
        }
    }

    #[inline]
    fn dev(&self) -> Option<&nvml_wrapper::Device<'static>> {
        match &*self.inner {
            BridgeImpl::Real(r) => Some(r.dev()),
            #[cfg(any(test, feature = "mock"))]
            BridgeImpl::Mock(_) => None,
        }
    }

    pub fn gpu_temp(&self) -> f32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.gpu_temp_c) { return v; }
        self.dev()
            .and_then(|d| d.temperature(TemperatureSensor::Gpu).ok())
            .map(|t| t as f32).unwrap_or(0.0)
    }

    pub fn gpu_util(&self) -> u32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.gpu_util_pct) { return v; }
        self.dev().and_then(|d| d.utilization_rates().ok()).map(|u| u.gpu).unwrap_or(0)
    }

    pub fn vram_used_mib(&self) -> u64 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.vram_used_mib) { return v; }
        self.dev().and_then(|d| d.memory_info().ok()).map(|m| m.used / 1024 / 1024).unwrap_or(0)
    }

    pub fn vram_total_mib(&self) -> u64 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.vram_total_mib) { return v; }
        self.dev().and_then(|d| d.memory_info().ok()).map(|m| m.total / 1024 / 1024).unwrap_or(8188)
    }

    pub fn power_w(&self) -> f32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.power_w) { return v; }
        self.dev().and_then(|d| d.power_usage().ok()).map(|p| p as f32 / 1000.0).unwrap_or(0.0)
    }

    pub fn fan_speed(&self) -> u32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.fan_speed_pct) { return v; }
        self.dev().and_then(|d| d.fan_speed(0u32).ok()).unwrap_or(0)
    }

    pub fn clock_sm_mhz(&self) -> u32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.clock_sm_mhz) { return v; }
        self.dev().and_then(|d| d.clock_info(Clock::SM).ok()).unwrap_or(0)
    }

    /// Maximum SM boost clock reported by the GPU. Used by the hang
    /// detector to distinguish "saturated at boost" (legitimate) from
    /// "stuck at low clock" (actual hang). Returns 0 if NVML can't tell.
    pub fn max_clock_sm_mhz(&self) -> u32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.clock_sm_mhz.saturating_add(100)) {
            return v;
        }
        self.dev()
            .and_then(|d| d.max_clock_info(Clock::SM).ok())
            .unwrap_or(0)
    }

    pub fn clock_mem_mhz(&self) -> u32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.clock_mem_mhz) { return v; }
        self.dev().and_then(|d| d.clock_info(Clock::Memory).ok()).unwrap_or(0)
    }

    pub fn power_limit_w(&self) -> f32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.power_limit_w) { return v; }
        self.dev().and_then(|d| d.enforced_power_limit().ok()).map(|mw| mw as f32 / 1000.0).unwrap_or(0.0)
    }

    pub fn throttle_reasons(&self) -> u64 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.throttle_reasons) { return v; }
        self.dev().and_then(|d| d.current_throttle_reasons().ok()).map(|r| r.bits()).unwrap_or(0)
    }

    pub fn set_power_limit_w(&self, watts: u32) -> Result<(), NvmlError> {
        let mw = watts.saturating_mul(1000);
        if mw < self.power_min_mw || mw > self.power_max_mw {
            return Err(NvmlError::PowerLimitOutOfRange {
                req: watts,
                min: self.power_min_mw / 1000,
                max: self.power_max_mw / 1000,
            });
        }
        match &*self.inner {
            BridgeImpl::Real(real) => {
                // Writes are rare (one per throttle change, cooldown-gated);
                // we pay the device_by_index cost here rather than risk
                // aliasing the cached Device. The Arc<Nvml> is the same one
                // backing the cached Device.
                let dev = real.nvml.device_by_index(real.device_index)
                    .map_err(|e| NvmlError::Call(e.to_string()))?;
                let mut dev_mut = dev;
                dev_mut.set_power_management_limit(mw)
                    .map_err(|e| NvmlError::Call(format!("set_power_limit({watts}W): {e}")))?;
                info!(watts, "NVML power-limit applied");
                Ok(())
            }
            #[cfg(any(test, feature = "mock"))]
            BridgeImpl::Mock(state) => {
                let mut s = state.lock().expect("mock state poisoned");
                s.set_power_calls.push(watts);
                if let Some(msg) = s.next_set_error.take() {
                    return Err(NvmlError::Call(msg));
                }
                s.power_limit_w = watts as f32;
                Ok(())
            }
        }
    }

    pub fn reset_power_limit(&self) -> Result<(), NvmlError> {
        let w = self.power_default_mw / 1000;
        self.set_power_limit_w(w)
    }

    pub fn power_range_w(&self) -> (u32, u32) {
        (self.power_min_mw / 1000, self.power_max_mw / 1000)
    }

    pub fn power_default_w(&self) -> u32 {
        self.power_default_mw / 1000
    }

    /// Number of GPU compute processes currently running.
    pub fn process_count(&self) -> u32 {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.process_count) { return v; }
        self.dev()
            .and_then(|d| d.running_compute_processes_count().ok())
            .unwrap_or(0)
    }

    /// ECC errors (corrected + uncorrected) since boot.
    pub fn ecc_errors(&self) -> (u64, u64) {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| (s.ecc_corrected, s.ecc_uncorrected)) { return v; }
        use nvml_wrapper::enum_wrappers::device::{MemoryError, EccCounter};
        let corrected = self.dev()
            .and_then(|d| d.total_ecc_errors(MemoryError::Corrected, EccCounter::Volatile).ok())
            .unwrap_or(0);
        let uncorrected = self.dev()
            .and_then(|d| d.total_ecc_errors(MemoryError::Uncorrected, EccCounter::Volatile).ok())
            .unwrap_or(0);
        (corrected, uncorrected)
    }

    /// Persistence mode status (true = enabled).
    pub fn persistence_enabled(&self) -> bool {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.persistence_enabled) { return v; }
        self.dev()
            .and_then(|d| d.is_in_persistent_mode().ok())
            .unwrap_or(false)
    }

    /// PCI bandwidth utilization: (rx_throughput_kb, tx_throughput_kb).
    pub fn pci_throughput_kb(&self) -> (u64, u64) {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| (s.pci_rx_kb, s.pci_tx_kb)) { return v; }
        use nvml_wrapper::enum_wrappers::device::PcieUtilCounter;
        let rx = self.dev()
            .and_then(|d| d.pcie_throughput(PcieUtilCounter::Receive).ok())
            .unwrap_or(0) as u64;
        let tx = self.dev()
            .and_then(|d| d.pcie_throughput(PcieUtilCounter::Send).ok())
            .unwrap_or(0) as u64;
        (rx, tx)
    }

    /// GPU board part number (e.g., "RTX 4060").
    pub fn board_part(&self) -> String {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.board_part.clone()) { return v; }
        self.dev()
            .and_then(|d| d.board_part_number().ok())
            .unwrap_or_else(|| "Unknown".to_string())
    }

    /// GPU serial number.
    pub fn serial(&self) -> String {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.serial.clone()) { return v; }
        self.dev()
            .and_then(|d| d.serial().ok())
            .unwrap_or_else(|| "Unknown".to_string())
    }

    /// GPU UUID.
    pub fn uuid(&self) -> String {
        #[cfg(any(test, feature = "mock"))]
        if let Some(v) = self.mock_read(|s| s.uuid.clone()) { return v; }
        self.dev()
            .and_then(|d| d.uuid().ok())
            .unwrap_or_else(|| "Unknown".to_string())
    }

    /// Driver version string (e.g., "550.54.14").
    pub fn driver_version(&self) -> String {
        self.nvml().sys_driver_version().unwrap_or_else(|_| "Unknown".to_string())
    }

    /// CUDA version string (e.g., "12.4").
    pub fn cuda_version(&self) -> String {
        let v = self.nvml().sys_cuda_driver_version().unwrap_or(0);
        let major = v / 1000;
        let minor = (v % 1000) / 10;
        format!("{}.{}", major, minor)
    }

    /// Access the inner NVML instance (for advanced queries).
    fn nvml(&self) -> &Arc<Nvml> {
        match &*self.inner {
            BridgeImpl::Real(r) => &r.nvml,
            #[cfg(any(test, feature = "mock"))]
            BridgeImpl::Mock(_) => panic!("nvml() called on mock bridge"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_default_values() {
        let b = NvmlBridge::mock();
        assert_eq!(b.gpu_temp(), 40.0);
        assert_eq!(b.gpu_util(), 0);
        assert_eq!(b.vram_total_mib(), 8188);
        assert_eq!(b.power_limit_w(), 115.0);
        assert_eq!(b.throttle_reasons(), 0);
    }

    #[test]
    fn mock_mut_reflects() {
        let b = NvmlBridge::mock();
        b.mock_mut(|s| {
            s.gpu_temp_c = 87.0;
            s.throttle_reasons = 0x20 | 0x40;
        });
        assert_eq!(b.gpu_temp(), 87.0);
        assert_eq!(b.throttle_reasons(), 0x60);
    }

    #[test]
    fn mock_set_power_within_range() {
        let b = NvmlBridge::mock();
        assert!(b.set_power_limit_w(90).is_ok());
        assert_eq!(b.power_limit_w(), 90.0);
        assert_eq!(b.mock_snapshot().set_power_calls, vec![90]);
    }

    #[test]
    fn mock_set_power_out_of_range() {
        let b = NvmlBridge::mock();
        assert!(matches!(b.set_power_limit_w(50).unwrap_err(),
                         NvmlError::PowerLimitOutOfRange { .. }));
        assert!(matches!(b.set_power_limit_w(200).unwrap_err(),
                         NvmlError::PowerLimitOutOfRange { .. }));
    }

    #[test]
    fn mock_set_power_injected_error() {
        let b = NvmlBridge::mock();
        b.mock_mut(|s| s.next_set_error = Some("simulated NVML EPERM".into()));
        assert!(matches!(b.set_power_limit_w(90).unwrap_err(), NvmlError::Call(_)));
        // The call was recorded even though it errored
        assert_eq!(b.mock_snapshot().set_power_calls, vec![90]);
    }

    #[test]
    fn reset_uses_default() {
        let b = NvmlBridge::mock();
        b.mock_mut(|s| s.power_limit_w = 90.0);
        b.reset_power_limit().unwrap();
        assert_eq!(b.power_limit_w(), 115.0);
    }

    #[test]
    fn clone_shares_inner() {
        // NvmlBridge is cheaply cloneable (internal Arc). Spawning actors
        // each with their own clone must not multiply Device handles.
        let b1 = NvmlBridge::mock();
        let b2 = b1.clone();
        b1.mock_mut(|s| s.gpu_temp_c = 99.0);
        assert_eq!(b2.gpu_temp(), 99.0, "clones must share state");
    }
}
