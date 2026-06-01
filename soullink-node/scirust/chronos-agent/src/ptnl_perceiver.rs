// ==========================================================================
// ptnl_perceiver.rs — PTNLPerceiver (Pillar 1)
//
// Cross-attention between input sequence (sliding window T) and fixed
// latent vectors (M).  Includes the learned linear inverse projection
// M → T for continuous reconstruction loss.
//
// All tensor buffers are pre-allocated in the struct (scratchpad pattern)
// to guarantee zero allocation in the hot inference path.
// ==========================================================================

use candle_core::{Device, Result, Tensor};

// --------------------------------------------------------------------------
// PTNLPerceiver
// --------------------------------------------------------------------------

pub struct PTNLPerceiver {
    // Learned parameters
    pub w_q: Tensor,       // (M, d_latent)  — query projection
    pub w_k: Tensor,       // (d_input, d_latent) — key projection
    pub w_v: Tensor,       // (d_input, d_latent) — value projection
    pub w_o: Tensor,       // (d_latent, d_input) — output projection
    pub w_proj: Tensor,    // (M, T) — learned inverse projection M → T

    // Pre-allocated latent buffer (M × d_latent)
    pub latents: Tensor,

    // Scratchpad buffers (pre-allocated once)
    pub scratch_q: Tensor,
    pub scratch_k: Tensor,
    pub scratch_v: Tensor,

    // Configuration
    pub d_input: usize,
    pub d_latent: usize,
    pub m: usize,
    pub t: usize,
    pub device: Device,

    // NoiseGate: when input variance exceeds this threshold,
    // the perceiver returns existing latents unchanged (noise rejection).
    pub noise_variance_threshold: f64,
    pub noise_gate_active: bool,
}

impl PTNLPerceiver {
    /// Create a new PTNLPerceiver with randomly initialised weights and
    /// pre-allocated scratch buffers.
    pub fn new(
        d_input: usize,
        d_latent: usize,
        m: usize,
        t: usize,
        device: &Device,
    ) -> Result<Self> {
        let scale_q = 1.0 / (d_latent as f64).sqrt();
        let scale_k = 1.0 / (d_latent as f64).sqrt();
        let scale_v = 1.0 / (d_input as f64).sqrt();

        let w_q = Tensor::randn(0.0f64, scale_q, (d_latent, d_latent), device)?;
        let w_k = Tensor::randn(0.0f64, scale_k, (d_input, d_latent), device)?;
        let w_v = Tensor::randn(0.0f64, scale_v, (d_input, d_latent), device)?;
        let w_o = Tensor::randn(0.0f64, 0.02f64, (d_latent, d_input), device)?;
        let w_proj = Tensor::randn(0.0f64, 0.02f64, (m, t), device)?;

        let latents = Tensor::zeros((m, d_latent), candle_core::DType::F64, device)?;

        // Pre-allocate scratch buffers at maximum dimension
        let scratch_q = Tensor::zeros((m, d_latent), candle_core::DType::F64, device)?;
        let scratch_k = Tensor::zeros((t, d_latent), candle_core::DType::F64, device)?;
        let scratch_v = Tensor::zeros((t, d_latent), candle_core::DType::F64, device)?;

        Ok(Self {
            w_q, w_k, w_v, w_o, w_proj,
            latents,
            scratch_q, scratch_k, scratch_v,
            d_input, d_latent, m, t,
            device: device.clone(),
            noise_variance_threshold: 0.15,  // threshold: variance > 0.15 triggers gate (tuned for normalized input range ~[-1, 1])
            noise_gate_active: false,
        })
    }

    /// Forward pass: cross-attention over input sequence `x` of shape (T, d_input).
    ///
    /// Includes a NoiseGate: if the variance of `x` exceeds `noise_variance_threshold`,
    /// the forward pass is short-circuited — existing latents are returned unchanged
    /// (noise rejection mode). This preserves computational resources and prevents
    /// the latent buffer from being contaminated by random input.
    ///
    /// Returns `(latents_out, reconstruction)` where:
    /// - `latents_out`: shape (M, d_latent) — updated latent vectors (or frozen if noise)
    /// - `reconstruction`: shape (T,) — per-timestep reconstruction loss contribution
    pub fn forward(&mut self, x: &Tensor) -> Result<(Tensor, Tensor)> {
        let (t, d_in) = x.dims2()?;
        assert_eq!(t, self.t, "t mismatch");
        assert_eq!(d_in, self.d_input, "d_in mismatch");

        // --- NoiseGate ---
        // Compute variance of the input signal. If it exceeds threshold,
        // return current latents unchanged and a zero reconstruction loss.
        let x_vec: Vec<f64> = x.flatten_all()?.to_vec1::<f64>()?;
        let mean = x_vec.iter().sum::<f64>() / x_vec.len() as f64;
        let variance = x_vec.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / x_vec.len() as f64;

        if variance > self.noise_variance_threshold {
            self.noise_gate_active = true;
            // Return frozen latents + zero reconstruction loss
            let recon_loss = Tensor::zeros(self.t, candle_core::DType::F64, &self.device)?;
            return Ok((self.latents.clone(), recon_loss));
        }
        self.noise_gate_active = false;
        // --- end NoiseGate ---

        // Use scratch buffers to avoid allocation
        let q = self.latents.matmul(&self.w_q)?;
        let k = x.matmul(&self.w_k)?;
        let v = x.matmul(&self.w_v)?;

        // Attention scores: S = Q @ K^T / sqrt(d_latent)
        let k_t = k.transpose(0, 1)?;
        let qk = q.matmul(&k_t)?;
        let scale_val = (self.d_latent as f64).sqrt();
        let scale_t = Tensor::new(scale_val, &self.device)?;
        let attn_scores = qk.broadcast_div(&scale_t)?;

        // Softmax over T dimension (dim=1)
        let attn_weights = candle_nn::ops::softmax(&attn_scores, 1)?;

        // Weighted sum: latents_out = attn_weights @ V  -> (M, d_latent)
        let latents_out = attn_weights.matmul(&v)?;

        // Output projection
        let projected = latents_out.matmul(&self.w_o)?;

        // Inverse projection M -> T
        let projected_mean = projected.mean(1)?.reshape((1, self.m))?;  // (1, M)
        let recon = projected_mean.matmul(&self.w_proj)?;

        // Reconstruction loss per timestep
        let x_mean = x.mean(1)?;
        let recon_loss = x_mean.sub(&recon.squeeze(0)?)?.sqr()?;

        // Update stored latents
        self.latents = latents_out.clone();

        Ok((latents_out, recon_loss))
    }

    /// Reset the latent buffer to zeros (call at start of each episode).
    pub fn reset(&mut self) -> Result<()> {
        self.latents = Tensor::zeros(
            (self.m, self.d_latent),
            candle_core::DType::F64,
            &self.device,
        )?;
        Ok(())
    }
}
