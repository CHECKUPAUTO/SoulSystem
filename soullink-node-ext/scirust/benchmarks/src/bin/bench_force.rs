// ==========================================================================
// bench_force.rs — SoulSystem V2 Hardware Benchmark
// ==========================================================================

use std::time::Instant;
use rand::Rng;

const RATIO: f64 = 1.5;
const START_N: usize = 64;
const MAX_N: usize = 100_000;
const WARMUP: usize = 10;
const ROUNDS: usize = 100;
const FMA_CHAIN_DEPTH: usize = 16;

fn geometric_progression(start: usize, ratio: f64, max: usize) -> Vec<usize> {
    let mut vals = Vec::new();
    let mut n = start as f64;
    while (n as usize) <= max {
        vals.push(n as usize);
        n *= ratio;
    }
    vals
}

fn cpu_fma_chain(a: &[f64], b: &[f64], c: &[f64], out: &mut [f64]) {
    let n = a.len();
    assert!(n == b.len() && n == c.len() && n == out.len());

    #[cfg(feature = "simd-avx512")]
    {
        unsafe { cpu_fma_chain_avx512(a, b, c, out, n) }
        return;
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        #[cfg(target_arch = "x86_64")]
        #[allow(unused_imports)]
        use core::arch::x86_64::*;

        unsafe {
            if std::arch::is_x86_feature_detected!("avx2") {
                cpu_fma_chain_avx2(a, b, c, out, n);
                return;
            }
            if std::arch::is_x86_feature_detected!("sse2") {
                cpu_fma_chain_sse2(a, b, c, out, n);
                return;
            }
        }
    }

    cpu_fma_chain_scalar(a, b, c, out, n);
}

