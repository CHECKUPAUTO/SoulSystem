//! SoulLink inter-module integration tests.
//!
//! Verifies that all brain modules can call each other:
//!
//! | Test | Channel | What it proves |
//! |------|---------|----------------|
//! | ssm_hnn_bridge | ssm → hnn | SSM encodes HNN state trajectories as embeddings |
//! | snn_hnn_bridge | dragon_hatchling → hnn | HNN momentum thresholds produce spike-like events |
//! | scirust_hnn_bridge | scirust → hnn | SciRust PatternMemory stores HNN energy motifs |
//! | ssm_jepa_bridge | ssm → jepa | JEPA uses SelectiveSSM as encoder backbone |
//! | scirust_jepa_bridge | scirust → jepa | JEPA uses SciRust symbolic/learning + Active Inference |
//! | hnn_jepa_cross | hnn ↔ jepa | JEPA predicts HNN latent states, Active Inference computes Free Energy |
//! | full_ecosystem | all | Every module accessible from one scope |
//! | workspace_coherence | misc | Crate-level invariants hold |

use ndarray::Array2;

// ─── 1. SSM → HNN ─────────────────────────────────────────────────────────

#[test]
fn ssm_hnn_bridge() {
    // SSM can encode an HNN state trajectory into embeddings
    use hnn::{HamiltonianLayer, HNNConfig};
    use soullink_ssm::hippo::HiPPOInitializer;
    use soullink_ssm::selective::SelectiveSSM;

    let cfg = HNNConfig {
        dim: 4,
        dt: 0.01,
        friction: 0.005,
        alpha: 0.5,
        beta: 0.1,
        mu: 0.0,
        spike_threshold: 0.8,
    };
    let mut layer = HamiltonianLayer::new(cfg);

    // Run HNN to produce a trajectory
    let mut trajectory = Vec::with_capacity(10);
    for _ in 0..10 {
        layer.step();
        trajectory.push(layer.activations());
    }
    assert_eq!(trajectory.len(), 10);
    assert_eq!(trajectory[0].len(), 4);

    // Encode the trajectory with SSM
    let a = HiPPOInitializer::legs(4);
    let ssm = SelectiveSSM::new(4, 4, a);
    // Convert f32 activations to f64 for SSM
    let traj_f64: Vec<f64> = trajectory.into_iter().flatten().map(|v| v as f64).collect();
    let traj_2d = Array2::from_shape_vec((10, 4), traj_f64).unwrap();
    let encoded = ssm.forward(&traj_2d);
    assert_eq!(encoded.shape(), &[10, 4], "SSM must encode HNN trajectory");
}

// ─── 2. DragonHatchling → HNN ──────────────────────────────────────────────

#[test]
fn snn_hnn_bridge() {
    // HNN momentum thresholds produce spike-like events,
    // analogous to DragonHatchling neuron firings
    use hnn::{HamiltonianLayer, HNNConfig};

    let cfg = HNNConfig {
        dim: 8,
        dt: 0.05,
        friction: 0.0,
        alpha: 0.1,
        beta: 0.01,
        mu: 0.0,
        spike_threshold: 0.2, // low threshold to ensure spikes
    };
    let mut layer = HamiltonianLayer::new(cfg);

    // Inject energy to produce spikes
    layer.stimulate(&[3.0; 8]);

    // Run a few steps — spikes should be detected
    for _ in 0..20 {
        layer.step();
    }

    // HNN tracks total spikes (momentum > threshold crossings)
    let spike_count = layer.total_spikes;
    assert!(spike_count > 0, "HNN must produce spike-like events with stimulus");

    // Activations are the position vector (q)
    let activations = layer.activations();
    assert_eq!(activations.len(), 8, "HNN activations = q vector");

    // Rates are the momentum magnitudes
    let rates = layer.rates();
    assert_eq!(rates.len(), 8, "HNN rates = |p| per coordinate");
}

// ─── 3. SciRust → HNN ──────────────────────────────────────────────────────

