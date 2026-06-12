// ==========================================================================
// projector.rs — TopologicalProjector (Pont Holonomique)
//
// MLP projection: latent space (M × d_latent) → LLM embedding space (d_llm)
// with GELU activation.  Implements deep prefix-tuning by generating
// Key/Value caches that are injected into each LLM attention layer.
//
// Phase 2 extension: apply_correction() applies a Δz regret signal
// directly onto the KV prefix, enabling hidden-state-level supervision
// without touching the LLM weights.
// ==========================================================================

use candle_core::{DType, Device, Result, Tensor};

// --------------------------------------------------------------------------
// TopologicalProjector
// --------------------------------------------------------------------------

pub struct TopologicalProjector {
    // Layer 1: d_latent → d_mid
    pub w1: Tensor,
    pub b1: Tensor,
    // Layer 2: d_mid → d_llm
    pub w2: Tensor,
    pub b2: Tensor,

    pub d_latent: usize,
    pub d_mid: usize,
    pub d_llm: usize,
    pub num_layers: usize,
    pub device: Device,
}

impl TopologicalProjector {
    pub fn new(
        d_latent: usize,
        d_mid: usize,
        d_llm: usize,
        num_layers: usize,
        device: &Device,
    ) -> Result<Self> {
        let scale1 = (2.0 / d_latent as f64).sqrt();
        let scale2 = (2.0 / d_mid as f64).sqrt();

        let w1 = Tensor::randn(0.0f64, scale1, (d_mid, d_latent), device)?;
        let b1 = Tensor::zeros(d_mid, DType::F64, device)?;
        let w2 = Tensor::randn(0.0f64, scale2, (d_llm, d_mid), device)?;
        let b2 = Tensor::zeros(d_llm, DType::F64, device)?;

        Ok(Self {
            w1, b1, w2, b2,
            d_latent, d_mid, d_llm,
            num_layers,
            device: device.clone(),
        })
    }

    /// Project latent vector (M × d_latent) → (d_llm,) embedding.
    /// Uses GELU activation between layers.
    pub fn project(&self, latents: &Tensor) -> Result<Tensor> {
        // Average over M latents → (d_latent,)
        let latent_mean = if latents.dims().len() > 1 {
            let m = latents.dims()[0];
            if m > 1 {
                latents.mean(0)?
            } else {
                latents.squeeze(0)?
            }
        } else {
            latents.clone()
        };

        // Ensure 2D for matmul
        let x = if latent_mean.dims().len() == 1 {
            latent_mean.reshape((1, self.d_latent))?
        } else {
            latent_mean
        };

        // Layer 1: h = GELU(W1 @ x + b1)
        let h = x.matmul(&self.w1.t()?)?
            .add(&self.b1.reshape((1, self.d_mid))?)?;
        let h_gelu = h.gelu()?;

        // Layer 2: embedding = W2 @ h + b2  → (1, d_llm)
        let emb = h_gelu.matmul(&self.w2.t()?)?
            .add(&self.b2.reshape((1, self.d_llm))?)?;

        Ok(emb.squeeze(0)?)  // (d_llm,)
    }

    /// Project a Δz correction signal (latent space) into LLM embedding space.
    ///
    /// This is the core of Phase 2 supervision: a regret-based correction
    /// in latent space is projected through the same MLP to produce a
    /// delta that can be applied to the KV prefix.
    ///
    /// `delta_z` — correction tensor in latent space, shape (d_latent,)
    /// Returns correction in LLM embedding space, shape (d_llm,)
    pub fn project_delta(&self, delta_z: &Tensor) -> Result<Tensor> {
        let dz_1d = if delta_z.dims().len() > 1 {
            delta_z.squeeze(0)?
        } else {
            delta_z.clone()
        };
        let x = dz_1d.reshape((1, self.d_latent))?;

        // Forward through the same MLP (linear, no activation — Δ correction)
        let h = x.matmul(&self.w1.t()?)?
            .add(&self.b1.reshape((1, self.d_mid))?)?;
        let h_gelu = h.gelu()?;
        let delta_emb = h_gelu.matmul(&self.w2.t()?)?
            .add(&self.b2.reshape((1, self.d_llm))?)?;

        Ok(delta_emb.squeeze(0)?)
    }

    /// Apply a correction delta to an existing KV prefix.
    ///
    /// Takes the output of `project_delta()` and distributes it across
    /// the per-layer K/V pairs produced by `generate_prefix_kv()`.
    ///
    /// Returns a new set of KV pairs with the correction applied.
    pub fn apply_correction(
        &self,
        kv_pairs: &[(Tensor, Tensor)],
        delta_emb: &Tensor,
        n_heads: usize,
        d_head: usize,
    ) -> Result<Vec<(Tensor, Tensor)>> {
        let kv_dim = n_heads * d_head;
        let delta_slice: Vec<f64> = delta_emb.to_vec1::<f64>()?;

        let mut corrected = Vec::with_capacity(self.num_layers);
        for layer in 0..self.num_layers {
            let (k, v) = &kv_pairs[layer];

            // Extract delta slices for this layer's K and V
            let k_start = layer * kv_dim;
            let k_end = k_start + kv_dim;
            let v_start = self.num_layers * kv_dim + layer * kv_dim;
            let v_end = v_start + kv_dim;

            let dk_data: Vec<f64> = delta_slice[k_start..k_end.min(delta_slice.len())].to_vec();
            let dv_data: Vec<f64> = delta_slice[v_start..v_end.min(delta_slice.len())].to_vec();

            // Add correction to existing K, V
            let dk = Tensor::from_slice(&dk_data, (1, 1, kv_dim), &self.device)?;
            let dv = Tensor::from_slice(&dv_data, (1, 1, kv_dim), &self.device)?;

            let k_corrected = k.add(&dk)?;
            let v_corrected = v.add(&dv)?;

            corrected.push((k_corrected, v_corrected));
        }

        Ok(corrected)
    }

    /// Generate Key/Value prefix caches for deep prefix-tuning.
    ///
    /// For each of `num_layers` attention layers, produces a (K, V) pair
    /// where K, V each have shape (1, n_prefix, d_head * n_heads).
    ///
    /// The `projected` is the projected embedding (d_llm,).  We split it
    /// into per-layer KV slices by viewing it as a contiguous buffer.
    pub fn generate_prefix_kv(
        &self,
        projected: &Tensor,
        n_heads: usize,
        d_head: usize,
    ) -> Result<Vec<(Tensor, Tensor)>> {
        let kv_dim = n_heads * d_head;
        let total_kv = self.num_layers * kv_dim * 2;

        let proj_slice: Vec<f64> = projected.to_vec1::<f64>()?;
        assert!(
            proj_slice.len() >= total_kv,
            "projected dim ({}) < total KV dim ({})",
            proj_slice.len(),
            total_kv
        );

        let mut kv_pairs = Vec::with_capacity(self.num_layers);
        for layer in 0..self.num_layers {
            let k_start = layer * kv_dim;
            let k_end = k_start + kv_dim;
            let v_start = self.num_layers * kv_dim + layer * kv_dim;
            let v_end = v_start + kv_dim;

            let k_data: Vec<f64> = proj_slice[k_start..k_end].to_vec();
            let v_data: Vec<f64> = proj_slice[v_start..v_end].to_vec();

            let k_tensor = Tensor::from_slice(
                &k_data, (1, 1, kv_dim), &self.device,
            )?;
            let v_tensor = Tensor::from_slice(
                &v_data, (1, 1, kv_dim), &self.device,
            )?;

            kv_pairs.push((k_tensor, v_tensor));
        }

        Ok(kv_pairs)
    }
}
