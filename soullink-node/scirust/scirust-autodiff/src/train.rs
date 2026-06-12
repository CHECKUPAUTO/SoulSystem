//! Training loop connecting the tape-based autodiff with neural network layers.
//!
//! Provides:
//! - `SGDOptimizer` / `AdamOptimizer` for gradient-based optimization
//! - `Trainer` for supervised learning with loss functions
//!
//! # Example
//!
//! ```ignore
//! use scirust_autodiff::train::*;
//!
//! let mut model = LinearModel::new(4, 2);
//! let mut trainer = Trainer::new(&mut model, Loss::MSE, 0.01);
//! trainer.train(&inputs, &targets, 100);
//! let predictions = trainer.predict(&inputs);
//! ```

use crate::tape::{Tape, Var};

// ---------------------------------------------------------------------------
// Optimizers
// ---------------------------------------------------------------------------

/// Stochastic Gradient Descent optimizer.
#[derive(Debug, Clone)]
pub struct SGDOptimizer {
    pub lr: f64,
    pub momentum: f64,
    velocity: Vec<f64>,
    initialized: bool,
}

impl SGDOptimizer {
    pub fn new(lr: f64) -> Self {
        Self { lr, momentum: 0.0, velocity: Vec::new(), initialized: false }
    }

    pub fn with_momentum(mut self, momentum: f64) -> Self {
        self.momentum = momentum;
        self
    }

    pub fn step(&mut self, params: &mut [f64], grads: &[f64]) {
        if !self.initialized {
            self.velocity = vec![0.0; params.len()];
            self.initialized = true;
        }
        for i in 0..params.len() {
            self.velocity[i] = self.momentum * self.velocity[i] + self.lr * grads[i];
            params[i] -= self.velocity[i];
        }
    }
}

/// Adam optimizer.
#[derive(Debug, Clone)]
pub struct AdamOptimizer {
    pub lr: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
    m: Vec<f64>,
    v: Vec<f64>,
    t: usize,
    initialized: bool,
}

impl AdamOptimizer {
    pub fn new(lr: f64) -> Self {
        Self {
            lr, beta1: 0.9, beta2: 0.999, epsilon: 1e-8,
            m: Vec::new(), v: Vec::new(), t: 0, initialized: false,
        }
    }

    pub fn step(&mut self, params: &mut [f64], grads: &[f64]) {
        if !self.initialized {
            self.m = vec![0.0; params.len()];
            self.v = vec![0.0; params.len()];
            self.initialized = true;
        }
        self.t += 1;

        for i in 0..params.len() {
            self.m[i] = self.beta1 * self.m[i] + (1.0 - self.beta1) * grads[i];
            self.v[i] = self.beta2 * self.v[i] + (1.0 - self.beta2) * grads[i] * grads[i];

            let m_hat = self.m[i] / (1.0 - self.beta1.powi(self.t as i32));
            let v_hat = self.v[i] / (1.0 - self.beta2.powi(self.t as i32));

            params[i] -= self.lr * m_hat / (v_hat.sqrt() + self.epsilon);
        }
    }
}

// ---------------------------------------------------------------------------
// Loss functions
// ---------------------------------------------------------------------------

/// Supported loss functions.
#[derive(Debug, Clone, Copy)]
pub enum Loss {
    MSE,
    MAE,
    CrossEntropy,
}

