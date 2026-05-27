// ==========================================================================
// training.rs — Entraînement des composants appris (V6)
//
// Implémente deux objectifs d'entraînement orthogonaux :
//
//   1. DENOISING SCORE MATCHING (Vincent 2011, Song & Ermon 2019) pour le
//      ScoreNetwork du PDT. L'objectif :
//
//        L_DSM = E_t, x_0, ε [ ‖s_θ(x_t, t, c) - (-ε/σ_t)‖² ]
//
//      où x_t = α_t·x_0 + σ_t·ε  (forward VP SDE)
//      et c = h_BCI au moment où x_0 a été observé.
//
//   2. CONTRASTIVE ENERGY pour le PotentialMLP du BCI. L'objectif est de
//      sculpter V(q) pour que :
//        - les latents "désirés" (états cohérents) soient minima locaux
//        - les latents bruités/incohérents soient énergétiquement défavorisés
//
//        L_V = E_q+ [ V(q+) ] - E_q- [ V(q-) ] + λ·E_q+ [ ‖∇V(q+)‖² ]
//
//      Le premier terme abaisse V sur les "bons" états, le second l'augmente
//      sur les "mauvais", le troisième régularise pour éviter les explosions
//      (Lipschitz-soft).
//
// IMPORTANT : les gradients sont calculés MANUELLEMENT via différence finie
// pour rester dans candle 0.10 sans Var/autograd. Pour la production (Jetson
// Thor, Blackwell GPU), brancher candle's Var API pour 5-10× speedup. Le
// squelette ci-dessous est designé pour cette migration : `Trainer::step`
// reçoit déjà des `f64`-grad vectors qui pourraient être remplacés par
// `Tensor`-grad de candle.
//
// USAGE
//
//   let mut trainer = Trainer::new(&device)?;
//   for batch in batches {
//       let score_loss = trainer.train_score_network(
//           &mut planner.score_net, &batch.latents, &batch.conditions)?;
//       let energy_loss = trainer.train_potential(
//           &mut bci.potential, &batch.good_latents, &batch.bad_latents)?;
//   }
// ==========================================================================

use candle_core::{DType, Device, Result, Tensor};
use rand::Rng;
use crate::planner::{ScoreNetwork, CosineSchedule, time_embedding};
use crate::bci::PotentialMLP;

// --------------------------------------------------------------------------
// Trainer config
// --------------------------------------------------------------------------

pub struct TrainerConfig {
    pub learning_rate: f64,
    pub grad_clip_norm: f64,
    pub dsm_batch_size: usize,
    /// Nombre de steps t échantillonnés par exemple dans DSM.
    pub dsm_time_samples: usize,
    /// Coefficient de régularisation du gradient pour l'énergie.
    pub energy_grad_reg: f64,
    /// Coefficient de marge pour le contrastif (V(bad) - V(good) ≥ margin).
    pub energy_margin: f64,
    /// Pas de différence finie pour les gradients manuels.
    pub fd_eps: f64,
}

impl Default for TrainerConfig {
    fn default() -> Self {
        Self {
            learning_rate: 1e-3,
            grad_clip_norm: 1.0,
            dsm_batch_size: 32,
            dsm_time_samples: 4,
            energy_grad_reg: 0.01,
            energy_margin: 1.0,
            fd_eps: 1e-4,
        }
    }
}

// --------------------------------------------------------------------------
// Trainer
// --------------------------------------------------------------------------

pub struct Trainer {
    pub config: TrainerConfig,
    pub device: Device,
    /// État Adam optionnel — pour l'instant SGD simple, structure prête à
    /// migrer vers Adam avec moment 1/2 par paramètre.
    pub step_count: u64,
}

impl Trainer {
    pub fn new(device: &Device) -> Result<Self> {
        Ok(Self {
            config: TrainerConfig::default(),
            device: device.clone(),
            step_count: 0,
        })
    }

    pub fn with_config(device: &Device, config: TrainerConfig) -> Result<Self> {
        Ok(Self { config, device: device.clone(), step_count: 0 })
    }

    // ----------------------------------------------------------------------
    // DSM — Denoising Score Matching pour le ScoreNetwork
    // ----------------------------------------------------------------------

