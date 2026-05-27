// ==========================================================================
// training.rs — Entraînement en ligne (V6.1)
//
// Denoising Score Matching pour ScoreNetwork (PDT)
// Contrastive Energy pour PotentialMLP (BCI)
// ==========================================================================

use candle_core::{Device, Result, Tensor};
use rand::Rng;
use crate::planner::{ScoreNetwork, CosineSchedule, time_embedding};
use crate::bci::PotentialMLP;

// --------------------------------------------------------------------------
// TrainerConfig
// --------------------------------------------------------------------------

pub struct TrainerConfig {
    pub learning_rate: f64,
    pub weight_decay: f64,
    pub dsm_time_samples: usize,
    pub energy_grad_reg: f64,
    pub energy_margin: f64,
    pub passes_per_occasion: usize,
}

impl Default for TrainerConfig {
    fn default() -> Self {
        Self {
            learning_rate: 1e-3, weight_decay: 1e-4,
            dsm_time_samples: 4, energy_grad_reg: 0.01,
            energy_margin: 0.5, passes_per_occasion: 4,
        }
    }
}

// --------------------------------------------------------------------------
// Trainer
// --------------------------------------------------------------------------

pub struct Trainer {
    pub config: TrainerConfig,
    pub device: Device,
    pub total_dsm_loss: f64,
    pub total_energy_loss: f64,
    pub train_calls: u64,
}

impl Trainer {
    pub fn new(device: &Device) -> Result<Self> {
        Ok(Self { config: TrainerConfig::default(), device: device.clone(),
            total_dsm_loss: 0.0, total_energy_loss: 0.0, train_calls: 0 })
    }

    pub fn train_score_network(
        &mut self, net: &mut ScoreNetwork,
        latents: &Tensor, conditions: &Tensor, sched: &CosineSchedule,
    ) -> Result<f64> {
        let (b, _) = latents.dims2()?;
        let mut rng = rand::thread_rng();
        let mut tot = 0.0f64;
        for _ in 0..self.config.passes_per_occasion {
            for i in 0..b {
                let x0 = latents.get(i)?;
                let c = conditions.get(i)?;
                for _ in 0..self.config.dsm_time_samples {
                    let t: usize = rng.gen_range(1..=sched.num_steps);
                    let alpha = sched.alpha_sqrt(t);
                    let sigma = sched.sigma(t).max(1e-6);
                    let ed: Vec<f64> = (0..net.d_latent)
                        .map(|_| rng.sample::<f64, _>(rand_distr::StandardNormal)).collect();
                    let eps = Tensor::from_slice(&ed, net.d_latent, &self.device)?;
                    let at = Tensor::new(alpha, &self.device)?;
                    let st = Tensor::new(sigma, &self.device)?;
                    let xt = x0.broadcast_mul(&at)?.add(&eps.broadcast_mul(&st)?)?;
                    let target = eps.broadcast_mul(&Tensor::new(-1.0 / sigma, &self.device)?)?;
                    let te = time_embedding(t as f64, net.d_t, &self.device)?;
                    let pred = net.score(&xt, &te, &c)?;
                    tot += pred.sub(&target)?.sqr()?.sum_all()?
                        .to_scalar::<f64>()? / net.d_latent as f64;
                }
            }
        }
        let cnt = b * self.config.dsm_time_samples * self.config.passes_per_occasion;
        let mean = tot / cnt as f64;
        self.total_dsm_loss += mean;
        self.train_calls += 1;
        self.spsa_score(net, latents, conditions, sched)?;
        Ok(mean)
    }

