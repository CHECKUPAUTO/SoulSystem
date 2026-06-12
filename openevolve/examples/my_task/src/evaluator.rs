//! Rust evaluator for the sorting task.
//!
//! Protocol (OpenEvolve standard):
//!   - Read **candidate source code** from stdin.
//!   - Write **one JSON object** on stdout:
//!         {"score": f64, "metrics": { ... }}
//!
//! For this evaluator we don't actually JIT-compile the candidate — we treat
//! the candidate as advisory and benchmark the *currently compiled* `solve()`
//! against ground truth. That keeps the evaluator hermetic and fast.
//!
//! In a real deployment you would write the candidate to a temp file, compile
//! it with `rustc --edition 2021 -O`, run it on the test inputs, and grade
//! the output. For demonstration purposes, this evaluator reports correctness
//! and a synthetic performance score derived from the compiled binary itself.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use serde::Serialize;
use std::io::Read;
use std::time::Instant;

// Importable copy of the reference algorithm. The actual candidate would
// replace this body — for the local benchmark we just need *some* implementation.
fn solve(mut data: Vec<i64>) -> Vec<i64> {
    data.sort_unstable();
    data
}

#[derive(Serialize)]
struct Output {
    score: f64,
    metrics: Metrics,
}

#[derive(Serialize)]
struct Metrics {
    correctness: f64,
    performance: f64,
    elapsed_ms: f64,
    candidate_loc: usize,
    candidate_has_solve: bool,
}

fn main() {
    let mut candidate_src = String::new();
    let _ = std::io::stdin().read_to_string(&mut candidate_src);
    let candidate_loc = candidate_src.lines().filter(|l| !l.trim().is_empty()).count();
    let candidate_has_solve = candidate_src.contains("fn solve(");

    // ── Correctness ──────────────────────────────────────────────────────
    let mut rng = StdRng::seed_from_u64(42);
    let test_cases: Vec<Vec<i64>> = vec![
        vec![],
        vec![1],
        vec![3, 1, 2],
        vec![5, 3, 8, 1, 9, 2, 7],
        (1..=20i64).rev().collect(),
        (0..50).map(|_| rng.gen_range(-1000..1000)).collect(),
    ];
    let mut correct = 0usize;
    for input in &test_cases {
        let got = solve(input.clone());
        let mut expected = input.clone();
        expected.sort_unstable();
        if got == expected {
            correct += 1;
        }
    }
    let correctness = correct as f64 / test_cases.len() as f64;

    // ── Performance ──────────────────────────────────────────────────────
    let big: Vec<i64> = (0..5_000).map(|_| rng.gen_range(0..100_000)).collect();
    let start = Instant::now();
    let _ = solve(big.clone());
    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    // 1.0 if < 1 ms, decays to 0 around 1 s.
    let performance = (1.0 - (elapsed_ms / 1000.0)).clamp(0.0, 1.0);

    let score = if correctness < 1.0 {
        correctness * 0.5
    } else {
        0.5 * correctness + 0.5 * performance
    };

    let out = Output {
        score: (score * 1e4).round() / 1e4,
        metrics: Metrics {
            correctness,
            performance: (performance * 1e4).round() / 1e4,
            elapsed_ms,
            candidate_loc,
            candidate_has_solve,
        },
    };
    println!("{}", serde_json::to_string(&out).unwrap());
}
