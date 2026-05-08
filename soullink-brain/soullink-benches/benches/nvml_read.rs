//! Microbench: NVML read paths.
//!
//! # Two modes
//!
//! 1. **Mock mode (always runs)** — benchmarks the `NvmlBridge::mock()` read
//!    paths: Arc dispatch + Mutex lock + struct-field read. Gives a lower
//!    bound on the overhead we add above raw NVML.
//!
//! 2. **Real mode (opt-in with `NVML_BENCH_REAL=1`)** — benchmarks real NVML
//!    FFI. Only meaningful on a host with a working NVIDIA driver. Measures
//!    the FFI overhead we actually pay per call.
//!
//! The gap between (1) and (2) is what would be reclaimed by caching the
//! `Device` handle in `OnceLock` (Phase 2c conditional optimisation). If
//! the gap is < 10 % on a full HomeoSync tick (8 reads in sequence), we
//! skip the cache refactor as premature optimisation.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soullink_nvml::NvmlBridge;

/// Simulate one HomeoSync tick — eight reads that the actor performs
/// every 5 seconds in production.
fn homeo_tick(b: &NvmlBridge) {
    let _ = black_box(b.gpu_temp());
    let _ = black_box(b.gpu_util());
    let _ = black_box(b.vram_used_mib());
    let _ = black_box(b.vram_total_mib());
    let _ = black_box(b.power_w());
    let _ = black_box(b.fan_speed());
    let _ = black_box(b.clock_sm_mhz());
    let _ = black_box(b.clock_mem_mhz());
}

fn bench_mock(c: &mut Criterion) {
    let mut g = c.benchmark_group("nvml_read/mock");

    let bridge = NvmlBridge::mock();

    g.bench_function("gpu_temp",       |b| b.iter(|| bridge.gpu_temp()));
    g.bench_function("gpu_util",       |b| b.iter(|| bridge.gpu_util()));
    g.bench_function("power_w",        |b| b.iter(|| bridge.power_w()));
    g.bench_function("clock_sm_mhz",   |b| b.iter(|| bridge.clock_sm_mhz()));
    g.bench_function("homeosync_tick", |b| b.iter(|| homeo_tick(&bridge)));

    g.finish();
}

fn bench_real(c: &mut Criterion) {
    if std::env::var("NVML_BENCH_REAL").ok().as_deref() != Some("1") {
        // Skipped silently — mock mode covers CI.
        return;
    }

    let bridge = match NvmlBridge::init(0) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("nvml_read/real: NVML init failed ({e}); skipping real benches");
            return;
        }
    };

    let mut g = c.benchmark_group("nvml_read/real");

    g.bench_function("gpu_temp",       |b| b.iter(|| bridge.gpu_temp()));
    g.bench_function("gpu_util",       |b| b.iter(|| bridge.gpu_util()));
    g.bench_function("power_w",        |b| b.iter(|| bridge.power_w()));
    g.bench_function("clock_sm_mhz",   |b| b.iter(|| bridge.clock_sm_mhz()));
    // The full tick is the decision metric for the OnceLock cache
    g.bench_function("homeosync_tick", |b| b.iter(|| homeo_tick(&bridge)));

    g.finish();
}

criterion_group!(benches, bench_mock, bench_real);
criterion_main!(benches);
