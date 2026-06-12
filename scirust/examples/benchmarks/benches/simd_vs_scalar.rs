//! SIMD vs scalar benchmarks using scirust-simd.
//!
//! Run: cargo bench --package benchmarks --bench simd_vs_scalar

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use scirust_simd::{simd_add_one, scalar_map, scalar_zip, ops};

fn bench_scalar_add_one(c: &mut Criterion) {
    let mut data: Vec<f64> = (0..10_000).map(|i| i as f64).collect();
    c.bench_function("scalar_add_one_10k", |bench| {
        bench.iter(|| {
            for x in data.iter_mut() {
                *x += 1.0;
            }
            black_box(&data[0]);
        })
    });
}

fn bench_simd_add_one(c: &mut Criterion) {
    let mut data: Vec<f64> = (0..10_000).map(|i| i as f64).collect();
    c.bench_function("simd_add_one_10k", |bench| {
        bench.iter(|| {
            simd_add_one(black_box(&mut data));
            black_box(data[0]);
        })
    });
}

fn bench_scalar_mul_f32(c: &mut Criterion) {
    let a: Vec<f32> = (0..10_000).map(|i| i as f32).collect();
    let b: Vec<f32> = (0..10_000).map(|i| (i * 2) as f32).collect();
    let mut out = vec![0.0f32; 10_000];

    c.bench_function("scalar_mul_f32_10k", |bench| {
        bench.iter(|| {
            for i in 0..a.len() {
                out[i] = a[i] * b[i];
            }
            black_box(out[0]);
        })
    });
}

fn bench_simd_mul_f64(c: &mut Criterion) {
    let a: Vec<f64> = (0..10_000).map(|i| i as f64).collect();
    let b: Vec<f64> = (0..10_000).map(|i| (i * 2) as f64).collect();
    let mut out = vec![0.0f64; 10_000];

    c.bench_function("simd_mul_f64_10k", |bench| {
        bench.iter(|| {
            ops::mul_f64(&a, &b, &mut out);
            black_box(out[0]);
        })
    });
}

fn bench_scalar_map(c: &mut Criterion) {
    let input: Vec<f32> = (0..10_000).map(|i| i as f32).collect();
    let mut output = vec![0.0f32; 10_000];

    c.bench_function("scalar_map_sin_10k", |bench| {
        bench.iter(|| {
            scalar_map(&input, &mut output, |x| x.sin());
            black_box(output[0]);
        })
    });
}

fn bench_scalar_zip(c: &mut Criterion) {
    let a: Vec<f32> = (0..10_000).map(|i| i as f32).collect();
    let b: Vec<f32> = (0..10_000).map(|i| (i * 2) as f32).collect();
    let mut out = vec![0.0f32; 10_000];

    c.bench_function("scalar_zip_add_10k", |bench| {
        bench.iter(|| {
            scalar_zip(&a, &b, &mut out, |x, y| x + y);
            black_box(out[0]);
        })
    });
}

criterion_group!(benches, bench_scalar_add_one, bench_simd_add_one, bench_scalar_mul_f32, bench_simd_mul_f64, bench_scalar_map, bench_scalar_zip);
criterion_main!(benches);
