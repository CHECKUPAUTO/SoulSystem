// ==========================================================================
// planner.rs — StochasticDiffusionPlanner (Pillar 4)
//
// Reverse denoising loop with:
//   - Strict inpainting: timestep-0 overridden by the injected current state
//   - Classifier-Free Guidance (CFG): linear extrapolation from a NULL
//     embedding (persistent in memory)
//   - Temperature decay: injected noise amplitude ∝ α_sync (linearly)
//
// Pre-allocated noise trajectory buffer for zero-allocation sampling.
// ==========================================================================

use candle_core::{DType, Device, Result, Tensor};
use rand::Rng;

// --------------------------------------------------------------------------
// StochasticDiffusionPlanner
// --------------------------------------------------------------------------

pub struct StochasticDiffusionPlanner {
    /// Number of diffusion steps.
    pub num_steps: usize,
    /// Latent dimension.
    pub d_latent: usize,
    /// NULL embedding for CFG (persistent).
    pub null_embed: Tensor,
    /// Pre-allocated noise trajectory: (num_steps, d_latent)
    pub noise_buffer: Tensor,
    /// Pre-allocated working trajectory: (num_steps, d_latent)
    pub trajectory: Tensor,
    pub device: Device,
}

impl StochasticDiffusionPlanner {
    pub fn new(num_steps: usize, d_latent: usize, device: &Device) -> Result<Self> {
        let null_embed = Tensor::zeros(d_latent, DType::F64, device)?;
        let noise_buffer = Tensor::zeros((num_steps, d_latent), DType::F64, device)?;
        let trajectory = Tensor::zeros((num_steps, d_latent), DType::F64, device)?;

        Ok(Self {
            num_steps,
            d_latent,
            null_embed,
            noise_buffer,
            trajectory,
            device: device.clone(),
        })
    }

    /// Sample a denoised trajectory conditioned on `latent_state`.
    ///
    /// Args:
    ///   latent_state — current latent vector, shape (d_latent,)
    ///   alpha_sync   — synchrony index for temperature modulation
    ///
    /// Returns trajectory tensor of shape (num_steps, d_latent).
    pub fn plan(&mut self, latent_state: &Tensor, alpha_sync: f64) -> Result<Tensor> {
        let mut rng = rand::thread_rng();

        // 1. Fill noise buffer with Gaussian noise (temperature ∝ α_sync)
        let temp = (1.0 - alpha_sync).clamp(0.01, 1.0);
        let mut noise_data: Vec<f64> = Vec::with_capacity(self.num_steps * self.d_latent);
        for _ in 0..self.num_steps * self.d_latent {
            let z: f64 = rng.sample(rand_distr::StandardNormal);
            noise_data.push(z * temp);
        }
        let noise = Tensor::from_slice(
            &noise_data,
            (self.num_steps, self.d_latent),
            &self.device,
        )?;

        // 2. Initialise trajectory with the noise
        let mut traj = noise.clone();

        // 3. Reverse denoising loop
        for step in (0..self.num_steps).rev() {
            let t_val = step as f64 / self.num_steps as f64;
            let _t = Tensor::new(t_val, &self.device)?;

            // Select the current step's trajectory row
            let step_vec = traj.get(step)?;  // (d_latent,)
            let step_vec_2d = step_vec.reshape((1, self.d_latent))?;

            // Conditional update (simulated score: pull toward origin)
            let alpha_t = Tensor::new(t_val, &self.device)?.broadcast_as((1, self.d_latent))?;
            let cond_score = step_vec_2d.neg()?.broadcast_mul(&alpha_t)?;

            // Unconditional score (toward NULL embedding)
            let uncond_score = step_vec_2d
                .sub(&self.null_embed.reshape((1, self.d_latent))?)?
                .neg()?
                .broadcast_mul(&alpha_t)?;

            // CFG: extrapolate
            let cfg_scale = 2.0;
            let score = uncond_score.add(&cond_score.sub(&uncond_score)?.broadcast_mul(
                &Tensor::new(cfg_scale, &self.device)?.reshape((1, 1))?
            )?)?;

            // Euler step
            let dt = Tensor::new(1.0 / self.num_steps as f64, &self.device)?;
            let dt_broadcast = dt.broadcast_as((1, self.d_latent))?;
            let update = score.broadcast_mul(&dt_broadcast)?;
            #[allow(unused_mut)]
            let mut new_row = step_vec_2d.sub(&update)?;

            // Inject noise each step (temperature decay)
            if step > 0 {
                let noise_scale = temp * (step as f64 / self.num_steps as f64);
                let inject_data: Vec<f64> = (0..self.d_latent)
                    .map(|_| rng.sample::<f64, _>(rand_distr::StandardNormal) * noise_scale)
                    .collect();
                let inject = Tensor::from_slice(
                    &inject_data, (1, self.d_latent), &self.device,
                )?;
                new_row = new_row.add(&inject)?;
            }

            traj = traj.slice_assign(
                &[step..step + 1, 0..self.d_latent],
                &new_row,
            )?;
        }

// 4. Strict inpainting: override step 0 with the injected latent state
        let state_2d = latent_state.reshape((1, self.d_latent))?;
        traj = traj.slice_assign(
            &[0..1, 0..self.d_latent],
            &state_2d,
        )?;

        self.trajectory = traj.clone();
        Ok(traj)
    }
}
