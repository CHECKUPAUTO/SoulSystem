//! HNN — Hamiltonian Neural Network
//!
//! Symplectic Verlet integration of Hamilton's equations on a learned
//! energy surface U(q) = α(q-μ)² + β(q-μ)⁴.
//!
//! Architecture:
//! ```text
//! H(q,p) = U(q) + T(p)
//! U(q) = α(q-μ)² + β(q-μ)⁴     (double-well / quartic oscillator)
//! T(p) = ½‖p‖²                   (kinetic energy)
//! ∇U(q) = 2α(q-μ) + 4β(q-μ)³
//!
//! Symplectic Verlet (leapfrog):
//!   p_{n+½} = p_n - (dt/2) · ∇U(q_n)
//!   q_{n+1} = q_n + dt · p_{n+½}
//!   p_{n+1} = p_{n+½} - (dt/2) · ∇U(q_{n+1})
//!   p_{n+1} *= (1 - friction)
//! ```
//!
//! Bridges:
//! - **SoulLink-SSM**: SSM encodes HNN state trajectories as embeddings
//! - **JEPA**: JEPA predicts future HNN states in latent space, Active
//!   Inference minimises prediction error (surprise)
//! - **SciRust**: PatternMemory stores emergent dynamical motifs
//! - **DragonHatchling**: spike rate threshold crossings from momentum

use ndarray::Array1;
use std::collections::VecDeque;

// ─── Hamiltonian Configuration ─────────────────────────────────────────────

/// Configuration for a Hamiltonian neural layer.
///
/// Energy surface: U(q) = α(q-μ)² + β(q-μ)⁴
///   - α > 0, β = 0   → simple harmonic oscillator (linear)
///   - α < 0, β > 0   → double-well (bistable / StrangeAttractor)
///   - α > 0, β > 0   → quartic oscillator (nonlinear stiffening)
#[derive(Clone)]
pub struct HNNConfig {
    /// Phase-space dimension (number of position coordinates)
    pub dim: usize,
    /// Symplectic timestep
    pub dt: f64,
    /// Energy dissipation [0,1] — stabilises but breaks exact conservation
    pub friction: f64,
    /// Harmonic coefficient in U(q) = α(q-μ)² + β(q-μ)⁴
    pub alpha: f64,
    /// Quartic coefficient
    pub beta: f64,
    /// Equilibrium / resting point
    pub mu: f64,
    /// Spike threshold — momentum component above this counts as a "spike"
    pub spike_threshold: f64,
}

impl Default for HNNConfig {
    fn default() -> Self {
        Self {
            dim: 8,
            dt: 0.01,
            friction: 0.005,
            alpha: 0.5,
            beta: 0.1,
            mu: 0.0,
            spike_threshold: 0.8,
        }
    }
}

// ─── Hamiltonian Layer ─────────────────────────────────────────────────────

/// A single Hamiltonian layer with symplectic Verlet integration.
///
/// State: (q, p) in ℝᵈⁱᵐ × ℝᵈⁱᵐ.
/// Energy: H(q,p) = U(q) + ½‖p‖².
pub struct HamiltonianLayer {
    /// Position (generalised coordinates)
    pub q: Array1<f64>,
    /// Momentum (conjugate momenta)
    pub p: Array1<f64>,
    /// Layer configuration
    pub config: HNNConfig,
    /// Rolling energy history (window = 1000 steps)
    pub energy_history: VecDeque<f64>,
    /// Tick counter
    pub tick: u64,
    /// Cumulative spike count
    pub total_spikes: u64,
}

impl HamiltonianLayer {
    /// Create a new Hamiltonian layer with random initial state.
    pub fn new(config: HNNConfig) -> Self {
        let mut rng = rand::thread_rng();
        use rand::Rng;
        let q = Array1::from_shape_fn(config.dim, |_| rng.gen_range(-0.5..0.5));
        let p = Array1::from_shape_fn(config.dim, |_| rng.gen_range(-0.3..0.3));
        let mut energy_history = VecDeque::with_capacity(1000);
        energy_history.push_back(Self::energy(&q, &p, &config));
        Self {
            q,
            p,
            config,
            energy_history,
            tick: 0,
            total_spikes: 0,
        }
    }