    fn spsa_score(
        &self, net: &mut ScoreNetwork,
        latents: &Tensor, conditions: &Tensor, sched: &CosineSchedule,
    ) -> Result<()> {
        let lr = self.config.learning_rate;
        let wd = self.config.weight_decay;
        let eps = 1e-4;
        for param in [("w1", net.w1.clone()), ("w2", net.w2.clone()), ("w3", net.w3.clone())] {
            let (name, saved) = param;
            let shape: &[usize] = saved.dims();
            let n: usize = shape.iter().product();
            let sign_data: Vec<f64> = (0..n).map(|_| if rand::random::<bool>() { 1.0 } else { -1.0 }).collect();
            let pert = Tensor::from_slice(&sign_data, shape, &self.device)?
                .broadcast_mul(&Tensor::new(eps, &self.device)?)?;

            // Perturbation +
            let pp = saved.add(&pert)?;
            if name == "w1" { net.w1 = pp } else if name == "w2" { net.w2 = pp } else if name == "w3" { net.w3 = pp }
            let lp = Self::dsm_snapshot(net, latents, conditions, sched)?;

            // Perturbation -
            let pm = saved.sub(&pert)?;
            if name == "w1" { net.w1 = pm } else if name == "w2" { net.w2 = pm } else if name == "w3" { net.w3 = pm }
            let lm = Self::dsm_snapshot(net, latents, conditions, sched)?;

            let g = (lp - lm) / (2.0 * eps) * (n as f64).sqrt();
            let sign_t = Tensor::from_slice(&sign_data, shape, &self.device)?;
            let update = sign_t.broadcast_mul(&Tensor::new(lr * g, &self.device)?)?;
            let decay = saved.broadcast_mul(&Tensor::new(1.0 - lr * wd, &self.device)?)?;
            let new = decay.sub(&update)?;
            if name == "w1" { net.w1 = new } else if name == "w2" { net.w2 = new } else if name == "w3" { net.w3 = new }
        }
        Ok(())
    }

    fn dsm_snapshot(
        net: &ScoreNetwork,
        latents: &Tensor, conditions: &Tensor, sched: &CosineSchedule,
    ) -> Result<f64> {
        let (b, _) = latents.dims2()?;
        let t = sched.num_steps / 2;
        let alpha = sched.alpha_sqrt(t);
        let sigma = sched.sigma(t).max(1e-6);
        let mut sum = 0.0;
        for i in 0..b.min(2) {
            let x0 = latents.get(i)?;
            let c = conditions.get(i)?;
            let ed: Vec<f64> = (0..net.d_latent).map(|_| rand::random::<f64>() * 2.0 - 1.0).collect();
            let eps = Tensor::from_slice(&ed, net.d_latent, &net.device)?;
            let xt = x0.broadcast_mul(&Tensor::new(alpha, &net.device)?)?
                     .add(&eps.broadcast_mul(&Tensor::new(sigma, &net.device)?)?)?;
            let target = eps.broadcast_mul(&Tensor::new(-1.0 / sigma, &net.device)?)?;
            let te = time_embedding(t as f64, net.d_t, &net.device)?;
            sum += net.score(&xt, &te, &c)?.sub(&target)?.sqr()?.sum_all()?
                .to_scalar::<f64>()? / net.d_latent as f64;
        }
        Ok(sum / b.min(2) as f64)
    }

    // ------------------------------------------------------------------
    // Contrastive Energy
    // ------------------------------------------------------------------

    pub fn train_potential(
        &mut self, potential: &mut PotentialMLP,
        good: &Tensor, bad: &Tensor,
    ) -> Result<f64> {
        let (bg, _) = good.dims2()?;
        let (bb, _) = bad.dims2()?;
        let mut vg = 0.0; for i in 0..bg { vg += potential.value(&good.get(i)?)?; }
        let mut vb = 0.0; for i in 0..bb { vb += potential.value(&bad.get(i)?)?; }
        let mut gr = 0.0; for i in 0..bg {
            let g = potential.grad(&good.get(i)?)?;
            gr += g.sqr()?.sum_all()?.to_scalar::<f64>()?;
        }
        let loss = (vg/bg as f64 - vb/bb as f64 + self.config.energy_margin).max(0.0)
                 + self.config.energy_grad_reg * gr / bg as f64;
        self.total_energy_loss += loss;
        self.train_calls += 1;
        self.spsa_potential(potential, good, bad)?;
        Ok(loss)
    }