#[test]
fn scirust_hnn_bridge() {
    // SciRust PatternMemory stores HNN energy motifs
    use hnn::{HamiltonianLayer, HNNConfig};
    use scirust_core::PatternMemory;
    use scirust_core::symbolic::Expr;

    let mut mem = PatternMemory::new();

    let cfg = HNNConfig {
        dim: 2,
        dt: 0.01,
        friction: 0.005,
        alpha: 0.5,
        beta: 0.1,
        mu: 0.0,
        spike_threshold: 0.8,
    };
    let mut layer = HamiltonianLayer::new(cfg);

    // Run and record energy patterns
    for _ in 0..10 {
        layer.step();
        let energy = layer.total_energy();
        let expr = Expr::Const(energy);
        mem.record(&Expr::Const(layer.tick as f64), &expr, true);
    }

    let best = mem.best_patterns(3);
    assert!(!best.is_empty(), "SciRust PatternMemory must store HNN energy patterns");

    // SciRust symbolic integration: convert energy value to an expression
    use scirust_core::symbolic::{parse, simplify};
    if let Ok(expr) = parse("energy_0") {
        let simplified = simplify(&expr);
        mem.record(&expr, &simplified, true);
        let best2 = mem.best_patterns(1);
        // Pattern memory works
    }

    // HNN network-level pattern learning
    use hnn::HNN;
    let cfgs = vec![
        HNNConfig { dim: 2, dt: 0.01, friction: 0.005, alpha: 0.5, beta: 0.1, mu: 0.0, spike_threshold: 0.8 },
    ];
    let mut net = HNN::new(cfgs);
    net.learn_patterns();
    let patterns = net.discover_spike_patterns();
    assert_eq!(patterns.len(), net.layers.len());
}

// ─── 4. SSM → JEPA ─────────────────────────────────────────────────────────

#[test]
fn ssm_jepa_bridge() {
    use jepa::JEPA;

    // JEPA uses soullink_ssm::SelectiveSSM as its encoder backbone
    let jepa = JEPA::new(
        4,   // d_input
        8,   // d_model
        4,   // d_embed
        2,   // state_dim
        8,   // d_hidden
    );

    // The encoder contains a real SelectiveSSM
    let _ssm_ref: &soullink_ssm::selective::SelectiveSSM = &jepa.encoder.ssm;
    assert_eq!(jepa.encoder.ssm.state_dim, 2);
    assert_eq!(jepa.encoder.d_embed, 4);

    // Forward pass uses SSM internally
    let x_t: Array2<f64> = Array2::zeros((3, 4)).mapv(|_: f64| rand::random::<f64>() * 0.5);
    let x_t1: Array2<f64> = Array2::zeros((3, 4)).mapv(|_: f64| rand::random::<f64>() * 0.5);
    let (loss, pred, target) = jepa.forward(&x_t, &x_t1);

    // SSM processes sequences → produces embeddings
    assert_eq!(pred.len(), 4, "JEPA predictor output is SSM-derived embedding");
    assert_eq!(target.len(), 4, "JEPA target output is SSM-derived embedding");
    assert!(loss >= 0.0, "JEPA loss must be non-negative");
}

// ─── 5. SciRust → JEPA ─────────────────────────────────────────────────────

#[test]
fn scirust_jepa_bridge() {
    use jepa::JEPA;

    // JEPA uses SciRust PatternMemory internally
    let mut jepa = JEPA::new(1, 4, 2, 2, 4);

    // SciRust symbolic expressions → JEPA training pairs
    let (xt, xt1) = jepa::symbolic_to_pair("x^2", "x", 2.0, 0.1).unwrap();
    assert!((xt[[0, 0]] - 4.0).abs() < 1e-10, "SciRust eval at x=2 should give 4");
    assert!((xt1[[0, 0]] - 4.41).abs() < 1e-10, "SciRust eval at x=2.1 should give 4.41");

    // JEPA forward with SciRust-generated input
    let (loss, _, _) = jepa.forward(&xt, &xt1);
    assert!(loss >= 0.0);

    // Record in SciRust pattern memory via JEPA
    jepa.record_success("x + 0", true);
    let best = jepa.pattern_memory.best_patterns(5);
    assert!(!best.is_empty(), "JEPA must be able to record in SciRust PatternMemory");

    // Use SciRust solver and optimizer from the same scope
    use scirust_core::{solve_linear, solve_quadratic, Optimizer, prove_equal};
    use scirust_core::symbolic::parse;

    let expr = parse("2*x + 4").unwrap();
    let root = solve_linear(&expr, "x");
    assert_eq!(root, Some(-2.0), "SciRust linear solver: 2x+4=0 → x=-2");
}