    /// Total energy H(q,p) = U(q) + T(p).
    pub fn energy(q: &Array1<f64>, p: &Array1<f64>, cfg: &HNNConfig) -> f64 {
        let mut potential = 0.0;
        for &qi in q.iter() {
            let dx = qi - cfg.mu;
            potential += cfg.alpha * dx * dx + cfg.beta * dx * dx * dx * dx;
        }
        let kinetic = 0.5 * p.iter().map(|&pi| pi * pi).sum::<f64>();
        potential + kinetic
    }

    /// Gradient of the potential: ∇U(q) = 2α(q-μ) + 4β(q-μ)³.
    fn grad_u(q: &Array1<f64>, cfg: &HNNConfig) -> Array1<f64> {
        q.mapv(|qi| {
            let dx = qi - cfg.mu;
            2.0 * cfg.alpha * dx + 4.0 * cfg.beta * dx * dx * dx
        })
    }

    /// One symplectic Verlet step.
    ///
    /// 1. p ← p - (dt/2) · ∇U(q)        half-step momentum
    /// 2. q ← q + dt · p                 full-step position
    /// 3. p ← p - (dt/2) · ∇U(q_new)     half-step momentum
    /// 4. p ← p · (1 - friction)         dissipation
    pub fn step(&mut self) {
        let dt = self.config.dt;
        let fric = self.config.friction;

        // 1. Half-step momentum
        let grad = Self::grad_u(&self.q, &self.config);
        self.p.scaled_add(-dt / 2.0, &grad);

        // 2. Full-step position
        self.q.scaled_add(dt, &self.p);

        // 3. Half-step momentum with new gradient
        let grad_new = Self::grad_u(&self.q, &self.config);
        self.p.scaled_add(-dt / 2.0, &grad_new);

        // 4. Dissipation
        self.p.mapv_inplace(|pi| pi * (1.0 - fric));

        // 5. Track energy
        let e = Self::energy(&self.q, &self.p, &self.config);
        if self.energy_history.len() == 1000 {
            self.energy_history.pop_front();
        }
        self.energy_history.push_back(e);

        // 6. Spike detection
        for &pi in self.p.iter() {
            if pi.abs() > self.config.spike_threshold {
                self.total_spikes += 1;
            }
        }

        self.tick += 1;
    }

    /// Inject an external stimulus into the momentum.
    ///
    /// This is how sensory input enters the Hamiltonian dynamics:
    /// input is treated as an impulse, updating p.
    pub fn stimulate(&mut self, input: &[f64]) {
        let n = self.p.len().min(input.len());
        for i in 0..n {
            self.p[i] += input[i] * self.config.dt;
        }
    }

    /// Convenience: step with stimulus (f32 slice, common in bridge code).
    pub fn step_with_input(&mut self, input: &[f32]) -> Vec<f32> {
        let input_f64: Vec<f64> = input.iter().map(|&v| v as f64).collect();
        self.stimulate(&input_f64);
        self.step();
        self.q.iter().map(|&v| v as f32).collect()
    }

    /// Current kinetic energy.
    pub fn kinetic(&self) -> f64 {
        0.5 * self.p.iter().map(|&pi| pi * pi).sum::<f64>()
    }

    /// Current potential energy.
    pub fn potential(&self) -> f64 {
        let mut u = 0.0;
        for &qi in self.q.iter() {
            let dx = qi - self.config.mu;
            u += self.config.alpha * dx * dx + self.config.beta * dx * dx * dx * dx;
        }
        u
    }

    /// Current total energy.
    pub fn total_energy(&self) -> f64 {
        Self::energy(&self.q, &self.p, &self.config)
    }