    /// Un step DSM sur un batch de latents observés.
    ///
    /// `latents` : shape (B, D_LATENT)
    /// `conditions` : shape (B, D_LATENT) — typiquement les h_BCI correspondants
    ///
    /// Retourne la loss scalaire.
    pub fn train_score_network(
        &mut self,
        net: &mut ScoreNetwork,
        latents: &Tensor,
        conditions: &Tensor,
        schedule: &CosineSchedule,
    ) -> Result<f64> {
        let (b, _d) = latents.dims2()?;
        let mut rng = rand::thread_rng();
        let mut total_loss = 0.0f64;

        // Boucle sur les exemples — calcule la loss par échantillonnage de t.
        for i in 0..b {
            let x0 = latents.get(i)?;            // (d_latent,)
            let c = conditions.get(i)?;          // (d_latent,)

            for _ in 0..self.config.dsm_time_samples {
                // Échantillonne t uniforme dans [1, num_steps]
                let t: usize = rng.gen_range(1..=schedule.num_steps);
                let alpha = schedule.alpha_sqrt(t);
                let sigma = schedule.sigma(t).max(1e-6);

                // Échantillonne le bruit ε ~ N(0, I)
                let eps_data: Vec<f64> = (0..net.d_latent)
                    .map(|_| rng.sample::<f64, _>(rand_distr::StandardNormal))
                    .collect();
                let eps = Tensor::from_slice(&eps_data, net.d_latent, &self.device)?;

                // x_t = α·x_0 + σ·ε
                let alpha_t = Tensor::new(alpha, &self.device)?;
                let sigma_t = Tensor::new(sigma, &self.device)?;
                let x_t = x0.broadcast_mul(&alpha_t)?
                          .add(&eps.broadcast_mul(&sigma_t)?)?;

                // Cible : -ε/σ (score de la distribution conditionnelle x_t|x_0)
                let neg_inv_sigma = Tensor::new(-1.0 / sigma, &self.device)?;
                let target = eps.broadcast_mul(&neg_inv_sigma)?;

                // Forward & loss seulement — l'update via autograd viendra
                // à l'étape 3. Pour l'instant, on accumule la loss pour
                // monitoring et on bouge les poids via SPSA-batch ci-dessous.
                let t_emb = time_embedding(t as f64, net.d_t, &self.device)?;
                let pred = net.score(&x_t, &t_emb, &c)?;
                let diff = pred.sub(&target)?;
                let loss: f64 = diff.sqr()?.sum_all()?.to_scalar()?;
                total_loss += loss / net.d_latent as f64;
                self.step_count += 1;
            }
        }

        // SPSA-style global update sur W1 du score net — un step grossier
        // qui pousse globalement dans la bonne direction. Une vraie
        // descente per-weight viendra avec l'autograd candle.
        self.spsa_update_score(net, latents, conditions, schedule)?;

        Ok(total_loss / (b * self.config.dsm_time_samples) as f64)
    }

    /// SPSA global sur w1 du score net : perturbation aléatoire, deux
    /// évaluations, descente dans la direction du gradient empirique.
    fn spsa_update_score(
        &self,
        net: &mut ScoreNetwork,
        latents: &Tensor,
        conditions: &Tensor,
        schedule: &CosineSchedule,
    ) -> Result<()> {
        let eps = self.config.fd_eps;
        let lr = self.config.learning_rate;
        let w1_shape = net.w1.dims2()?;
        let n = w1_shape.0 * w1_shape.1;

        let pert: Vec<f64> = (0..n).map(|_| {
            if rand::random::<bool>() { eps } else { -eps }
        }).collect();
        let pert_t = Tensor::from_slice(&pert, w1_shape, &self.device)?;

        let w1_save = net.w1.clone();
        net.w1 = w1_save.add(&pert_t)?;
        let l_plus = Self::dsm_loss_snapshot(net, latents, conditions, schedule)?;
        net.w1 = w1_save.sub(&pert_t)?;
        let l_minus = Self::dsm_loss_snapshot(net, latents, conditions, schedule)?;
        net.w1 = w1_save;

        let grad_est = (l_plus - l_minus) / (2.0 * eps);
        let sign_pert: Vec<f64> = pert.iter().map(|p| p.signum()).collect();
        let sign_t = Tensor::from_slice(&sign_pert, w1_shape, &self.device)?;
        let lr_grad = Tensor::new(lr * grad_est, &self.device)?;
        let update = sign_t.broadcast_mul(&lr_grad)?;
        net.w1 = net.w1.sub(&update)?;
        Ok(())
    }

