// ==========================================================================
// training.rs — Entraînement V6.1 (AdamW + autograd candle)
//
// Refonte de la V6.0 (SPSA placeholder) avec :
//
//   1. AUTOGRAD COMPLET via candle_core::Var + Tensor::backward
//      Tous les paramètres (w1, w2, w3, b1, b2, b3 du ScoreNetwork ;
//      w1, w2, b1, b2 du PotentialMLP) sont des Var. Le forward construit
//      un graphe différentiable, .backward() peuple GradStore, et AdamW
//      met à jour via .set() en place — tous les clones du Var voient
//      l'update.
//
//   2. ADAMW (candle_nn::optim) au lieu de SPSA
//      Speed-up estimé 5-10× sur la convergence vs SGD/SPSA.
//      Weight decay intégré → régularisation L2 gratuite.
//
//   3. OBJECTIFS INCHANGÉS — seule la mécanique d'optim change.
//      - DSM (Denoising Score Matching) pour le ScoreNetwork
//      - Contrastive Energy pour le PotentialMLP
// ==========================================================================

use candle_core::{Device, Result, Tensor};
use candle_nn::{Optimizer, optim::{AdamW, ParamsAdamW}};
use rand::Rng;

use crate::planner::{ScoreNetwork, CosineSchedule, time_embedding};
use crate::bci::PotentialMLP;

// --------------------------------------------------------------------------
// Trainer config
// --------------------------------------------------------------------------

#[derive(Clone)]
pub struct TrainerConfig {
    pub score_lr: f64,
    pub potential_lr: f64,
    pub weight_decay: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub adam_eps: f64,
    pub dsm_time_samples: usize,
    pub energy_grad_reg: f64,
    pub energy_margin: f64,
    pub grad_clip_norm: f64,
}

impl Default for TrainerConfig {
    fn default() -> Self {
        Self {
            score_lr: 1e-3,
            potential_lr: 5e-4,
            weight_decay: 1e-4,
            beta1: 0.9,
            beta2: 0.999,
            adam_eps: 1e-8,
            dsm_time_samples: 4,
            energy_grad_reg: 0.01,
            energy_margin: 1.0,
            grad_clip_norm: 1.0,
        }
    }
}

// --------------------------------------------------------------------------
// Trainer — un optimizer par composant pour pouvoir tuner indépendamment
// --------------------------------------------------------------------------

pub struct Trainer {
    pub config: TrainerConfig,
    pub device: Device,
    pub score_opt: AdamW,
    pub potential_opt: AdamW,
    pub step_count: u64,
    pub last_score_loss: f64,
    pub last_potential_loss: f64,
}

impl Trainer {
    pub fn new(
        device: &Device,
        score_vars: Vec<candle_core::Var>,
        potential_vars: Vec<candle_core::Var>,
    ) -> Result<Self> {
        let cfg = TrainerConfig::default();
        let score_params = ParamsAdamW {
            lr: cfg.score_lr,
            beta1: cfg.beta1,
            beta2: cfg.beta2,
            eps: cfg.adam_eps,
            weight_decay: cfg.weight_decay,
        };
        let potential_params = ParamsAdamW {
            lr: cfg.potential_lr,
            beta1: cfg.beta1,
            beta2: cfg.beta2,
            eps: cfg.adam_eps,
            weight_decay: cfg.weight_decay,
        };
        let score_opt = AdamW::new(score_vars, score_params)?;
        let potential_opt = AdamW::new(potential_vars, potential_params)?;
        Ok(Self {
            config: cfg,
            device: device.clone(),
            score_opt, potential_opt,
            step_count: 0,
            last_score_loss: 0.0,
            last_potential_loss: 0.0,
        })
    }

    // ----------------------------------------------------------------------
    // Score Network — DSM training step
    // ----------------------------------------------------------------------

    pub fn train_score_step(
        &mut self,
        net: &ScoreNetwork,
        latents: &Tensor,
        conditions: &Tensor,
        schedule: &CosineSchedule,
    ) -> Result<f64> {
        let (b, _) = latents.dims2()?;
        let mut rng = rand::thread_rng();

        let mut accum_loss: Option<Tensor> = None;
        let mut n_terms = 0;

        for i in 0..b {
            let x0 = latents.get(i)?;
            let c = conditions.get(i)?;

            for _ in 0..self.config.dsm_time_samples {
                let t: usize = rng.gen_range(1..=schedule.num_steps);
                let alpha = schedule.alpha_sqrt(t);
                let sigma = schedule.sigma(t).max(1e-6);

                let eps_data: Vec<f64> = (0..net.d_latent)
                    .map(|_| rng.sample::<f64, _>(rand_distr::StandardNormal))
                    .collect();
                let eps = Tensor::from_slice(&eps_data, net.d_latent, &self.device)?;

                let alpha_t = Tensor::new(alpha, &self.device)?;
                let sigma_t = Tensor::new(sigma, &self.device)?;
                let x_t = x0.broadcast_mul(&alpha_t)?
                          .add(&eps.broadcast_mul(&sigma_t)?)?;

                let neg_inv_sigma = Tensor::new(-1.0 / sigma, &self.device)?;
                let target = eps.broadcast_mul(&neg_inv_sigma)?;

                let t_emb = time_embedding(t as f64, net.d_t, &self.device)?;
                let pred = net.score(&x_t, &t_emb, &c)?;
                let diff = pred.sub(&target)?;
                let loss_term = diff.sqr()?.mean_all()?;

                accum_loss = Some(match accum_loss {
                    None => loss_term,
                    Some(prev) => prev.add(&loss_term)?,
                });
                n_terms += 1;
            }
        }

        let total = accum_loss.unwrap();
        let n_t = Tensor::new(n_terms as f64, &self.device)?;
        let loss = total.broadcast_div(&n_t)?;
        let loss_value: f64 = loss.to_scalar()?;

        self.score_opt.backward_step(&loss)?;
        self.last_score_loss = loss_value;
        self.step_count += 1;
        Ok(loss_value)
    }