#[cfg(feature = "simd-avx512")]
#[target_feature(enable = "avx512f")]
unsafe fn cpu_fma_chain_avx512(a: &[f64], b: &[f64], c: &[f64], out: &mut [f64], n: usize) {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    let mut i = 0;
    while i + 8 <= n {
        let mut va = _mm512_loadu_pd(a.as_ptr().add(i));
        let vb = _mm512_loadu_pd(b.as_ptr().add(i));
        let mut vc = _mm512_loadu_pd(c.as_ptr().add(i));

        let mut v = _mm512_fmadd_pd(va, vb, vc);
        v = _mm512_fmadd_pd(v, va, vb);
        v = _mm512_fmadd_pd(v, vb, vc);
        v = _mm512_fmadd_pd(v, va, vb);
        v = _mm512_fmadd_pd(v, vb, vc);

        for _ in 0..(FMA_CHAIN_DEPTH - 5) {
            v = _mm512_fmadd_pd(v, va, vb);
            core::mem::swap(&mut va, &mut vb);
        }

        _mm512_storeu_pd(out.as_mut_ptr().add(i), v);
        i += 8;
    }

    while i < n {
        let mut v = a[i] * b[i] + c[i];
        for _ in 0..FMA_CHAIN_DEPTH - 1 {
            v = v * a[i] + b[i];
        }
        out[i] = v;
        i += 1;
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn cpu_fma_chain_avx2(a: &[f64], b: &[f64], c: &[f64], out: &mut [f64], n: usize) {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    let mut i = 0;
    while i + 4 <= n {
        let va = _mm256_loadu_pd(a.as_ptr().add(i));
        let vb = _mm256_loadu_pd(b.as_ptr().add(i));
        let vc = _mm256_loadu_pd(c.as_ptr().add(i));

        let mut v = _mm256_fmadd_pd(va, vb, vc);
        for _ in 1..FMA_CHAIN_DEPTH {
            v = _mm256_fmadd_pd(v, va, vb);
        }

        _mm256_storeu_pd(out.as_mut_ptr().add(i), v);
        i += 4;
    }

    while i < n {
        let mut v = a[i] * b[i] + c[i];
        for _ in 1..FMA_CHAIN_DEPTH {
            v = v * a[i] + b[i];
        }
        out[i] = v;
        i += 1;
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse2")]
unsafe fn cpu_fma_chain_sse2(a: &[f64], b: &[f64], c: &[f64], out: &mut [f64], n: usize) {
    #[cfg(target_arch = "x86_64")]
    use core::arch::x86_64::*;

    let mut i = 0;
    while i + 2 <= n {
        let va = _mm_loadu_pd(a.as_ptr().add(i));
        let vb = _mm_loadu_pd(b.as_ptr().add(i));
        let vc = _mm_loadu_pd(c.as_ptr().add(i));

        let mut v = _mm_add_pd(_mm_mul_pd(va, vb), vc);
        for _ in 1..FMA_CHAIN_DEPTH {
            v = _mm_add_pd(_mm_mul_pd(v, va), vb);
        }

        _mm_storeu_pd(out.as_mut_ptr().add(i), v);
        i += 2;
    }

    while i < n {
        let mut v = a[i] * b[i] + c[i];
        for _ in 1..FMA_CHAIN_DEPTH {
            v = v * a[i] + b[i];
        }
        out[i] = v;
        i += 1;
    }
}

fn cpu_fma_chain_scalar(a: &[f64], b: &[f64], c: &[f64], out: &mut [f64], n: usize) {
    for i in 0..n {
        let mut v = a[i] * b[i] + c[i];
        for _ in 1..FMA_CHAIN_DEPTH {
            v = v * a[i] + b[i];
        }
        out[i] = v;
    }
}

#[allow(unused_variables)]
fn gpu_fma_chain(
    a: &[f64],
    b: &[f64],
    c: &[f64],
    out: &mut [f64],
    a_f32: &[f32],
    b_f32: &[f32],
    c_f32: &[f32],
    out_f32: &mut [f32],
) {
    #[cfg(any(feature = "gpu-cuda", feature = "gpu-uma"))]
    {
        gpu_fma_chain_impl(a, b, c, out, a_f32, b_f32, c_f32, out_f32);
        return;
    }

    #[cfg(not(any(feature = "gpu-cuda", feature = "gpu-uma")))]
    {
        let n = a.len();
        scirust_gpu::dispatch::gpu_or_cpu(
            unsafe { std::slice::from_raw_parts_mut(out.as_mut_ptr() as *mut f32, n) },
            |_chunk| {},
        );
        cpu_fma_chain_scalar(a, b, c, out, n);
    }
}

#[cfg(any(feature = "gpu-cuda", feature = "gpu-uma"))]
#[allow(unused_variables)]
fn gpu_fma_chain_impl(
    a: &[f64],
    b: &[f64],
    c: &[f64],
    out: &mut [f64],
    a_f32: &[f32],
    b_f32: &[f32],
    c_f32: &[f32],
    out_f32: &mut [f32],
) {
    let n = a.len();
    let chain = FMA_CHAIN_DEPTH;
    let base_ptr = out_f32.as_ptr() as usize;

    scirust_gpu::dispatch::gpu_or_cpu(out_f32, |chunk| {
        let chunk_ptr = chunk.as_ptr() as usize;
        let start = (chunk_ptr - base_ptr) / std::mem::size_of::<f32>();
        let local_n = chunk.len();
        for j in 0..local_n {
            let idx = start + j;
            if idx >= n {
                break;
            }
            let mut v = a_f32[idx] * b_f32[idx] + c_f32[idx];
            for _ in 1..chain {
                v = v * a_f32[idx] + b_f32[idx];
            }
            chunk[j] = v;
        }
    });

    for (i, &v) in out_f32.iter().enumerate() {
        out[i] = v as f64;
    }
}



fn bench_cpu(a: &[f64], b: &[f64], c: &[f64], out: &mut [f64]) -> f64 {
    for _ in 0..WARMUP {
        cpu_fma_chain(a, b, c, out);
    }
    let start = Instant::now();
    for _ in 0..ROUNDS {
        cpu_fma_chain(a, b, c, out);
    }
    let elapsed = start.elapsed();
    elapsed.as_secs_f64() * 1_000_000.0 / ROUNDS as f64
}

fn bench_gpu(a: &[f64], b: &[f64], c: &[f64], out: &mut [f64]) -> f64 {
    let n = a.len();

    // Pre-allocate f32 conversion buffers once and reuse across all rounds
    // to avoid allocation noise in benchmark measurements.
    let mut a_f32: Vec<f32> = vec![0.0f32; n];
    let mut b_f32: Vec<f32> = vec![0.0f32; n];
    let mut c_f32: Vec<f32> = vec![0.0f32; n];
    let mut out_f32: Vec<f32> = vec![0.0f32; n];

    for (i, &x) in a.iter().enumerate() { a_f32[i] = x as f32; }
    for (i, &x) in b.iter().enumerate() { b_f32[i] = x as f32; }
    for (i, &x) in c.iter().enumerate() { c_f32[i] = x as f32; }

    for _ in 0..WARMUP {
        gpu_fma_chain(a, b, c, out, &a_f32, &b_f32, &c_f32, &mut out_f32);
    }
    let start = Instant::now();
    for _ in 0..ROUNDS {
        gpu_fma_chain(a, b, c, out, &a_f32, &b_f32, &c_f32, &mut out_f32);
    }
    let elapsed = start.elapsed();
    elapsed.as_secs_f64() * 1_000_000.0 / ROUNDS as f64
}



fn print_header() {
    println!();
    println!("  {:>8}  {:>12}  {:>12}  {:>8}  {:>10}", "N", "CPU (\u{00b5}s)", "GPU (\u{00b5}s)", "Winner", "\u{0394} (\u{00b5}s)");
    println!("  {:>8}  {:>12}  {:>12}  {:>8}  {:>10}", "--------", "----------", "----------", "-------", "--------");
}

fn print_row(n: usize, cpu_us: f64, gpu_us: f64) {
    let (winner, delta) = if cpu_us <= gpu_us {
        ("CPU", gpu_us - cpu_us)
    } else {
        ("GPU", cpu_us - gpu_us)
    };
    println!("  {:>8}  {:>12.2}  {:>12.2}  {:>8}  {:>10.2}", n, cpu_us, gpu_us, winner, delta);
}

fn find_threshold(results: &[(usize, f64, f64)], margin_us: f64, consecutive: usize) -> Option<usize> {
    let mut streak = 0usize;
    for &(n, cpu, gpu) in results {
        if gpu < cpu - margin_us {
            streak += 1;
            if streak >= consecutive {
                return Some(n);
            }
        } else {
            streak = 0;
        }
    }
    None
}

fn main() {
    println!("\u{2550}\u{2550}\u{2550} SoulSystem V2 \u{2014} Force Field FMA Benchmark \u{2550}\u{2550}\u{2550}\n");
    println!("AVX-512 feature:  {}", cfg!(feature = "simd-avx512"));
    println!("GPU-CUDA feature: {}", cfg!(feature = "gpu-cuda"));
    println!("GPU-UMA feature:  {}", cfg!(feature = "gpu-uma"));
    println!("FMA chain depth:  {} FMAs/element", FMA_CHAIN_DEPTH);
    println!("Rounds/dim:       {}", ROUNDS);
    println!();

    let dims = geometric_progression(START_N, RATIO, MAX_N);
    println!("Testing {} dimensions from N={} to N={}", dims.len(), dims[0], dims.last().unwrap());
    println!();

    let max_n = *dims.last().unwrap();
    let mut rng = rand::thread_rng();

    let a: Vec<f64> = (0..max_n).map(|_| rng.gen::<f64>() * 2.0 - 1.0).collect();
    let b: Vec<f64> = (0..max_n).map(|_| rng.gen::<f64>() * 2.0 - 1.0).collect();
    let c: Vec<f64> = (0..max_n).map(|_| rng.gen::<f64>() * 2.0 - 1.0).collect();
    let mut out: Vec<f64> = vec![0.0f64; max_n];

    let check_n = 256.min(max_n);
    cpu_fma_chain(&a[..check_n], &b[..check_n], &c[..check_n], &mut out[..check_n]);
    let mut gpu_out = vec![0.0f64; check_n];

    // Pre-allocate f32 buffers for the GPU correctness check
    let a_f32: Vec<f32> = (0..check_n).map(|i| a[i] as f32).collect();
    let b_f32: Vec<f32> = (0..check_n).map(|i| b[i] as f32).collect();
    let c_f32: Vec<f32> = (0..check_n).map(|i| c[i] as f32).collect();
    let mut out_f32: Vec<f32> = vec![0.0f32; check_n];

    gpu_fma_chain(&a[..check_n], &b[..check_n], &c[..check_n], &mut gpu_out[..check_n],
             &a_f32, &b_f32, &c_f32, &mut out_f32);
    
    let max_diff = out[..check_n].iter().zip(gpu_out.iter()).map(|(x, y)| (x - y).abs()).fold(0.0f64, f64::max);
    println!("Correctness check (N={}): max |cpu - gpu| = {:.2e}", check_n, max_diff);
    if max_diff > 1e-3 {
        eprintln!("\u{26a0} Warning: CPU and GPU results diverge (max_diff={:.4})", max_diff);
    }
    println!();

    print_header();
    let mut results: Vec<(usize, f64, f64)> = Vec::with_capacity(dims.len());

    for &n in &dims {
        let cpu_us = bench_cpu(&a[..n], &b[..n], &c[..n], &mut out[..n]);
        let gpu_us = bench_gpu(&a[..n], &b[..n], &c[..n], &mut out[..n]);

        results.push((n, cpu_us, gpu_us));
        print_row(n, cpu_us, gpu_us);
    }

    let threshold = find_threshold(&results, 5.0, 3);

    println!();
    println!("\u{2550}\u{2550}\u{2550} Result \u{2550}\u{2550}\u{2550}");
    match threshold {
        Some(n) => println!("MATRIX_GPU_THRESHOLD = {}  (GPU durably wins at N={} and above)", n, n),
        None => {
            if let Some(&(_, last_cpu, last_gpu)) = results.last() {
                if last_gpu < last_cpu {
                    println!("MATRIX_GPU_THRESHOLD = {}  (GPU wins at max tested N={})", *dims.last().unwrap(), dims.last().unwrap());
                } else {
                    println!("MATRIX_GPU_THRESHOLD > {}  (CPU still faster at max tested N={}, CPU={:.1}\u{00b5}s GPU={:.1}\u{00b5}s)", max_n, max_n, last_cpu, last_gpu);
                }
            }
        }
    }
    println!();
}