    fn spsa_potential(
        &self, potential: &mut PotentialMLP, good: &Tensor, bad: &Tensor,
    ) -> Result<()> {
        let lr = self.config.learning_rate;
        let wd = self.config.weight_decay;
        let eps = 1e-4;
        for param in [("w1", potential.w1.clone()), ("w2", potential.w2.clone())] {
            let (name, saved) = param;
            let shape: &[usize] = saved.dims();
            let n: usize = shape.iter().product();
            let sd: Vec<f64> = (0..n).map(|_| if rand::random::<bool>() { 1.0 } else { -1.0 }).collect();
            let pert = Tensor::from_slice(&sd, shape, &self.device)?
                .broadcast_mul(&Tensor::new(eps, &self.device)?)?;

            let pp = saved.add(&pert)?;
            if name == "w1" { potential.w1 = pp } else { potential.w2 = pp }
            let lp = Self::cl_snapshot(potential, good, bad)?;

            let pm = saved.sub(&pert)?;
            if name == "w1" { potential.w1 = pm } else { potential.w2 = pm }
            let lm = Self::cl_snapshot(potential, good, bad)?;

            let g = (lp - lm) / (2.0 * eps) * (n as f64).sqrt();
            let sign_t = Tensor::from_slice(&sd, shape, &self.device)?;
            let update = sign_t.broadcast_mul(&Tensor::new(lr * g, &self.device)?)?;
            let decay = saved.broadcast_mul(&Tensor::new(1.0 - lr * wd, &self.device)?)?;
            let new = decay.sub(&update)?;
            if name == "w1" { potential.w1 = new } else { potential.w2 = new }
        }
        Ok(())
    }

    fn cl_snapshot(potential: &PotentialMLP, good: &Tensor, bad: &Tensor) -> Result<f64> {
        let (bg, _) = good.dims2()?; let (bb, _) = bad.dims2()?;
        let mut vg = 0.0; for i in 0..bg { vg += potential.value(&good.get(i)?)?; }
        let mut vb = 0.0; for i in 0..bb { vb += potential.value(&bad.get(i)?)?; }
        Ok(vg / bg as f64 - vb / bb as f64)
    }

    pub fn report(&self) -> String {
        format!("Training: {} calls | avg DSM: {:.4} | avg Energy: {:.4}",
            self.train_calls,
            self.total_dsm_loss / (self.train_calls.max(1) as f64),
            self.total_energy_loss / (self.train_calls.max(1) as f64))
    }
}

// ==========================================================================
// Replay Buffer
// ==========================================================================

use std::collections::VecDeque;

pub struct ReplayBuffer {
    pub capacity: usize, pub d_latent: usize,
    pub latents: Vec<Vec<f64>>, pub conditions: Vec<Vec<f64>>,
    pub quality: Vec<bool>,
    recent_alphas: VecDeque<f64>,
}

impl ReplayBuffer {
    pub fn new(capacity: usize, d_latent: usize) -> Self {
        Self { capacity, d_latent,
            latents: Vec::with_capacity(capacity),
            conditions: Vec::with_capacity(capacity),
            quality: Vec::with_capacity(capacity),
            recent_alphas: VecDeque::with_capacity(32) }
    }

    pub fn push(&mut self, latent: Vec<f64>, condition: Vec<f64>, alpha_sync: f64) {
        if self.latents.len() >= self.capacity {
            self.latents.remove(0); self.conditions.remove(0); self.quality.remove(0);
        }
        self.recent_alphas.push_back(alpha_sync);
        if self.recent_alphas.len() > 32 { self.recent_alphas.pop_front(); }
        let median = if self.recent_alphas.len() >= 4 {
            let mut v: Vec<f64> = self.recent_alphas.iter().copied().collect();
            v.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
            let m = v.len() / 2;
            if v.len() % 2 == 0 { (v[m-1] + v[m]) / 2.0 } else { v[m] }
        } else { 0.4 };
        self.latents.push(latent);
        self.conditions.push(condition);
        self.quality.push(alpha_sync >= median);
    }

