//! Microbench: UDS + MessagePack frame encoding/decoding.
//!
//! Measures the pure serialization cost; the actual push_snapshot round-trip
//! over UDS is dominated by the kernel socket path which needs a running
//! server to test meaningfully. This bench answers: "is MsgPack actually
//! smaller and faster than JSON for our SystemSnapshot?"

use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soullink_core::{
    ipc::{decode_frame, encode_frame},
    SystemSnapshot,
};
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

fn bench_ipc(c: &mut Criterion) {
    let mut g = c.benchmark_group("ipc_uds");

    let snap = sample_snapshot();
    let framed = encode_frame(&snap).unwrap();

    g.bench_function("encode_frame", |b| {
        b.iter(|| encode_frame(black_box(&snap)).unwrap());
    });

    g.bench_function("decode_frame", |b| {
        b.iter(|| {
            let _: SystemSnapshot = decode_frame(black_box(&framed)).unwrap();
        });
    });

    g.bench_function("roundtrip", |b| {
        b.iter(|| {
            let f = encode_frame(black_box(&snap)).unwrap();
            let _: SystemSnapshot = decode_frame(&f).unwrap();
        });
    });

    // Report the actual frame size for reference
    println!("\n--- IPC frame sizes ---");
    println!("  MessagePack frame: {} bytes", framed.len());
    let json = serde_json::to_vec(&snap).unwrap();
    println!("  JSON (equivalent): {} bytes", json.len());
    println!("  ratio: {:.2}×", json.len() as f64 / framed.len() as f64);

    g.finish();
}

criterion_group!(benches, bench_ipc);
criterion_main!(benches);
