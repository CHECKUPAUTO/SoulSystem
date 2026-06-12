// ==========================================================================
// learning.rs — RegretOptimizer (Apprentissage A Posteriori)
//
// Computes divergence between LLM token logits and the ideal spatiotemporal
// trajectory from StochasticDiffusionPlanner.
//
// When regret exceeds threshold, generates Δz feedback signal to
// HyperNetwork for immediate prefix vector correction.
// ==========================================================================

use candle_core::{Device, Result, Tensor};

// --------------------------------------------------------------------------
// RegretOptimizer
// --------------------------------------------------------------------------

pub struct RegretOptimizer {
    pub threshold: f64,
    pub d_latent: usize,
    pub device: Device,
}

impl RegretOptimizer {
    pub fn new(threshold: f64, d_latent: usize, device: &Device) -> Self {
        Self { threshold, d_latent, device: device.clone() }
    }

    /// Compute regret loss: MSE(logits_llm, trajectory_ideal).
    ///
    /// `logits_llm` — raw output logits from the LLM, shape (vocab_size,) or (1, vocab_size)
    /// `trajectory_ideal` — ideal latent trajectory from planner, shape (d_latent,)
    ///
    /// Returns (L_regret, delta_z) where delta_z is the correction signal
    /// to send to HyperNetwork if L_regret > threshold.
    pub fn compute(
        &self,
        logits_llm: &Tensor,
        trajectory_ideal: &Tensor,
    ) -> Result<(f64, Option<Tensor>)> {
        // Handle shape differences: reduce logits to (d_latent,) via softmax-weighted pool
        let logits_1d = if logits_llm.dims().len() > 1 {
            logits_llm.squeeze(0)?
        } else {
            logits_llm.clone()
        };

        let traj_1d = if trajectory_ideal.dims().len() > 1 {
            trajectory_ideal.squeeze(0)?
        } else {
            trajectory_ideal.clone()
        };

        // Both tensors may have different dimensionalities (logits ~ vocab_size,
        // trajectory ~ d_latent).  We project both to a common comparison space
        // by taking the softmax over logits and computing the mean-weighted pooled
        // representation.  For simplicity, we downsample logits to match d_latent
        // via strided mean pooling.
        let logits_size = logits_1d.dims1()?;
        let step = (logits_size / self.d_latent).max(1);

        let pooled_logits = if logits_size > self.d_latent {
            // Strided mean pooling
            let mut pooled: Vec<f64> = Vec::with_capacity(self.d_latent);
            let logits_slice: Vec<f64> = logits_1d.to_vec1::<f64>()?;
            for i in 0..self.d_latent {
                let start = i * step;
                let end = ((i + 1) * step).min(logits_size);
                let mean = logits_slice[start..end].iter().sum::<f64>()
                    / (end - start) as f64;
                pooled.push(mean);
            }
            Tensor::from_slice(&pooled, self.d_latent, &self.device)?
        } else {
            // Pad with zeros
            let mut data = logits_1d.to_vec1::<f64>()?;
            data.resize(self.d_latent, 0.0);
            Tensor::from_slice(&data, self.d_latent, &self.device)?
        };

        // MSE between pooled logits and trajectory
        let diff = pooled_logits.sub(&traj_1d)?;
        let sq_err = diff.sqr()?;
        let mse_tensor = sq_err.mean_all()?;
        let mse: f64 = mse_tensor.to_scalar::<f64>()?;

        // Generate correction signal if threshold exceeded
        let delta_z = if mse > self.threshold {
            // Δz = -∇(L_regret) = -(pooled - traj) * 2/n  (gradient of MSE)
            let n = self.d_latent as f64;
            let grad = diff.broadcast_mul(&Tensor::new(2.0 / n, &self.device)?)?;
            Some(grad.neg()?)
        } else {
            None
        };

        Ok((mse, delta_z))
    }
}