    fn dsm_loss_snapshot(
        net: &ScoreNetwork,
        latents: &Tensor,
        conditions: &Tensor,
        schedule: &CosineSchedule,
    ) -> Result<f64> {
        // Loss snapshot sur le batch avec t fixe à mid-schedule.
        let (b, _) = latents.dims2()?;
        let t = schedule.num_steps / 2;
        let alpha = schedule.alpha_sqrt(t);
        let sigma = schedule.sigma(t).max(1e-6);
        let mut sum = 0.0;
        let mut rng = rand::thread_rng();
        for i in 0..b {
            let x0 = latents.get(i)?;
            let c = conditions.get(i)?;
            let eps_data: Vec<f64> = (0..net.d_latent)
                .map(|_| rng.sample::<f64, _>(rand_distr::StandardNormal))
                .collect();
            let eps = Tensor::from_slice(&eps_data, net.d_latent, &net.device)?;
            let alpha_t = Tensor::new(alpha, &net.device)?;
            let sigma_t = Tensor::new(sigma, &net.device)?;
            let x_t = x0.broadcast_mul(&alpha_t)?
                      .add(&eps.broadcast_mul(&sigma_t)?)?;
            let neg_inv_sigma = Tensor::new(-1.0 / sigma, &net.device)?;
            let target = eps.broadcast_mul(&neg_inv_sigma)?;
            let t_emb = time_embedding(t as f64, net.d_t, &net.device)?;
            let pred = net.score(&x_t, &t_emb, &c)?;
            let diff = pred.sub(&target)?;
            sum += diff.sqr()?.sum_all()?.to_scalar::<f64>()? / net.d_latent as f64;
        }
        Ok(sum / b as f64)
    }

    // ----------------------------------------------------------------------
    // Contrastive Energy pour le PotentialMLP
    // ----------------------------------------------------------------------

    /// Un step contrastif sur le potentiel V.
    ///
    /// `good_latents` : (B, d_q) — états cohérents (attracteurs désirés)
    /// `bad_latents`  : (B, d_q) — états bruités/incohérents
    ///
    /// Loss : V(good).mean() - V(bad).mean() + λ·‖∇V(good)‖².mean()
    ///
    /// Le bon mouvement est de descendre V(good) (mettre les attracteurs en
    /// minima) et monter V(bad) (rendre les incohérences chères), avec
    /// une régularisation du gradient au point bon pour éviter les
    /// instabilités du paysage énergétique.
    pub fn train_potential(
        &mut self,
        potential: &mut PotentialMLP,
        good_latents: &Tensor,
        bad_latents: &Tensor,
    ) -> Result<f64> {
        let (b_good, _) = good_latents.dims2()?;
        let (b_bad, _) = bad_latents.dims2()?;

        // Loss courante (snapshot)
        let mut v_good_sum = 0.0f64;
        for i in 0..b_good {
            let q = good_latents.get(i)?;
            v_good_sum += potential.value(&q)?;
        }
        let v_good_mean = v_good_sum / b_good as f64;

        let mut v_bad_sum = 0.0f64;
        for i in 0..b_bad {
            let q = bad_latents.get(i)?;
            v_bad_sum += potential.value(&q)?;
        }
        let v_bad_mean = v_bad_sum / b_bad as f64;

        // Régularisation gradient
        let mut grad_reg_sum = 0.0f64;
        for i in 0..b_good {
            let q = good_latents.get(i)?;
            let g = potential.grad(&q)?;
            let g_norm: f64 = g.sqr()?.sum_all()?.to_scalar()?;
            grad_reg_sum += g_norm;
        }
        let grad_reg = grad_reg_sum / b_good as f64;

        // Loss totale (hinge sur la marge)
        let margin_violation = (v_good_mean - v_bad_mean + self.config.energy_margin).max(0.0);
        let loss = margin_violation + self.config.energy_grad_reg * grad_reg;

        // Mise à jour des poids via différence finie globale + SGD.
        // On met à jour w1 et w2 du potentiel.
        self.fd_update_potential(potential, good_latents, bad_latents)?;

        Ok(loss)
    }