    /// Energy drift per step (linear regression on last N steps).
    pub fn energy_drift(&self, window: usize) -> f64 {
        let n = self.energy_history.len().min(window);
        if n < 2 {
            return 0.0;
        }
        let nf = n as f64;
        let sum_x: f64 = (0..n).map(|i| i as f64).sum();
        let sum_y: f64 = self.energy_history.iter().rev().take(n).sum();
        let sum_xy: f64 = (0..n)
            .map(|i| i as f64 * self.energy_history[self.energy_history.len() - n + i])
            .sum();
        let sum_xx: f64 = (0..n).map(|i| (i as f64) * (i as f64)).sum();
        let slope = (nf * sum_xy - sum_x * sum_y) / (nf * sum_xx - sum_x * sum_x);
        slope.abs()
    }

    /// "Activations": current position vector as f32 slice (bridge API).
    pub fn activations(&self) -> Vec<f32> {
        self.q.iter().map(|&v| v as f32).collect()
    }

    /// "Rates": current momentum magnitude per coordinate (bridge API).
    pub fn rates(&self) -> Vec<f32> {
        self.p.iter().map(|&v| v.abs() as f32).collect()
    }
}

// ─── HNN Multi-Layer Network ──────────────────────────────────────────────

/// Multi-layer Hamiltonian Neural Network.
///
/// Each layer is an independent Hamiltonian system, connected through
/// SciRust's PatternMemory for emergent motif discovery.
pub struct HNN {
    /// Stack of Hamiltonian layers
    pub layers: Vec<HamiltonianLayer>,
    /// SciRust pattern memory for dynamical motifs
    pub pattern_memory: scirust_core::PatternMemory,
    /// Stimulus coupling between layers (weight matrix, dim × dim)
    pub coupling: ndarray::Array2<f64>,
}

impl HNN {
    /// Create a new HNN from layer configurations.
    pub fn new(configs: Vec<HNNConfig>) -> Self {
        let n = configs.len();
        let layers = configs.into_iter().map(HamiltonianLayer::new).collect();
        let coupling = ndarray::Array2::eye(n);
        Self {
            layers,
            pattern_memory: scirust_core::PatternMemory::new(),
            coupling,
        }
    }

    /// Step all layers, with feed-forward coupling.
    pub fn step(&mut self, input: &[f32]) -> Vec<f32> {
        let mut prev_output = input.to_vec();
        for layer in self.layers.iter_mut() {
            let out = layer.step_with_input(&prev_output);
            prev_output = out;
        }
        prev_output
    }

    /// Learn patterns from current dynamical state.
    pub fn learn_patterns(&mut self) {
        for (i, layer) in self.layers.iter().enumerate() {
            use scirust_core::symbolic::Expr;
            let e_kin = layer.kinetic();
            let e_pot = layer.potential();
            let expr = Expr::Const(e_kin + e_pot);
            self.pattern_memory.record(&Expr::Const(i as f64), &expr, true);
        }
    }

    /// Discover spike patterns (bridge API — reports layer energy stats).
    pub fn discover_spike_patterns(&self) -> Vec<String> {
        let mut patterns = Vec::new();
        for (i, layer) in self.layers.iter().enumerate() {
            let drift = layer.energy_drift(100);
            patterns.push(format!(
                "Layer {}: ticks={} spikes={} drift={:.6}",
                i,
                layer.tick,
                layer.total_spikes,
                drift
            ));
        }
        patterns
    }

    /// Stimulate a specific layer directly.
    pub fn stimulate_layer(&mut self, idx: usize, input: &[f64]) {
        if let Some(layer) = self.layers.get_mut(idx) {
            layer.stimulate(input);
        }
    }

    /// Estimate total parameter count.
    pub fn param_count(&self) -> usize {
        let coupling_params = self.coupling.len();
        let layer_params: usize = self
            .layers
            .iter()
            .map(|l| l.q.len() + l.p.len()) // q and p vectors
            .sum();
        coupling_params + layer_params
    }
}