    pub fn has_both_classes(&self) -> bool {
        self.quality.iter().any(|&q| q) && self.quality.iter().any(|&q| !q)
    }

    pub fn sample_balanced(&self, bs: usize, device: &Device)
        -> Result<(Tensor, Tensor, Tensor, Tensor)>
    {
        let goods: Vec<_> = (0..self.quality.len()).filter(|&i| self.quality[i]).collect();
        let bads: Vec<_> = (0..self.quality.len()).filter(|&i| !self.quality[i]).collect();
        if goods.is_empty() || bads.is_empty() {
            return Err(candle_core::Error::Msg("need both classes".into()).bt());
        }
        let mut rng = rand::thread_rng();
        let half = (bs / 2).min(goods.len()).min(bads.len());
        let (mut gl, mut gc, mut bl, mut bc) = (vec![], vec![], vec![], vec![]);
        for _ in 0..half { let i = goods[rng.gen_range(0..goods.len())];
            gl.extend_from_slice(&self.latents[i]); gc.extend_from_slice(&self.conditions[i]); }
        for _ in 0..half { let i = bads[rng.gen_range(0..bads.len())];
            bl.extend_from_slice(&self.latents[i]); bc.extend_from_slice(&self.conditions[i]); }
        Ok((Tensor::from_slice(&gl, (half, self.d_latent), device)?,
            Tensor::from_slice(&gc, (half, self.d_latent), device)?,
            Tensor::from_slice(&bl, (half, self.d_latent), device)?,
            Tensor::from_slice(&bc, (half, self.d_latent), device)?))
    }

    pub fn len(&self) -> usize { self.latents.len() }
    pub fn is_empty(&self) -> bool { self.latents.is_empty() }
    pub fn good_count(&self) -> usize { self.quality.iter().filter(|&q| *q).count() }
    pub fn bad_count(&self) -> usize { self.quality.iter().filter(|&q| !*q).count() }
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    fn dev() -> Device { Device::Cpu }

    #[test]
    fn trainer_score_finite() {
        let mut net = ScoreNetwork::new(16, 32, 16, 128, &dev()).unwrap();
        let sched = CosineSchedule::new(10, 0.008);
        let mut t = Trainer::new(&dev()).unwrap();
        let l = Tensor::randn(0.0, 0.3, (4, 16), &dev()).unwrap();
        let c = Tensor::randn(0.0, 0.3, (4, 16), &dev()).unwrap();
        assert!(t.train_score_network(&mut net, &l, &c, &sched).unwrap() < 1e6);
    }

    #[test]
    fn trainer_potential_finite() {
        let mut p = PotentialMLP::new(8, 32, &dev()).unwrap();
        let mut t = Trainer::new(&dev()).unwrap();
        let g = Tensor::randn(0.0, 0.1, (4, 8), &dev()).unwrap();
        let b = Tensor::randn(0.0, 2.0, (4, 8), &dev()).unwrap();
        assert!(t.train_potential(&mut p, &g, &b).unwrap().is_finite());
    }

    #[test]
    fn replay_balanced() {
        let mut buf = ReplayBuffer::new(100, 8);
        for i in 0..50 { buf.push(vec![0.1; 8], vec![0.1; 8], if i%2==0 { 0.6 } else { 0.2 }); }
        assert!(buf.has_both_classes());
        let (a,_,b,_) = buf.sample_balanced(16, &dev()).unwrap();
        assert_eq!(a.dims(), &[8, 8]); assert_eq!(b.dims(), &[8, 8]);
    }

    #[test]
    fn dynamic_threshold() {
        let mut buf = ReplayBuffer::new(100, 4);
        for i in 0..30 { buf.push(vec![0.0; 4], vec![0.0; 4], 0.20 + i as f64 * 0.005); }
        assert!(buf.has_both_classes());
    }
}
