//! Autodiff benchmark — tape construction and backward pass.
//!
//! Run: cargo bench --package benchmarks --bench autodiff_bench

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use scirust_autodiff::tape::{Tape, Var};

fn bench_small_graph(c: &mut Criterion) {
    c.bench_function("Autodiff tape: 10 ops", |bench| {
        bench.iter(|| {
            let tape = Tape::new();
            let x = tape.var("x", 2.0);
            let y = tape.var("y", 3.0);
            let z = x.add(&y).mul(&x).sub(&y.sin()).powi(2);
            tape.backward(&z);
            black_box(x.grad());
        })
    });
}

fn bench_medium_graph(c: &mut Criterion) {
    c.bench_function("Autodiff tape: 100 ops", |bench| {
        bench.iter(|| {
            let tape = Tape::new();
            let mut v = tape.var("v0", 1.5);
            for i in 1..50 {
                let vi = tape.var(&format!("v{}", i), i as f64 * 0.1);
                v = v.add(&vi).mul(&v).sub(&vi.sin());
            }
            tape.backward(&v);
            black_box(v.val());
        })
    });
}

fn bench_adam_optimizer(c: &mut Criterion) {
    use scirust_autodiff::train::{AdamOptimizer, Loss, SGDOptimizer, Trainer};

    c.bench_function("SGD: 10 steps linear regression", |bench| {
        bench.iter(|| {
            let mut w = vec![0.0f64; 3];
            let mut b = 0.0f64;
            let inputs: Vec<Vec<f64>> = vec![vec![1.0, 2.0, 3.0]; 10];
            let targets: Vec<f64> = vec![5.0; 10];
            let input_refs: Vec<&[f64]> = inputs.iter().map(|v| v.as_slice()).collect();

            let mut trainer = Trainer::new(Loss::MSE, 0.01);
            let _losses = trainer.train(&mut w, &mut b, &input_refs, &targets, 10);
            black_box(w[0]);
        })
    });
}

criterion_group!(benches, bench_small_graph, bench_medium_graph, bench_adam_optimizer);
criterion_main!(benches);