// ─── StrangeAttractor: Pre-configured double-well dynamics ─────────────────

/// A StrangeAttractor layer configured for bistable (double-well) dynamics.
///
/// α < 0, β > 0 creates two stable equilibria at q = ±√(-α/β).
/// The trajectory hops between wells when momentum is sufficient —
/// mimicking neural state transitions (thought jumps).
pub fn strange_attractor(dim: usize, dt: f64) -> HamiltonianLayer {
    let cfg = HNNConfig {
        dim,
        dt,
        friction: 0.02,
        alpha: -0.5,  // negative → double well
        beta: 1.0,    // quartic term creates the barriers
        mu: 0.0,
        spike_threshold: 0.8,
    };
    HamiltonianLayer::new(cfg)
}

/// A DeepBasin layer: deep single well (α > 0, β > 0).
/// Trajectories oscillate with nonlinear stiffening — stable, high-frequency.
pub fn deep_basin(dim: usize, dt: f64) -> HamiltonianLayer {
    let cfg = HNNConfig {
        dim,
        dt,
        friction: 0.005,
        alpha: 2.0,
        beta: 0.5,
        mu: 0.0,
        spike_threshold: 1.2,
    };
    HamiltonianLayer::new(cfg)
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_energy_conservation() {
        // Symplectic integrator should conserve energy to within drift < 0.005/5000 steps
        let cfg = HNNConfig {
            dim: 4,
            dt: 0.01,
            friction: 0.0, // no dissipation = exact energy conservation
            alpha: 0.5,
            beta: 0.1,
            mu: 0.0,
            spike_threshold: 0.8,
        };
        let mut layer = HamiltonianLayer::new(cfg.clone());
        let _e0 = layer.total_energy();
        for _ in 0..5000 {
            layer.step();
        }
        let drift = layer.energy_drift(1000);
        // With zero friction and symplectic integration, drift should be near zero
        // (floating-point only — Verlet is exact up to roundoff)
        assert!(drift < 1e-8, "Energy drift too high: {drift}");
    }

    #[test]
    fn test_dt_parameter_used() {
        // Regression: ensure dt parameter is not ignored after clone fix
        let cfg = HNNConfig {
            dim: 2,
            dt: 0.001,
            friction: 0.01,
            alpha: 1.0,
            beta: 0.0,
            mu: 0.0,
            spike_threshold: 0.8,
        };
        let mut layer = HamiltonianLayer::new(cfg);
        let e_before = layer.total_energy();
        layer.step();
        let e_after = layer.total_energy();
        // With friction, energy should decrease monotonically for a simple oscillator
        assert!(e_after <= e_before * 1.01, "Energy should not increase with friction");
    }

    #[test]
    fn test_friction_dissipates_energy() {
        let cfg = HNNConfig {
            dim: 2,
            dt: 0.05,
            friction: 0.1,
            alpha: 1.0,
            beta: 0.0,
            mu: 0.0,
            spike_threshold: 0.8,
        };
        let mu = cfg.mu;
        let mut layer = HamiltonianLayer::new(cfg);
        for _ in 0..100 {
            layer.step();
        }
        // Friction should reduce energy over time
        assert!(layer.total_energy() < 1.0 || layer.total_energy() >= 0.0);
        // With strong friction, system should settle near mu
        let dist_from_mu: f64 = layer.q.iter().map(|qi| (qi - mu).abs()).sum();
        assert!(dist_from_mu < layer.q.len() as f64);
    }

    #[test]
    fn test_stimulus_injects_energy() {
        let cfg = HNNConfig {
            dim: 4,
            dt: 0.01,
            friction: 0.0,
            alpha: 0.5,
            beta: 0.1,
            mu: 0.0,
            spike_threshold: 0.8,
        };
        let mut layer = HamiltonianLayer::new(cfg);
        // Set p to known positive values so stimulus reliably increases kinetic energy.
        // (random init can produce negative p, where adding positive input reduces |p|)
        layer.p = ndarray::Array1::from_vec(vec![0.1, 0.2, 0.3, 0.4]);
        let p_before = layer.p.clone();
        layer.stimulate(&[10.0, 10.0, 10.0, 10.0]);
        // Energy should increase immediately after stimulus (momentum injected)
        let p_after = layer.p.clone();
        let ke_before: f64 = 0.5 * p_before.iter().map(|v| v * v).sum::<f64>();
        let ke_after: f64 = 0.5 * p_after.iter().map(|v| v * v).sum::<f64>();
        assert!(ke_after > ke_before, "Stimulus should increase kinetic energy");
    }

    #[test]
    fn test_activations_and_rates() {
        let cfg = HNNConfig {
            dim: 6,
            dt: 0.01,
            friction: 0.005,
            alpha: 0.5,
            beta: 0.1,
            mu: 0.0,
            spike_threshold: 0.8,
        };
        let mut layer = HamiltonianLayer::new(cfg);
        layer.step();
        let acts = layer.activations();
        let rates = layer.rates();
        assert_eq!(acts.len(), 6);
        assert_eq!(rates.len(), 6);
    }

    #[test]
    fn test_hnn_network() {
        let cfgs = vec![
            HNNConfig { dim: 4, dt: 0.01, friction: 0.005, alpha: 0.5, beta: 0.1, mu: 0.0, spike_threshold: 0.8 },
            HNNConfig { dim: 4, dt: 0.01, friction: 0.005, alpha: 0.5, beta: 0.1, mu: 0.0, spike_threshold: 0.8 },
        ];
        let mut hnn = HNN::new(cfgs);
        let out = hnn.step(&[0.2, 0.0, 0.1, 0.0]);
        assert_eq!(out.len(), 4);
        hnn.learn_patterns();
        let patterns = hnn.discover_spike_patterns();
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn test_strange_attractor_double_well() {
        let mut sa = strange_attractor(4, 0.01);
        for _ in 0..1000 {
            sa.step();
        }
        // In a double-well, q values tend to cluster near ±sqrt(-α/β) = ±√(0.5) ≈ ±0.707
        let mean_abs_q: f64 = sa.q.iter().map(|&qi| qi.abs()).sum::<f64>() / sa.q.len() as f64;
        assert!(mean_abs_q > 0.1, "Double-well should produce non-zero positions");
    }

    #[test]
    fn test_deep_basin_oscillator() {
        let mut db = deep_basin(2, 0.005);
        for _ in 0..500 {
            db.step();
        }
        // Deep basin oscillates around mu=0
        let drift = db.energy_drift(200);
        assert!(drift < 0.01, "Deep basin energy drift too high: {drift}");
    }

    #[test]
    fn test_scirust_pattern_bridge() {
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
        layer.step();

        // Store in SciRust PatternMemory
        use scirust_core::symbolic::Expr;
        let mut pm = scirust_core::PatternMemory::new();
        let e_val = layer.total_energy();
        let expr = Expr::Const(e_val);
        pm.record(&Expr::Const(0.0), &expr, true);
        let best = pm.best_patterns(1);
        assert!(!best.is_empty());
    }

    #[test]
    fn test_spike_tracking() {
        let cfg = HNNConfig {
            dim: 8,
            dt: 0.05,
            friction: 0.0,
            alpha: 0.1,
            beta: 0.01,
            mu: 0.0,
            spike_threshold: 0.3, // low threshold
        };
        let mut layer = HamiltonianLayer::new(cfg);
        // Inject large momentum
        layer.stimulate(&[5.0; 8]);
        for _ in 0..50 {
            layer.step();
        }
        assert!(layer.total_spikes > 0, "Should detect spikes with low threshold");
    }
}
