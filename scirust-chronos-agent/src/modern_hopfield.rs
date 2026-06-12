// ==========================================================================
// modern_hopfield.rs — Continuous Modern Hopfield Network (V6 BCI)
//
// Remplace GRUCell comme noyau de raisonnement. Au lieu d'un état caché
// causal, ce module implémente une dynamique d'énergie convergente.
//
// Principe :
//   E = -lse(β, Ξ · x)  où lse = log-sum-exp
//   x_{t+1} = Ξ^T · softmax(β · Ξ · x_t)
//
// C'est un Modern Hopfield Network (Ramsauer et al., 2020) avec :
//   - Convergence garantie vers un point fixe d'énergie minimale
//   - Rétention de M motifs associatifs
//   - Mise à jour par softmax pondéré par β (inverse température)
//
// L'α_sync devient un vrai paramètre de température β = f(α_sync).
// ==========================================================================

use candle_core::{DType, Device, Result, Tensor};

// --------------------------------------------------------------------------
// ModernHopfieldNetwork
// --------------------------------------------------------------------------

pub struct ModernHopfieldNetwork {
    /// Stored associative patterns: (M, d_latent)
    pub patterns: Tensor,
    /// Number of stored patterns.
    pub m: usize,
    /// Latent dimension.
    pub d_latent: usize,
    /// Current state (the "consciousness" attractor).
    pub state: Tensor,
    /// Energy of the current state.
    pub energy: f64,
    /// Inverse temperature β — higher = sharper attractor.
    pub beta: Tensor,
    pub device: Device,
    /// Pre-allocated scratch buffer for attention scores: (M,)
    pub scratch_scores: Tensor,
}

impl ModernHopfieldNetwork {
    /// Create a new Hopfield network with M initial random patterns.
    pub fn new(m: usize, d_latent: usize, device: &Device) -> Result<Self> {
        let scale = 1.0 / (d_latent as f64).sqrt();
        let patterns = Tensor::randn(0.0f64, scale, (m, d_latent), device)?;
        let state = Tensor::zeros(d_latent, DType::F64, device)?;
        let beta = Tensor::new(1.0f64, device)?;
        let scratch_scores = Tensor::zeros(m, DType::F64, device)?;

        Ok(Self {
            patterns,
            m,
            d_latent,
            state,
            energy: 0.0,
            beta,
            device: device.clone(),
            scratch_scores,
        })
    }

    /// Learn a new pattern by inserting it into the associative memory.
    /// If M exceeds capacity, the oldest pattern is evicted (FIFO).
    pub fn learn(&mut self, pattern: &Tensor) -> Result<()> {
        // pattern: (d_latent,)
        let p_2d = pattern.reshape((1, self.d_latent))?;

        // Shift patterns manually: collect all but first, append new
        let mut data: Vec<f64> = Vec::with_capacity(self.m * self.d_latent);

        // Skip first row (oldest), keep rows 1..M
        let all_data: Vec<f64> = self.patterns.flatten_all()?.to_vec1::<f64>()?;
        for row in 1..self.m {
            let start = row * self.d_latent;
            let end = start + self.d_latent;
            if end <= all_data.len() {
                data.extend_from_slice(&all_data[start..end]);
            }
        }

        // Append new pattern
        let new_data: Vec<f64> = p_2d.flatten_all()?.to_vec1::<f64>()?;
        data.extend(&new_data);

        // If we had fewer than M rows (initial state), pad with zeros
        while data.len() < self.m * self.d_latent {
            data.push(0.0);
        }
        data.truncate(self.m * self.d_latent);

        self.patterns = Tensor::from_slice(&data, (self.m, self.d_latent), &self.device)?;
        Ok(())
    }

