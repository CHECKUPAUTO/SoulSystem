//! Bayesian network inference benchmarks.
//!
//! Measures exact inference (enumeration), Gibbs sampling, and
//! belief propagation on networks of varying sizes.
//!
//! Run: cargo bench --package benchmarks --bench bayesian_inference

use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;
use scirust_probability::bayesian::{BayesianNetwork, BayesianNode, BayesianInferenceEngine};
use scirust_probability::models::Prior;
use std::collections::HashMap;

fn bench_small_network(c: &mut Criterion) {
    let mut net = BayesianNetwork::new();
    net.add_node(BayesianNode::new_continuous("a", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
    net.add_node(BayesianNode::new_continuous("b", Prior::Gaussian { mu: 1.0, sigma_sq: 1.0 }));
    net.add_node(BayesianNode::new_continuous("c", Prior::Gaussian { mu: 2.0, sigma_sq: 1.0 }));
    net.add_edge("a", "b", 0.7);
    net.add_edge("b", "c", 0.8);

    let mut engine = BayesianInferenceEngine::new(net);

    c.bench_function("Bayes query small network (enum)", |bench| {
        bench.iter(|| {
            let result = engine.query("c", &HashMap::new()).unwrap();
            black_box(result);
        })
    });
}

fn bench_gibbs_sampling(c: &mut Criterion) {
    let mut net = BayesianNetwork::new();
    net.add_node(BayesianNode::new_continuous("x", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
    net.add_node(BayesianNode::new_continuous("y", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
    net.add_node(BayesianNode::new_continuous("z", Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
    net.add_edge("x", "y", 0.8);
    net.add_edge("y", "z", 0.7);

    let mut engine = BayesianInferenceEngine::new(net);

    c.bench_function("Gibbs 500 samples 3-node", |bench| {
        bench.iter(|| {
            let result = engine.gibbs(
                &["z".to_string()],
                &HashMap::new(),
                500,
            );
            black_box(result.get("z").copied().unwrap_or(0.0));
        })
    });
}

fn bench_belief_propagation(c: &mut Criterion) {
    let mut net = BayesianNetwork::new();
    for i in 0..10 {
        let id = format!("n{}", i);
        net.add_node(BayesianNode::new_continuous(&id, Prior::Gaussian { mu: 0.0, sigma_sq: 1.0 }));
    }
    for i in 0..9 {
        net.add_edge(&format!("n{}", i), &format!("n{}", i + 1), 0.5);
    }

    c.bench_function("Belief prop 10-node chain", |bench| {
        bench.iter(|| {
            let result = net.belief_propagation(&HashMap::new(), 5);
            black_box(result.get("n0").copied().unwrap_or(0.0));
        })
    });
}

criterion_group!(benches, bench_small_network, bench_gibbs_sampling, bench_belief_propagation);
criterion_main!(benches);
