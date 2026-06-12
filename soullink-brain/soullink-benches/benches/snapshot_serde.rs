//! Microbench: `SystemSnapshot` JSON round-trip.
//!
//! This is the guardian→orchestrator hot path (5 s cadence). Baseline for
//! Phase 2c; Phase 3 will compare against MessagePack when IPC migrates to
//! Unix Domain Socket.

use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soullink_core::SystemSnapshot;
use uuid::Uuid;

fn sample_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        ts: Utc::now(),
        correlation_id: Uuid::new_v4(),
        gpu_temp_c: 78.5,
        gpu_util_pct: 87,
        gpu_mem_used_mib: 6211,
        gpu_mem_total_mib: 8188,
        gpu_power_w: 112.3,
        fan_speed_pct: 62,
        pcie_rx_mbs: 1250.0,
        pcie_tx_mbs: 980.0,
        xid_error_count: 0,
        throttle_active: false,
        sm_clock_mhz: 2415,
        mem_clock_mhz: 8000,
        throttle_reasons: 0x20,
        power_limit_w: 115.0,
    }
}

fn bench_serde(c: &mut Criterion) {
    let mut g = c.benchmark_group("snapshot_serde");

    let snap = sample_snapshot();
    let serialized = serde_json::to_string(&snap).unwrap();

    g.bench_function("to_string", |b| {
        b.iter(|| serde_json::to_string(black_box(&snap)).unwrap());
    });

    g.bench_function("to_vec", |b| {
        b.iter(|| serde_json::to_vec(black_box(&snap)).unwrap());
    });

    g.bench_function("from_str", |b| {
        b.iter(|| {
            let _: SystemSnapshot = serde_json::from_str(black_box(&serialized)).unwrap();
        });
    });

    g.bench_function("roundtrip", |b| {
        b.iter(|| {
            let s = serde_json::to_string(black_box(&snap)).unwrap();
            let _: SystemSnapshot = serde_json::from_str(&s).unwrap();
        });
    });

    g.finish();
}

criterion_group!(benches, bench_serde);
criterion_main!(benches);
