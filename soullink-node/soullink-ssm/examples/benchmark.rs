/// Soullink-SSM Benchmark: compare scan methods across sequence lengths.
///
/// Measures throughput for sequential, parallel (Blelloch), CUDA-style,
/// and tiled scan algorithms at various sequence lengths.
///
/// Usage:
///   cargo run --release --example benchmark
///   cargo run --release --example benchmark -- --quick   (shorter run)
use std::time::Instant;

use ndarray::Array1;
use soullink_ssm::cuda::CudaScan;
use soullink_ssm::parallel::{MultiDimScan, ParallelScan, ScanElement};
use soullink_ssm::ssm;
use soullink_ssm::tiled_scan::TiledScan;

/// Timing helper: run a closure `repeat` times and return average duration.
fn bench<F>(label: &str, repeat: usize, f: F)
where
    F: Fn(),
{
    let start = Instant::now();
    for _ in 0..repeat {
        f();
    }
    let dur = start.elapsed();
    let avg_ns = dur.as_nanos() as f64 / repeat as f64;
    println!("  {:<30} {:>8.0} ns/op  ({} runs)", label, avg_ns, repeat);
}

fn bench_1d_scan(seq_len: usize, repeat: usize) {
    let elements: Vec<ScanElement> = (0..seq_len)
        .map(|i| {
            let a = 0.5 + ((i % 5) as f64) * 0.08;
            let b = (i as f64) * 0.1;
            ScanElement::new(a, b)
        })
        .collect();

    let a_seq: Vec<f64> = elements.iter().map(|e| e.a).collect();
    let b_seq: Vec<f64> = elements.iter().map(|e| e.b).collect();

    println!("\n  Sequence length: {}", seq_len);

    // Sequential scan
    bench("sequential (serial)", repeat, || {
        let _ = ParallelScan::selective_scan_1d(&a_seq, &b_seq, 0.0);
    });

    // Parallel Blelloch scan
    let elements_clone = elements.clone();
    bench("parallel (Blelloch)", repeat, || {
        let _ = ParallelScan::parallel_scan(&elements_clone);
    });

    // CUDA-style scan (block=64)
    let elements_clone = elements.clone();
    bench("CUDA-style (block=64)", repeat, || {
        let _ = CudaScan::scan(&elements_clone, 64);
    });

    // CUDA-style scan (block=128)
    let elements_clone = elements.clone();
    bench("CUDA-style (block=128)", repeat, || {
        let _ = CudaScan::scan(&elements_clone, 128);
    });

    // CUDA-style scan (block=256)
    let elements_clone = elements.clone();
    bench("CUDA-style (block=256)", repeat, || {
        let _ = CudaScan::scan(&elements_clone, 256);
    });
}

fn bench_multidim_scan(seq_len: usize, state_dim: usize, repeat: usize) {
    use ndarray::Array2;
    let mut rng = rand::thread_rng();

    let a_mat: Vec<Array2<f64>> = (0..seq_len)
        .map(|_| {
            let mut a = Array2::zeros((state_dim, state_dim));
            for i in 0..state_dim {
                for j in 0..state_dim {
                    a[[i, j]] = rand::Rng::gen_range(&mut rng, -0.2..0.2);
                }
                a[[i, i]] += 0.5;
            }
            a
        })
        .collect();
    let b_vecs: Vec<Array1<f64>> = (0..seq_len)
        .map(|_| Array1::from_shape_fn(state_dim, |_| rand::Rng::gen_range(&mut rng, -0.3..0.3)))
        .collect();
    let h0 = Array1::zeros(state_dim);

    println!("\n  Multi-dim scan: L={}, N={}", seq_len, state_dim);

    // Sequential multi-dim scan
    let a_clone = a_mat.clone();
    let b_clone = b_vecs.clone();
    let h0_clone = h0.clone();
    bench("sequential (multi-dim)", repeat, || {
        let _ = MultiDimScan::scan(&a_clone, &b_clone, &h0_clone);
    });

    // Parallel multi-dim scan
    let a_clone = a_mat.clone();
    let b_clone = b_vecs.clone();
    bench("parallel (multi-dim)", repeat, || {
        let _ = MultiDimScan::parallel_scan(&a_clone, &b_clone);
    });
}