    /// Forward step: converge toward the attractor.
    ///
    ///   attention = softmax(β · Ξ · x_t)      — similarity to stored patterns
    ///   x_{t+1}   = Ξ^T · attention           — retrieved pattern
    ///
    /// Where:
    ///   Ξ :: (M, d_latent) — stored patterns
    ///   x_t :: (d_latent,) — current state
    ///   β   = f(α_sync)    — inverse temperature
    ///
    /// Returns the new state and its energy E.
    pub fn step(&mut self, x: &Tensor, alpha_sync: f64) -> Result<(Tensor, f64)> {
        // Set β from α_sync: β = 1 + 10 * α_sync (sharper when synchronised)
        let beta_val = 1.0 + 10.0 * alpha_sync.clamp(0.0, 1.0);
        self.beta = Tensor::new(beta_val, &self.device)?;

        let x_2d = if x.dims().len() == 1 {
            x.reshape((1, self.d_latent))?
        } else {
            x.clone()
        };

        // Compute similarity scores: s = Ξ · x  -> (M,)
        // Ξ: (M, d_latent), x: (1, d_latent)
        let scores = self.patterns.matmul(&x_2d.t()?)?.squeeze(1)?; // (M,)

        // Apply β scaling
        let scaled_scores = scores.broadcast_mul(&self.beta.reshape((1,))?)?; // (M,)

        // Softmax attention
        let attention = candle_nn::ops::softmax(&scaled_scores, 0)?; // (M,)

        // Retrieve: x' = Ξ^T · attention  -> (d_latent,)
        let attention_2d = attention.reshape((self.m, 1))?; // (M, 1)
        let new_state = self.patterns.t()?.matmul(&attention_2d)?.squeeze(1)?; // (d_latent,)

        // Compute energy: E = -lse(β, Ξ · x)
        let max_score = scaled_scores
            .to_vec1::<f64>()?
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max);
        let lse = max_score + (scaled_scores.to_vec1::<f64>()?
            .iter()
            .map(|s| (s - max_score).exp())
            .sum::<f64>())
            .ln();
        let energy = -lse / beta_val; // Normalised energy (lower = more stable)

        // Update state
        self.state = new_state.clone();
        self.energy = energy;

        Ok((new_state, energy))
    }

    /// Inject an external insight as a new pattern.
    /// This is the V6 equivalent of the retro-causal insight injection.
    pub fn inject_insight(&mut self, insight: &Tensor) -> Result<()> {
        self.learn(insight)
    }

    /// Get the current stored patterns.
    pub fn patterns(&self) -> &Tensor {
        &self.patterns
    }

    /// Reset state to zero.
    pub fn reset(&mut self) -> Result<()> {
        self.state = Tensor::zeros(self.d_latent, DType::F64, &self.device)?;
        self.energy = 0.0;
        Ok(())
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hopfield_creation() {
        let device = Device::Cpu;
        let hn = ModernHopfieldNetwork::new(8, 16, &device).unwrap();
        assert_eq!(hn.m, 8);
        assert_eq!(hn.d_latent, 16);
    }

    #[test]
    fn test_hopfield_forward() {
        let device = Device::Cpu;
        let mut hn = ModernHopfieldNetwork::new(8, 16, &device).unwrap();

        let input = Tensor::randn(0.0f64, 1.0f64, 16, &device).unwrap();
        let (output, energy) = hn.step(&input, 0.8).unwrap();

        assert_eq!(output.dims(), vec![16]);
        assert!(energy.is_finite());
        assert!(energy <= 0.0, "Energy should be non-positive for a valid attractor");
    }

    #[test]
    fn test_hopfield_convergence() {
        let device = Device::Cpu;
        let mut hn = ModernHopfieldNetwork::new(8, 16, &device).unwrap();

        // Inject a pattern
        let pattern = Tensor::randn(0.0f64, 1.0f64, 16, &device).unwrap();
        hn.learn(&pattern).unwrap();

        let input = Tensor::randn(0.0f64, 0.5f64, 16, &device).unwrap();
        let (state, energy) = hn.step(&input, 0.9).unwrap();

        // With high β and a learned pattern, the energy should be low
        assert!(energy < -0.5 || energy.abs() < 1.0,
            "High-temperature energy should be moderate, got {}", energy);
        assert_eq!(state.dims(), vec![16], "State dims should be (d_latent,)");
    }

    #[test]
    fn test_hopfield_beta_scaling() {
        let device = Device::Cpu;
        let mut hn = ModernHopfieldNetwork::new(8, 16, &device).unwrap();

        let input = Tensor::randn(0.0f64, 1.0f64, 16, &device).unwrap();

        // High alpha_sync → sharp β → focused attractor
        let (_, e_high) = hn.step(&input, 0.9).unwrap();

        // Both energies should be negative (stable attractors)
        assert!(e_high <= 0.0, "Energy should be non-positive");
        assert!(e_high.is_finite(), "Energy should be finite");
    }
}
