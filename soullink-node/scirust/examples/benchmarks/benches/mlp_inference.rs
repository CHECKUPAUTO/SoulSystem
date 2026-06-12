//! MLP inference benchmark (MNIST-like architecture).
//!
//! Measures forward-pass latency and throughput for an MLP
//! 784 → 256 → 128 → 10 with ReLU activations.
//!
//! Run: cargo bench --package benchmarks

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn mlp_forward_784_256_128_10(batch: usize) {
    let (w1, _b1): (Vec<f32>, Vec<f32>) = (vec![0.01; 784 * 256], vec![0.0; 256]);
    let (w2, _b2): (Vec<f32>, Vec<f32>) = (vec![0.01; 256 * 128], vec![0.0; 128]);
    let (w3, _b3): (Vec<f32>, Vec<f32>) = (vec![0.01; 128 * 10], vec![0.0; 10]);
    let input: Vec<f32> = vec![0.5; batch * 784];

    let mut h1 = vec![0.0f32; batch * 256];
    for b in 0..batch {
        for i in 0..256 {
            let mut sum = 0.0f32;
            for j in 0..784 { sum += input[b * 784 + j] * w1[i * 784 + j]; }
            h1[b * 256 + i] = if sum > 0.0 { sum } else { 0.0 };
        }
    }

    let mut h2 = vec![0.0f32; batch * 128];
    for b in 0..batch {
        for i in 0..128 {
            let mut sum = 0.0f32;
            for j in 0..256 { sum += h1[b * 256 + j] * w2[i * 256 + j]; }
            h2[b * 128 + i] = if sum > 0.0 { sum } else { 0.0 };
        }
    }

    let mut output = vec![0.0f32; batch * 10];
    for b in 0..batch {
        let mut max_val = f32::NEG_INFINITY;
        for i in 0..10 {
            let mut sum = 0.0f32;
            for j in 0..128 { sum += h2[b * 128 + j] * w3[i * 128 + j]; }
            output[b * 10 + i] = sum;
            max_val = max_val.max(sum);
        }
        let mut total = 0.0f32;
        for i in 0..10 { output[b * 10 + i] = (output[b * 10 + i] - max_val).exp(); total += output[b * 10 + i]; }
        let inv_total = 1.0 / total;
        for i in 0..10 { output[b * 10 + i] *= inv_total; }
    }
    black_box(&output[..1]);
}

fn bench_mlp_inference(c: &mut Criterion) {
    let mut group = c.benchmark_group("MLP Inference");

    group.bench_with_input("batch=1", &1, |b, &batch| {
        b.iter(|| mlp_forward_784_256_128_10(black_box(batch)));
    });

    group.bench_with_input("batch=32", &32, |b, &batch| {
        b.iter(|| mlp_forward_784_256_128_10(black_box(batch)));
    });

    group.bench_with_input("batch=256", &256, |b, &batch| {
        b.iter(|| mlp_forward_784_256_128_10(black_box(batch)));
    });

    group.finish();
}

criterion_group!(benches, bench_mlp_inference);
criterion_main!(benches);