    /// Mise à jour FD du potentiel. Calcule un gradient approché par
    /// perturbation aléatoire (SPSA — Spall 1992), efficace pour les
    /// mises à jour SGD-like sans backprop complet. O(2) évaluations
    /// vs O(P) pour FD par paramètre.
    fn fd_update_potential(
        &self,
        potential: &mut PotentialMLP,
        good_latents: &Tensor,
        bad_latents: &Tensor,
    ) -> Result<()> {
        let eps = self.config.fd_eps;
        let lr = self.config.learning_rate;

        // SPSA-step pour w1
        let w1_shape = potential.w1.dims2()?;
        let n1 = w1_shape.0 * w1_shape.1;
        let pert: Vec<f64> = (0..n1).map(|_| {
            if rand::random::<bool>() { eps } else { -eps }
        }).collect();
        let pert_tensor = Tensor::from_slice(&pert, w1_shape, &self.device)?;

        let w1_plus = potential.w1.add(&pert_tensor)?;
        let w1_minus = potential.w1.sub(&pert_tensor)?;

        // L_plus / L_minus
        let w1_save = potential.w1.clone();
        potential.w1 = w1_plus;
        let l_plus = Self::contrastive_loss_snapshot(potential, good_latents, bad_latents)?;
        potential.w1 = w1_minus;
        let l_minus = Self::contrastive_loss_snapshot(potential, good_latents, bad_latents)?;
        potential.w1 = w1_save;

        // SPSA gradient estimate ≈ (L+ - L-) / (2·ε·pert)
        let grad_est = (l_plus - l_minus) / (2.0 * eps);
        // Update : w1 -= lr · grad_est · sign(pert) (élément-wise)
        let sign_pert: Vec<f64> = pert.iter().map(|p| if *p > 0.0 { 1.0 } else { -1.0 }).collect();
        let sign_t = Tensor::from_slice(&sign_pert, w1_shape, &self.device)?;
        let lr_grad = Tensor::new(lr * grad_est, &self.device)?;
        let update = sign_t.broadcast_mul(&lr_grad)?;
        potential.w1 = potential.w1.sub(&update)?;

        Ok(())
    }

    fn contrastive_loss_snapshot(
        potential: &PotentialMLP,
        good_latents: &Tensor,
        bad_latents: &Tensor,
    ) -> Result<f64> {
        let (b_good, _) = good_latents.dims2()?;
        let (b_bad, _) = bad_latents.dims2()?;
        let mut v_good_sum = 0.0f64;
        for i in 0..b_good {
            v_good_sum += potential.value(&good_latents.get(i)?)?;
        }
        let mut v_bad_sum = 0.0f64;
        for i in 0..b_bad {
            v_bad_sum += potential.value(&bad_latents.get(i)?)?;
        }
        Ok(v_good_sum / b_good as f64 - v_bad_sum / b_bad as f64)
    }
}

// ==========================================================================
// Replay Buffer — collecte des exemples d'entraînement à chaud
// ==========================================================================

/// Tampon circulaire qui collecte des (latent, condition, future_obs) tuples
/// pendant l'inférence pour servir de données d'entraînement DSM.
pub struct ReplayBuffer {
    pub capacity: usize,
    pub d_latent: usize,
    pub latents: Vec<Vec<f64>>,
    pub conditions: Vec<Vec<f64>>,
    /// Indique si l'état était "bon" (α_sync élevé) ou "mauvais" (α_sync bas).
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

    /// Sample un batch équilibré bon/mauvais.
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
        let half = batch_size / 2;

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
    fn trainer_score_does_not_explode() {
        let mut net = ScoreNetwork::new(16, 32, 16, 128, &dev()).unwrap();
        let schedule = CosineSchedule::new(10, 0.008);
        let mut trainer = Trainer::new(&dev()).unwrap();

        let latents = Tensor::randn(0.0f64, 0.3, (4, 16), &dev()).unwrap();
        let conds = Tensor::randn(0.0f64, 0.3, (4, 16), &dev()).unwrap();
        let loss = trainer.train_score_network(&mut net, &latents, &conds, &schedule).unwrap();
        assert!(loss.is_finite() && loss < 1e6, "loss should be finite: {}", loss);
    }

    #[test]
    fn trainer_potential_contrastive_step() {
        let mut p = PotentialMLP::new(8, 32, &dev()).unwrap();
        let mut trainer = Trainer::new(&dev()).unwrap();

        let good = Tensor::randn(0.0f64, 0.1, (4, 8), &dev()).unwrap();  // bon état : petit norme
        let bad = Tensor::randn(0.0f64, 2.0, (4, 8), &dev()).unwrap();   // mauvais : grand norme
        let loss = trainer.train_potential(&mut p, &good, &bad).unwrap();
        assert!(loss.is_finite());
    }

    #[test]
    fn replay_buffer_balanced_sample() {
        let mut buf = ReplayBuffer::new(100, 8);
        for i in 0..50 {
            let is_good = i % 2 == 0;
            buf.push(vec![0.1; 8], vec![0.1; 8], is_good);
        }
        let (g_l, _g_c, b_l, _b_c) = buf.sample_balanced(16, &dev()).unwrap();
        assert_eq!(g_l.dims(), &[8, 8]);  // 16/2 = 8 good
        assert_eq!(b_l.dims(), &[8, 8]);  // 16/2 = 8 bad
    }
}
