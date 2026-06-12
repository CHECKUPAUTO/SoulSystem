// ==========================================================================
// hypernetwork.rs — ConditioningHyperNetwork (Pillar 5)
//
// A two-layer MLP that takes the absolute prediction error e_t and returns
// a latent-space offset vector Δz.  This adjusts the topology of the latent
// space in response to surprise/misprediction.
//
// All weights are pre-allocated; forward uses in-place scratch tensors.
// ==========================================================================

use candle_core::{DType, Device, Result, Tensor};

// --------------------------------------------------------------------------
// ConditioningHyperNetwork
// --------------------------------------------------------------------------

pub struct ConditioningHyperNetwork {
    // Layer 1: d_input → d_hidden
    pub w1: Tensor,
    pub b1: Tensor,
    // Layer 2: d_hidden → d_output
    pub w2: Tensor,
    pub b2: Tensor,

    pub d_input: usize,
    pub d_hidden: usize,
    pub d_output: usize,
    pub device: Device,

    // Pre-allocated scratch
    pub scratch_hidden: Tensor,
    pub scratch_output: Tensor,
}

impl ConditioningHyperNetwork {
    pub fn new(
        d_input: usize,
        d_hidden: usize,
        d_output: usize,
        device: &Device,
    ) -> Result<Self> {
        let scale1 = (2.0 / d_input as f64).sqrt();
        let scale2 = (2.0 / d_hidden as f64).sqrt();

        let w1 = Tensor::randn(0.0f64, scale1, (d_hidden, d_input), device)?;
        let b1 = Tensor::zeros(d_hidden, DType::F64, device)?;
        let w2 = Tensor::randn(0.0f64, scale2, (d_output, d_hidden), device)?;
        let b2 = Tensor::zeros(d_output, DType::F64, device)?;

        let scratch_hidden = Tensor::zeros(d_hidden, DType::F64, device)?;
        let scratch_output = Tensor::zeros(d_output, DType::F64, device)?;

        Ok(Self {
            w1, b1, w2, b2,
            d_input, d_hidden, d_output,
            device: device.clone(),
            scratch_hidden, scratch_output,
        })
    }

    /// Forward: Δz = W2 @ ReLU(W1 @ e + b1) + b2
    ///
    /// `e` is a scalar error (f64) — broadcast to input dimension.
    pub fn forward(&mut self, e_t: f64) -> Result<Tensor> {
        // Broadcast error to input vector
        let e_data: Vec<f64> = (0..self.d_input).map(|_| e_t).collect();
        let e_vec = Tensor::from_slice(&e_data, (1, self.d_input), &self.device)?;  // (1, d_input)

        // Layer 1: h = ReLU(W1 @ e + b1)
        let h = e_vec.matmul(&self.w1.t()?)?.add(&self.b1.reshape((1, self.d_hidden))?)?;
        let h_relu = h.relu()?;

        // Layer 2: Δz = W2 @ h + b2
        let dz = h_relu.matmul(&self.w2.t()?)?.add(&self.b2.reshape((1, self.d_output))?)?;

        Ok(dz)
    }

    /// Apply plasticity shift: modulate weights by dissonance-scaled noise.
    /// This is the core evolutionary mechanism — when metacognitive dissonance
    /// is high, the network expands its representational capacity by jittering
    /// weights proportionally to the dissonance signal.
    pub fn apply_plasticity_shift(&mut self, dissonance: f64) -> Result<()> {
        let noise_scale = dissonance.clamp(0.0, 1.0) * 0.1;
        let mut rng = rand::thread_rng();
        use rand::Rng;
        // Add small noise to layer 1 weights
        let w1_data: Vec<f64> = self.w1.flatten_all()?.to_vec1::<f64>()?;
        let w1_shifted: Vec<f64> = w1_data.iter().map(|w| w + rng.gen::<f64>() * noise_scale - noise_scale * 0.5).collect();
        self.w1 = Tensor::from_slice(&w1_shifted, (self.d_hidden, self.d_input), &self.device)?;
        // Also shift biases
        let b1_data: Vec<f64> = self.b1.to_vec1::<f64>()?;
        let b1_shifted: Vec<f64> = b1_data.iter().map(|b| b + rng.gen::<f64>() * noise_scale * 0.5 - noise_scale * 0.25).collect();
        self.b1 = Tensor::from_slice(&b1_shifted, self.d_hidden, &self.device)?;
        Ok(())
    }
}