fn bench_tiled_scan(seq_len: usize, state_dim: usize, tile_size: usize, repeat: usize) {
    use ndarray::Array2;
    let mut rng = rand::thread_rng();

    let a_mat: Vec<Array2<f64>> = (0..seq_len)
        .map(|_| {
            let mut a = Array2::zeros((state_dim, state_dim));
            for i in 0..state_dim {
                for j in 0..state_dim {
                    a[[i, j]] = rand::Rng::gen_range(&mut rng, -0.2..0.2);
                }
                a[[i, i]] += 0.5;
            }
            a
        })
        .collect();
    let b_vecs: Vec<Array1<f64>> = (0..seq_len)
        .map(|_| Array1::from_shape_fn(state_dim, |_| rand::Rng::gen_range(&mut rng, -0.3..0.3)))
        .collect();
    let h0 = Array1::zeros(state_dim);

    println!(
        "\n  Tiled scan: L={}, N={}, tile={}",
        seq_len, state_dim, tile_size
    );

    let a_clone = a_mat.clone();
    let b_clone = b_vecs.clone();
    let h0_clone = h0.clone();
    bench(&format!("tiled (tile={})", tile_size), repeat, || {
        let _ = TiledScan::scan(&a_clone, &b_clone, tile_size, &h0_clone);
    });
}

fn bench_fused_scan(seq_len: usize, state_dim: usize, tile_size: usize, repeat: usize) {
    use ndarray::Array2;
    let mut rng = rand::thread_rng();

    let a = Array2::eye(state_dim) * -0.5;
    let b_shared: Vec<Array1<f64>> = (0..seq_len)
        .map(|_| Array1::from_shape_fn(state_dim, |_| rand::Rng::gen_range(&mut rng, -0.2..0.2)))
        .collect();
    let deltas: Vec<f64> = (0..seq_len)
        .map(|_| rand::Rng::gen_range(&mut rng, 0.01..0.05))
        .collect();
    let h0 = Array1::zeros(state_dim);

    println!(
        "\n  Fused scan: L={}, N={}, tile={}",
        seq_len, state_dim, tile_size
    );

    // Non-fused: pre-discretize then tiled scan
    let a_clone = a.clone();
    let b_clone = b_shared.clone();
    let d_clone = deltas.clone();
    let h0_clone = h0.clone();
    bench("non-fused (pre-discretize + tiled)", repeat, || {
        let mut a_bar_seq = Vec::with_capacity(seq_len);
        let mut b_bar_seq = Vec::with_capacity(seq_len);
        for t in 0..seq_len {
            let (ab, bb) = ssm::discretize_zoh(&a_clone, &b_clone[t], d_clone[t]);
            a_bar_seq.push(ab);
            b_bar_seq.push(bb);
        }
        let _ = TiledScan::scan(&a_bar_seq, &b_bar_seq, tile_size, &h0_clone);
    });

    // Fused: discretize within each tile
    let a_clone = a.clone();
    let b_clone = b_shared.clone();
    let d_clone = deltas.clone();
    let h0_clone = h0.clone();
    bench("fused (discretize per tile)", repeat, || {
        let _ = TiledScan::fused_tiled_scan(&a_clone, &b_clone, &d_clone, &h0_clone, tile_size);
    });
}

fn main() {
    println!("═══ Soullink-SSM Benchmark ═══\n");
    println!("All timings are averages of multiple runs.\n");

    // Determine repeat counts based on sequence length
    let quick = std::env::args().any(|a| a == "--quick");

    // 1D scan benchmarks
    println!("─── 1D Scan ───");
    for &len in &[16, 64, 256, 1024] {
        let repeat = if quick { 50 } else { 2000.max(200_000 / len) };
        bench_1d_scan(len, repeat);
    }

    // Multi-dim scan benchmarks
    println!("\n─── Multi-Dim Scan ───");
    for &(len, n) in &[(32, 4), (128, 8), (256, 16)] {
        let repeat = if quick {
            30
        } else {
            1000.max(50_000 / (len * n))
        };
        bench_multidim_scan(len, n, repeat);
    }

    // Tiled scan benchmarks
    println!("\n─── Tiled Scan ───");
    for &(len, n, tile) in &[(64, 4, 8), (128, 8, 16), (256, 8, 32)] {
        let repeat = if quick {
            30
        } else {
            500.max(20_000 / (len * n))
        };
        bench_tiled_scan(len, n, tile, repeat);
    }

    // Fused vs non-fused scan comparison
    println!("\n─── Fused Scan ───");
    for &(len, n, tile) in &[(64, 4, 8), (256, 8, 32)] {
        let repeat = if quick {
            20
        } else {
            300.max(10_000 / (len * n))
        };
        bench_fused_scan(len, n, tile, repeat);
    }

    println!("\n═══ Benchmark complete ═══");
}