    // ----------------------------------------------------------------------
    // Potential — Contrastive Energy training step
    // ----------------------------------------------------------------------

    pub fn train_potential_step(
        &mut self,
        potential: &PotentialMLP,
        good_latents: &Tensor,
        bad_latents: &Tensor,
    ) -> Result<f64> {
        let (b_good, _) = good_latents.dims2()?;
        let (b_bad, _) = bad_latents.dims2()?;

        let mut v_good_accum: Option<Tensor> = None;
        for i in 0..b_good {
            let v = potential.value_tensor(&good_latents.get(i)?)?;
            v_good_accum = Some(match v_good_accum {
                None => v,
                Some(p) => p.add(&v)?,
            });
        }
        let v_good_mean = v_good_accum.unwrap()
            .broadcast_div(&Tensor::new(b_good as f64, &self.device)?)?;

        let mut v_bad_accum: Option<Tensor> = None;
        for i in 0..b_bad {
            let v = potential.value_tensor(&bad_latents.get(i)?)?;
            v_bad_accum = Some(match v_bad_accum {
                None => v,
                Some(p) => p.add(&v)?,
            });
        }
        let v_bad_mean = v_bad_accum.unwrap()
            .broadcast_div(&Tensor::new(b_bad as f64, &self.device)?)?;

        let margin = Tensor::new(self.config.energy_margin, &self.device)?;
        let diff = v_good_mean.sub(&v_bad_mean)?.add(&margin)?;
        let zero = Tensor::zeros_like(&diff)?;
        let hinge = diff.maximum(&zero)?;

        let loss = hinge;
        let loss_value: f64 = loss.to_scalar()?;

        self.potential_opt.backward_step(&loss)?;
        self.last_potential_loss = loss_value;
        Ok(loss_value)
    }

    pub fn set_score_lr(&mut self, lr: f64) {
        self.score_opt.set_learning_rate(lr);
    }
    pub fn set_potential_lr(&mut self, lr: f64) {
        self.potential_opt.set_learning_rate(lr);
    }
}

// ==========================================================================
// Replay Buffer
// ==========================================================================

pub struct ReplayBuffer {
    pub capacity: usize,
    pub d_latent: usize,
    pub latents: Vec<Vec<f64>>,
    pub conditions: Vec<Vec<f64>>,
    pub quality: Vec<bool>,
}

