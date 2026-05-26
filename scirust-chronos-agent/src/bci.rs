// ==========================================================================
// bci.rs — BCI Recurrent Module (Pillar 3)
//
// Manually-coded GRU cell with retro-causal insight injection.
// When α_sync exceeds the threshold, tanh-activated insight is added
// to the candidate hidden state before the update gate.
//
// All weight matrices are pre-allocated; no dynamic allocation in forward.
// ==========================================================================

use candle_core::{DType, Device, Result, Tensor};

// --------------------------------------------------------------------------
// GRUCell — single gated recurrent unit cell
// --------------------------------------------------------------------------

pub struct GRUCell {
    // Input → hidden weights
    pub w_iz: Tensor,   // (d_hidden, d_input)
    pub w_ih: Tensor,   // (d_hidden, d_input)
    pub w_in: Tensor,   // (d_hidden, d_input)

    // Hidden → hidden weights
    pub w_hz: Tensor,   // (d_hidden, d_hidden)
    pub w_hh: Tensor,   // (d_hidden, d_hidden)
    pub w_hn: Tensor,   // (d_hidden, d_hidden)

    // Biases
    pub b_z: Tensor,    // (d_hidden,)
    pub b_h: Tensor,    // (d_hidden,)
    pub b_n: Tensor,    // (d_hidden,)

    pub d_input: usize,
    pub d_hidden: usize,
    pub device: Device,

    // Pre-allocated scratch space
    pub h_state: Tensor,
}

impl GRUCell {
    pub fn new(d_input: usize, d_hidden: usize, device: &Device) -> Result<Self> {
        let scale = 1.0 / (d_hidden as f64).sqrt();

        let w_iz = Tensor::randn(0.0f64, scale, (d_hidden, d_input), device)?;
        let w_ih = Tensor::randn(0.0f64, scale, (d_hidden, d_input), device)?;
        let w_in = Tensor::randn(0.0f64, scale, (d_hidden, d_input), device)?;
        let w_hz = Tensor::randn(0.0f64, scale, (d_hidden, d_hidden), device)?;
        let w_hh = Tensor::randn(0.0f64, scale, (d_hidden, d_hidden), device)?;
        let w_hn = Tensor::randn(0.0f64, scale, (d_hidden, d_hidden), device)?;

        let b_z = Tensor::zeros(d_hidden, DType::F64, device)?;
        let b_h = Tensor::zeros(d_hidden, DType::F64, device)?;
        let b_n = Tensor::zeros(d_hidden, DType::F64, device)?;

        let h_state = Tensor::zeros(d_hidden, DType::F64, device)?;

        Ok(Self {
            w_iz, w_ih, w_in, w_hz, w_hh, w_hn,
            b_z, b_h, b_n,
            d_input, d_hidden,
            device: device.clone(),
            h_state,
        })
    }

    /// Reset the hidden state to zero.
    pub fn reset(&mut self) -> Result<()> {
        self.h_state = Tensor::zeros(self.d_hidden, DType::F64, &self.device)?;
        Ok(())
    }

    /// Forward step:
    ///   z = σ(W_iz @ x + W_hz @ h + b_z)      — update gate
    ///   r = σ(W_ih @ x + W_hh @ h + b_h)      — reset gate
    ///   n = tanh(W_in @ x + r ⊙ W_hn @ h + b_n) — candidate
    ///   h' = (1 - z) ⊙ n + z ⊙ h              — new hidden state
    ///
    /// If α_sync > threshold, inject tanh(α_sync) into n before the gate.
    pub fn step(&mut self, x: &Tensor, alpha_sync: f64, threshold: f64) -> Result<Tensor> {
        // Ensure inputs are 2D (candle matmul requires 2D+ tensors)
        let x_2d = if x.dims().len() == 1 {
            x.reshape((1, self.d_input))?
        } else {
            x.clone()
        };
        let h_2d = if self.h_state.dims().len() == 1 {
            self.h_state.reshape((1, self.d_hidden))?
        } else {
            self.h_state.clone()
        };

        // Gates
        let pre_z = x_2d.matmul(&self.w_iz.t()?)?
            .add(&h_2d.matmul(&self.w_hz.t()?)?)?
            .add(&self.b_z.reshape((1, self.d_hidden))?)?;
        let z = candle_nn::ops::sigmoid(&pre_z)?;

        let pre_r = x_2d.matmul(&self.w_ih.t()?)?
            .add(&h_2d.matmul(&self.w_hh.t()?)?)?
            .add(&self.b_h.reshape((1, self.d_hidden))?)?;
        let r = candle_nn::ops::sigmoid(&pre_r)?;

        // Candidate
        let mut pre_n = x_2d.matmul(&self.w_in.t()?)?;
        let rnn = r.broadcast_mul(&h_2d.matmul(&self.w_hn.t()?)?)?;
        pre_n = pre_n.add(&rnn)?.add(&self.b_n.reshape((1, self.d_hidden))?)?;

        // Retro-causal insight injection
        if alpha_sync > threshold {
            let insight = Tensor::new(alpha_sync.tanh(), &self.device)?
                .broadcast_as(pre_n.shape())?;
            pre_n = pre_n.add(&insight)?;
        }

        let n = pre_n.tanh()?;

        // Update: h' = (1 - z) * n + z * h
        let one_row = Tensor::new(1.0f64, &self.device)?.reshape((1, 1))?;
        let h_new_2d = one_row.broadcast_sub(&z)?.broadcast_mul(&n)?
            .add(&z.broadcast_mul(&h_2d)?)?;

        // Store hidden state as 1D for next call (compatibility)
        let h_new = h_new_2d.squeeze(0)?;
        self.h_state = h_new.clone();
        Ok(h_new)
    }

}
