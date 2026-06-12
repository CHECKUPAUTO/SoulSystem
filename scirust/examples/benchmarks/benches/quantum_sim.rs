//! Quantum circuit simulation benchmarks.
//!
//! Measures state-vector simulation for circuits of varying qubit counts.
//!
//! Run: cargo bench --package benchmarks --bench quantum_sim

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use scirust_quantum::circuit::QuantumCircuit;
use scirust_quantum::simulator::{probabilities, simulate};
use scirust_quantum::gradient::parameter_shift;

fn bench_2qubit_bell(c: &mut Criterion) {
    let qc = QuantumCircuit::bell_state();

    c.bench_function("Quantum Bell state (2 qubits)", |bench| {
        bench.iter(|| {
            let probs = probabilities(black_box(&qc)).unwrap();
            black_box(probs[0]);
        })
    });
}

fn bench_4qubit_variational(c: &mut Criterion) {
    let qc = QuantumCircuit::variational_circuit(4);

    c.bench_function("Quantum variational 4-qubit", |bench| {
        bench.iter(|| {
            let state = simulate(black_box(&qc)).unwrap();
            black_box(state[0]);
        })
    });
}

fn bench_8qubit_random(c: &mut Criterion) {
    let mut qc = QuantumCircuit::new(8);
    for i in 0..8 {
        qc.h(i);
        qc.ry_angle(i, i as f64 * 0.3);
    }
    for i in 0..7 {
        qc.cnot(i, i + 1);
    }

    c.bench_function("Quantum 8-qubit random circuit", |bench| {
        bench.iter(|| {
            let state = simulate(black_box(&qc)).unwrap();
            black_box(state[0]);
        })
    });
}

fn bench_12qubit_depth3(c: &mut Criterion) {
    let mut qc = QuantumCircuit::new(12);
    for layer in 0..3 {
        for i in 0..12 {
            qc.ry_angle(i, (i as f64 + layer as f64) * 0.5);
        }
        for i in 0..11 {
            qc.cnot(i, i + 1);
        }
    }

    c.bench_function("Quantum 12-qubit depth-3", |bench| {
        bench.iter(|| {
            let state = simulate(black_box(&qc)).unwrap();
            black_box(state[0]);
        })
    });
}

fn bench_parameter_shift(c: &mut Criterion) {
    let mut qc = QuantumCircuit::new_variational(2, &["t0", "t1", "t2"]);
    qc.ry(0, 0);
    qc.ry(1, 1);
    qc.cnot(0, 1);
    qc.rz(0, 2);

    let params = vec![0.5, 0.3, 0.8];

    c.bench_function("Parameter-shift gradient (2 qubits)", |bench| {
        bench.iter(|| {
            let grad = parameter_shift(black_box(&qc), 0, black_box(&params)).unwrap();
            black_box(grad);
        })
    });
}

criterion_group!(benches, bench_2qubit_bell, bench_4qubit_variational, bench_8qubit_random, bench_12qubit_depth3, bench_parameter_shift);
criterion_main!(benches);