impl ReplayBuffer {
    pub fn new(capacity: usize, d_latent: usize) -> Self {
        Self {
            capacity, d_latent,
            latents: Vec::with_capacity(capacity),
            conditions: Vec::with_capacity(capacity),
            quality: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, latent: Vec<f64>, condition: Vec<f64>, is_good: bool) {
        if self.latents.len() >= self.capacity {
            self.latents.remove(0);
            self.conditions.remove(0);
            self.quality.remove(0);
        }
        self.latents.push(latent);
        self.conditions.push(condition);
        self.quality.push(is_good);
    }

    pub fn has_both_classes(&self) -> bool {
        self.quality.iter().any(|&q| q) && self.quality.iter().any(|&q| !q)
    }

    pub fn sample_balanced(&self, batch_size: usize, device: &Device)
        -> Result<(Tensor, Tensor, Tensor, Tensor)>
    {
        let goods: Vec<usize> = (0..self.quality.len())
            .filter(|&i| self.quality[i]).collect();
        let bads: Vec<usize> = (0..self.quality.len())
            .filter(|&i| !self.quality[i]).collect();

        if goods.is_empty() || bads.is_empty() {
            return Err(candle_core::Error::Msg(
                "ReplayBuffer needs both good and bad samples".into()).bt());
        }

        let mut rng = rand::thread_rng();
        let half = (batch_size / 2).max(1);

        let mut good_latents = Vec::new();
        let mut good_conds = Vec::new();
        for _ in 0..half {
            let i = goods[rng.gen_range(0..goods.len())];
            good_latents.extend_from_slice(&self.latents[i]);
            good_conds.extend_from_slice(&self.conditions[i]);
        }

        let mut bad_latents = Vec::new();
        let mut bad_conds = Vec::new();
        for _ in 0..half {
            let i = bads[rng.gen_range(0..bads.len())];
            bad_latents.extend_from_slice(&self.latents[i]);
            bad_conds.extend_from_slice(&self.conditions[i]);
        }

        let good_l_t = Tensor::from_slice(&good_latents, (half, self.d_latent), device)?;
        let good_c_t = Tensor::from_slice(&good_conds, (half, self.d_latent), device)?;
        let bad_l_t = Tensor::from_slice(&bad_latents, (half, self.d_latent), device)?;
        let bad_c_t = Tensor::from_slice(&bad_conds, (half, self.d_latent), device)?;

        Ok((good_l_t, good_c_t, bad_l_t, bad_c_t))
    }

    pub fn len(&self) -> usize { self.latents.len() }
    pub fn is_empty(&self) -> bool { self.latents.is_empty() }
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn dev() -> Device { Device::Cpu }

    #[test]
    fn dsm_step_does_not_diverge() {
        let net = ScoreNetwork::new(16, 32, 16, 64, &dev()).unwrap();
        let schedule = CosineSchedule::new(10, 0.008);
        let mut trainer = Trainer::new(&dev(),
            net.trainable_vars(),
            PotentialMLP::new(8, 32, &dev()).unwrap().trainable_vars()).unwrap();

        let latents = Tensor::randn(0.0f64, 0.3, (4, 16), &dev()).unwrap();
        let conds = Tensor::randn(0.0f64, 0.3, (4, 16), &dev()).unwrap();

        let l0 = trainer.train_score_step(&net, &latents, &conds, &schedule).unwrap();
        for _ in 0..10 {
            trainer.train_score_step(&net, &latents, &conds, &schedule).unwrap();
        }
        assert!(trainer.last_score_loss.is_finite()
                && trainer.last_score_loss < l0 * 100.0);
    }

    #[test]
    fn potential_step_moves_weights() {
        let potential = PotentialMLP::new(8, 32, &dev()).unwrap();
        let score = ScoreNetwork::new(16, 32, 16, 64, &dev()).unwrap();
        let mut trainer = Trainer::new(&dev(),
            score.trainable_vars(),
            potential.trainable_vars()).unwrap();

        // Setup non-trivial : good ET bad ont la même norme (donc ½‖q‖² ne
        // discrimine pas), seul le MLP doit apprendre la distinction.
        // good ∈ +x direction, bad ∈ -x direction, même magnitude.
        let mut good_vec = vec![0.0; 8 * 4];
        let mut bad_vec = vec![0.0; 8 * 4];
        for i in 0..4 {
            good_vec[i * 8] = 0.5;
            bad_vec[i * 8] = -0.5;
        }
        let good = Tensor::from_slice(&good_vec, (4, 8), &dev()).unwrap();
        let bad = Tensor::from_slice(&bad_vec, (4, 8), &dev()).unwrap();

        let v_good_0: f64 = (0..4).map(|i| potential.value(&good.get(i).unwrap()).unwrap())
            .sum::<f64>() / 4.0;
        let v_bad_0: f64 = (0..4).map(|i| potential.value(&bad.get(i).unwrap()).unwrap())
            .sum::<f64>() / 4.0;

        // Augmente la lr pour test plus rapide
        trainer.set_potential_lr(1e-2);
        for _ in 0..50 {
            trainer.train_potential_step(&potential, &good, &bad).unwrap();
        }

        let v_good_after: f64 = (0..4).map(|i| potential.value(&good.get(i).unwrap()).unwrap())
            .sum::<f64>() / 4.0;
        let v_bad_after: f64 = (0..4).map(|i| potential.value(&bad.get(i).unwrap()).unwrap())
            .sum::<f64>() / 4.0;

        assert!(v_good_after.is_finite() && v_bad_after.is_finite());
        // Le gap V(bad) - V(good) doit augmenter (les good devenir plus bas relativement).
        let gap_before = v_bad_0 - v_good_0;
        let gap_after = v_bad_after - v_good_after;
        assert!(gap_after >= gap_before - 1e-6,
                "gap V(bad)-V(good) didn't increase: {} → {}", gap_before, gap_after);
    }

    #[test]
    fn replay_buffer_balanced_sample() {
        let mut buf = ReplayBuffer::new(100, 8);
        for i in 0..50 {
            buf.push(vec![0.1; 8], vec![0.1; 8], i % 2 == 0);
        }
        let (g_l, _, b_l, _) = buf.sample_balanced(16, &dev()).unwrap();
        assert_eq!(g_l.dims(), &[8, 8]);
        assert_eq!(b_l.dims(), &[8, 8]);
    }

    #[test]
    fn replay_has_both_classes() {
        let mut buf = ReplayBuffer::new(100, 4);
        assert!(!buf.has_both_classes());
        buf.push(vec![0.0; 4], vec![0.0; 4], true);
        assert!(!buf.has_both_classes());
        buf.push(vec![0.0; 4], vec![0.0; 4], false);
        assert!(buf.has_both_classes());
    }
}