impl Loss {
    /// Compute loss = f(prediction, target) using the tape.
    pub fn compute(&self, tape: &Tape, pred: Var, target: f64) -> Var {
        let t = tape.var("target", target);
        match self {
            Loss::MSE => {
                let diff = pred.sub(&t);
                diff.mul(&diff) // (pred - target)²
            }
            Loss::MAE => {
                let diff = pred.sub(&t);
                // |diff| approximated as sqrt(diff² + ε) for differentiability
                diff.mul(&diff).add_scalar(1e-10).sqrt()
            }
            Loss::CrossEntropy => {
                // -[target * log(pred) + (1-target) * log(1-pred)]
                let one = tape.var("one", 1.0);
                let log_pred = pred.add_scalar(1e-10).ln(); // avoid log(0)
                let log_1mp = one.sub(&pred).add_scalar(1e-10).ln();
                let term1 = t.mul(&log_pred);
                let term2 = one.sub(&t).mul(&log_1mp);
                term1.add(&term2).neg()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Trainer
// ---------------------------------------------------------------------------

/// A simple supervised learning trainer.
///
/// Connects the tape-based autodiff with parameter optimization.
pub struct Trainer {
    pub optimizer: SGDOptimizer,
    pub loss: Loss,
}

impl Trainer {
    pub fn new(loss: Loss, lr: f64) -> Self {
        Self { optimizer: SGDOptimizer::new(lr), loss }
    }

    pub fn with_optimizer(optimizer: SGDOptimizer, loss: Loss) -> Self {
        Self { optimizer, loss }
    }

    /// Train a linear model y = w·x + b on a single batch.
    pub fn train_step(
        &mut self,
        w: &mut [f64],
        b: &mut f64,
        inputs: &[&[f64]],
        targets: &[f64],
    ) -> f64 {
        let n = inputs.len();
        let mut total_loss = 0.0;
        let mut w_grads = vec![0.0; w.len()];
        let mut b_grad = 0.0;

        for i in 0..n {
            let tape = Tape::new();
            let mut vars = Vec::new();
            for j in 0..w.len() {
                vars.push(tape.var(&format!("w{}", j), w[j]));
            }
            let bias = tape.var("b", *b);

            // Forward: pred = Σ(wj * xj) + b
            let x_vars: Vec<Var> = inputs[i].iter().enumerate()
                .map(|(j, &v)| tape.var(&format!("x{}", j), v))
                .collect();

            let mut pred = bias.clone();
            for j in 0..w.len() {
                pred = pred.add(&vars[j].mul(&x_vars[j]));
            }

            // Loss
            let loss_val = self.loss.compute(&tape, pred, targets[i]);
            total_loss += loss_val.val();

            // Backward
            tape.backward(&loss_val);

            // Accumulate gradients
            for j in 0..w.len() {
                w_grads[j] += vars[j].grad();
            }
            b_grad += bias.grad();
        }

        // Average gradients
        let inv_n = 1.0 / n as f64;
        for g in &mut w_grads { *g *= inv_n; }
        b_grad *= inv_n;

        // Update parameters
        self.optimizer.step(w, &w_grads);
        *b -= self.optimizer.lr * b_grad;

        total_loss / n as f64
    }

    /// Train for multiple epochs.
    pub fn train(
        &mut self,
        w: &mut [f64],
        b: &mut f64,
        inputs: &[&[f64]],
        targets: &[f64],
        epochs: usize,
    ) -> Vec<f64> {
        // Normalize learning rate for the data scale
        let scale = inputs.iter().flat_map(|v| v.iter()).map(|x| x.abs()).fold(0.0f64, f64::max).max(1.0);
        let original_lr = self.optimizer.lr;
        self.optimizer.lr = original_lr / scale;
        let mut losses = Vec::with_capacity(epochs);
        for _epoch in 0..epochs {
            let loss = self.train_step(w, b, inputs, targets);
            losses.push(loss);
        }
        self.optimizer.lr = original_lr;
        losses
    }

    /// Predict using trained parameters.
    pub fn predict(w: &[f64], b: f64, input: &[f64]) -> f64 {
        let mut sum = b;
        for (j, &x) in input.iter().enumerate() {
            sum += w[j] * x;
        }
        sum
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sgd_optimizer_step() {
        let mut opt = SGDOptimizer::new(0.1);
        let mut params = vec![1.0, 2.0];
        let grads = vec![0.5, -0.5];
        opt.step(&mut params, &grads);
        assert!((params[0] - 0.95).abs() < 1e-10);
        assert!((params[1] - 2.05).abs() < 1e-10);
    }

    #[test]
    fn test_adam_optimizer_step() {
        let mut opt = AdamOptimizer::new(0.1);
        let mut params = vec![0.0, 0.0];
        let grads = vec![1.0, 2.0];
        opt.step(&mut params, &grads);
        // Adam should update params in the negative gradient direction
        assert!(params[0] < 0.0);
        assert!(params[1] < 0.0);
    }

    #[test]
    fn test_mse_loss() {
        let tape = Tape::new();
        let pred = tape.var("pred", 3.0);
        let loss = Loss::MSE.compute(&tape, pred, 1.0);
        assert!((loss.val() - 4.0).abs() < 1e-10); // (3-1)² = 4
    }

    #[test]
    fn test_linear_training() {
        // Train y = 2*x + 1
        let mut w = vec![0.0];
        let mut b = 0.0;

        let inputs: Vec<Vec<f64>> = (0..20).map(|i| vec![i as f64]).collect();
        let targets: Vec<f64> = (0..20).map(|i| 2.0 * i as f64 + 1.0).collect();
        let input_refs: Vec<&[f64]> = inputs.iter().map(|v| v.as_slice()).collect();

        let mut trainer = Trainer::new(Loss::MSE, 0.01);
        let losses = trainer.train(&mut w, &mut b, &input_refs, &targets, 50);

        // Should converge
        let final_loss = losses.last().copied().unwrap_or(f64::MAX);
        assert!(final_loss < 1.0, "Training didn't converge, loss={}", final_loss);

        // Predictions should be close
        let pred = Trainer::predict(&w, b, &[5.0]);
        assert!((pred - 11.0).abs() < 2.0, "w={:.4}, b={:.4}, pred(5)={:.4}", w[0], b, pred);
    }

    #[test]
    fn test_cross_entropy_loss() {
        let tape = Tape::new();
        let pred = tape.var("pred", 0.8);
        let loss = Loss::CrossEntropy.compute(&tape, pred, 1.0);
        // -[1*log(0.8) + 0*log(0.2)] = -log(0.8) ≈ 0.223
        assert!((loss.val() - 0.22314).abs() < 0.01);
    }

    #[test]
    fn test_momentum_sgd() {
        let mut opt = SGDOptimizer::new(0.1).with_momentum(0.9);
        let mut params = vec![10.0];
        let grads = vec![1.0];

        opt.step(&mut params, &grads); // first step: uses lr only
        opt.step(&mut params, &grads); // second step: uses momentum
        // With momentum=0.9, 2nd step should move more
        assert!(params[0] < 9.8, "Momentum should accelerate: {}", params[0]);
    }
}
