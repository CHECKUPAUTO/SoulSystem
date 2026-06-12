//! GEMM (matrix multiply) benchmarks using scirust-inference tensor ops.
//!
//! Run: cargo bench --package benchmarks --bench inference_gemm

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use scirust_inference::tensor::Tensor;
use scirust_inference::tensor::math::{matmul_f32, softmax_f32, broadcast_to_f32};

fn gemm_small(c: &mut Criterion) {
    let a = Tensor::<f32>::rand(&[64, 64]);
    let b = Tensor::<f32>::rand(&[64, 64]);

    c.bench_function("GEMM 64x64", |bench| {
        bench.iter(|| {
            let c = matmul_f32(&a, &b).unwrap();
            black_box(c.data()[0]);
        })
    });
}

fn gemm_medium(c: &mut Criterion) {
    let a = Tensor::<f32>::rand(&[256, 256]);
    let b = Tensor::<f32>::rand(&[256, 256]);

    c.bench_function("GEMM 256x256", |bench| {
        bench.iter(|| {
            let c = matmul_f32(&a, &b).unwrap();
            black_box(c.data()[0]);
        })
    });
}

fn gemm_large(c: &mut Criterion) {
    let a = Tensor::<f32>::rand(&[512, 512]);
    let b = Tensor::<f32>::rand(&[512, 512]);

    c.bench_function("GEMM 512x512", |bench| {
        bench.iter(|| {
            let c = matmul_f32(&a, &b).unwrap();
            black_box(c.data()[0]);
        })
    });
}

fn softmax_bench(c: &mut Criterion) {
    let t = Tensor::<f32>::rand(&[1024, 1000]);

    c.bench_function("Softmax 1024x1000", |bench| {
        bench.iter(|| {
            let s = softmax_f32(&t);
            black_box(s.data()[0]);
        })
    });
}

fn broadcast_bench(c: &mut Criterion) {
    let t = Tensor::<f32>::rand(&[1000]);
    let shape = vec![128, 1000];

    c.bench_function("Broadcast [1000] -> [128,1000]", |bench| {
        bench.iter(|| {
            let result = broadcast_to_f32(&t, &shape).unwrap();
            black_box(result.data()[0]);
        })
    });
}

criterion_group!(benches, gemm_small, gemm_medium, gemm_large, softmax_bench, broadcast_bench);
criterion_main!(benches);