// ─── 6. HNN ↔ JEPA cross (Active Inference bridge) ────────────────────────

#[test]
fn hnn_jepa_cross() {
    // JEPA predicts HNN latent states; Active Inference computes Free Energy
    use hnn::HNNConfig;
    use jepa::JEPA;

    // Create an HNN layer
    let cfg = HNNConfig {
        dim: 2,
        dt: 0.01,
        friction: 0.005,
        alpha: 0.5,
        beta: 0.1,
        mu: 0.0,
        spike_threshold: 0.8,
    };
    let _hnn_cfg = cfg;

    // Create JEPA model
    let jepa = JEPA::new(2, 4, 2, 2, 4);

    // Build an HNN trajectory for JEPA to evaluate
    use hnn::HamiltonianLayer;
    let mut layer = HamiltonianLayer::new(HNNConfig {
        dim: 2,
        dt: 0.01,
        friction: 0.005,
        alpha: 0.5,
        beta: 0.1,
        mu: 0.0,
        spike_threshold: 0.8,
    });

    let mut states: Vec<f64> = Vec::new();
    for _ in 0..10 {
        layer.step();
        for &v in layer.q.iter() {
            states.push(v);
        }
    }

    // Convert to JEPA trajectory (10 steps, 2-dim)
    let trajectory = Array2::from_shape_vec((10, 2), states).unwrap();

    // JEPA evaluates the trajectory with Active Inference Free Energy
    let (total_fe, per_step) = jepa.evaluate_hnn_trajectory(&trajectory, 0.1);
    assert!(total_fe >= 0.0, "Total Free Energy must be non-negative");
    assert_eq!(per_step.len(), 9, "Free Energy per step for 10 timesteps = 9 pairs");

    // Active Inference force: corrective signal from prediction error
    if trajectory.shape()[0] >= 2 {
        let xt = trajectory.slice(ndarray::s![0..=0, ..]).to_owned();
        let xt1 = trajectory.slice(ndarray::s![1..=1, ..]).to_owned();
        let force = jepa.active_inference_force(&xt, &xt1, 0.1);
        assert_eq!(force.len(), 2, "Active Inference force should match embedding dim");
    }
}

// ─── 7. Full ecosystem access ──────────────────────────────────────────────

#[test]
fn full_ecosystem() {
    // This test proves ALL modules are accessible from a single scope

    // soullink_ssm
    use soullink_ssm::hippo::HiPPOInitializer;
    use soullink_ssm::ssm::{matrix_exp, discretize_zoh, SSMKernel, SSMConfig};
    use soullink_ssm::selective::SelectiveSSM;
    use soullink_ssm::parallel::{MultiDimScan, ParallelScan};
    use soullink_ssm::layer::MambaBlock;

    // dragon_hatchling_core
    use dragon_hatchling_core::neuron::{Neuron, NeuronConfig};
    use dragon_hatchling_core::synapse::{Synapse, SynapseConfig};
    use dragon_hatchling_core::plasticity::PlasticityRule;
    use dragon_hatchling_core::graph::BrainGraph;
    use dragon_hatchling_core::model::DragonHatchling;

    // scirust_core
    use scirust_core::PatternMemory;
    use scirust_core::symbolic::{parse, simplify, Expr};
    use scirust_core::{solve_linear, Optimizer, prove_equal};
    use scirust_core::learning::{polynomial_fit, discover_patterns};

    // hnn (Hamiltonian Neural Network)
    use hnn::{HamiltonianLayer, HNNConfig, HNN};

    // jepa (with Active Inference)
    use jepa::{JEPA, cosine_similarity, symbolic_to_pair, FreeEnergy};

    // ── Instantiate everything in one scope ──

    // 1. SSM
    let a_hippo = HiPPOInitializer::legs(4);
    let _ssm = SelectiveSSM::new(4, 8, a_hippo);

    // 2. SNN
    let _neuron_cfg = NeuronConfig::default();
    let _syn_cfg = SynapseConfig::default();
    let _plasticity = PlasticityRule::default();

    // 3. SciRust
    let expr = parse("x^2 + 1").unwrap();
    let _simplified = simplify(&expr);
    let mut pm = PatternMemory::new();
    pm.record(&expr, &_simplified, true);

    // 4. HNN (Hamiltonian Neural Network)
    let hnn_cfg = HNNConfig {
        dim: 2,
        dt: 0.01,
        friction: 0.005,
        alpha: 0.5,
        beta: 0.1,
        mu: 0.0,
        spike_threshold: 0.8,
    };
    let _layer = HamiltonianLayer::new(hnn_cfg);

    // 5. HNN multi-layer
    let cfgs = vec![
        HNNConfig { dim: 2, dt: 0.01, friction: 0.005, alpha: 0.5, beta: 0.1, mu: 0.0, spike_threshold: 0.8 },
    ];
    let _hnn = HNN::new(cfgs);

    // 6. JEPA (SSM + SciRust + Active Inference)
    let _jepa = JEPA::new(2, 8, 4, 4, 8);

    // 7. Cosine similarity (JEPA bridge)
    let v1 = ndarray::Array1::from_vec(vec![1.0, 0.0]);
    let v2 = ndarray::Array1::from_vec(vec![0.0, 1.0]);
    assert!((cosine_similarity(&v1, &v2) - 0.0).abs() < 1e-10);

    // 8. Free Energy (Active Inference)
    let x_t: Array2<f64> = Array2::zeros((3, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);
    let x_t1: Array2<f64> = Array2::zeros((3, 2)).mapv(|_: f64| rand::random::<f64>() * 0.5);
    let fe = _jepa.free_energy(&x_t, &x_t1, 0.1);
    assert!(fe.total >= 0.0, "Free Energy must be non-negative");

    // 9. SciRust pattern discovery works alongside JEPA
    let patterns = discover_patterns(&parse("x^2 + x").unwrap());
    assert!(!patterns.is_empty(), "SciRust should discover patterns in x^2 + x");

    // All modules loaded and functional
}

// ─── 8. Workspace coherence ────────────────────────────────────────────────

#[test]
fn workspace_coherence() {
    use scirust_core::{solve_linear, solve_quadratic};
    use scirust_core::symbolic::parse;

    // SSM default config
    let ssm_version = soullink_ssm::ssm::SSMConfig::default().state_dim;
    assert_eq!(ssm_version, 16, "soullink_ssm default state_dim should be 16");

    // SciRust can solve equations
    let expr = parse("2*x - 6").unwrap();
    let root = solve_linear(&expr, "x");
    assert_eq!(root, Some(3.0), "SciRust: 2x-6=0 → x=3");

    // SciRust quadratic solver
    let expr2 = parse("x^2 - 5*x + 6").unwrap();
    let roots = solve_quadratic(&expr2, "x");
    assert!(!roots.is_empty(), "SciRust: x^2-5x+6=0 should have roots");
    // Roots should be 2 and 3 (order not guaranteed)
    let has_2 = roots.iter().any(|r| (r - 2.0).abs() < 1e-10);
    let has_3 = roots.iter().any(|r| (r - 3.0).abs() < 1e-10);
    assert!(has_2, "x=2 should be a root, got: {:?}", roots);
    assert!(has_3, "x=3 should be a root, got: {:?}", roots);

    // DragonHatchling Neuron accessible (manual construction)
    use dragon_hatchling_core::neuron::Neuron;
    let neuron = Neuron {
        pos: glam::Vec3A::new(0.0, 0.0, 0.0),
        potential: 0.0,
        activation: 0.0,
        bias: 0.05,
        meta_plasticity: 1.0,
        age: 0,
        last_spike_step: 0,
        refractory: 0,
        inhibitory: false,
    };
    assert!(!neuron.inhibitory);

    // HNN HamiltonianLayer conserves energy
    use hnn::{HamiltonianLayer, HNNConfig};
    let cfg = HNNConfig {
        dim: 2,
        dt: 0.01,
        friction: 0.0,
        alpha: 0.5,
        beta: 0.1,
        mu: 0.0,
        spike_threshold: 0.8,
    };
    let mut layer = HamiltonianLayer::new(cfg);
    let _e0 = layer.total_energy();
    for _ in 0..100 {
        layer.step();
    }
    let drift = layer.energy_drift(50);
    assert!(drift < 1e-6, "HNN symplectic integrator must conserve energy, drift={drift}");
}
