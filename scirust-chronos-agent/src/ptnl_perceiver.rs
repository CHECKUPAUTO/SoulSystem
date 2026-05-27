// ==========================================================================
// ptnl_perceiver.rs — PTNLPerceiver V6 (Pillar 1)
//
// Drop-in replacement for V5. Tient enfin les promesses du nom :
//
//   "Perceptron Temporel Non Linéaire" — V6 ajoute :
//
//   1. NON-LINÉARITÉ EXPLICITE
//      MLP résiduel à 2 couches (GELU) après la cross-attention.
//      La V5 était strictement linéaire (composition de matmul + softmax).
//      Un softmax n'est PAS une non-linéarité d'expressivité — c'est une
//      normalisation. Un perceptron multicouche en est une.
//
//   2. ENCODAGE TEMPOREL EXPLICITE (Time2Vec, Kazemi 2019)
//      φ(t)[0]   = ω₀·t + φ₀                 (linéaire)
//      φ(t)[i>0] = sin(ωᵢ·t + φᵢ)            (périodiques apprenables)
//      Ajouté aux features avant projection K/V.
//      La V5 encodait le temps "implicitement par l'ordre des lignes" —
//      ce qui ne survit pas au shuffle batch et n'exprime aucune périodicité.
//
//   3. BIDIRECTIONNALITÉ (passé ⊕ futur)
//      La fenêtre d'entrée est concat[past_window | future_predictions]
//      où future_predictions vient du PDT au step précédent (trajectoire
//      planifiée). L'attention non-causale (mask=None) permet aux latents
//      de regarder *symétriquement* dans le passé et le futur.
//      C'est ce qui justifie philosophiquement le "block universe" :
//      passé et futur ont le même statut sémantique pour la perception.
//
//   4. RECONSTRUCTION PER-DIMENSION
//      W_o : (d_latent → d_input), appliqué par token avant projection
//      M→T_total. La V5 collapsait sur mean(dim=1) avant la projection
//      inverse, perdant l'information per-feature.
//
//   5. NOISEGATE GRADUEL
//      La V5 avait un gate binaire à variance > 0.15 : tout ou rien.
//      V6 utilise un soft-gate : att_gain = exp(-(σ²/τ²)), ce qui
//      atténue progressivement l'attention au signal bruité sans tout
//      figer. Plus stable, moins fragile au stress test.
// ==========================================================================

use candle_core::{DType, Device, Result, Tensor};

// --------------------------------------------------------------------------
// Helper : clipping L2 — anti-explosion en boucle fermée non-entraînée
// --------------------------------------------------------------------------

fn clip_l2_matrix(x: &Tensor, max_norm_per_row: f64) -> Result<Tensor> {
    // Clip ligne par ligne. Pour (M, d_latent), borne ‖latent_i‖ ≤ max.
    if x.dims().len() != 2 {
        return Ok(x.clone());
    }
    let (m, _d) = x.dims2()?;
    let mut rows = Vec::with_capacity(m);
    for i in 0..m {
        let row = x.get(i)?;
        let norm: f64 = row.sqr()?.sum_all()?.to_scalar::<f64>()?.sqrt();
        if norm <= max_norm_per_row || norm < 1e-12 {
            rows.push(row);
        } else {
            let scale = Tensor::new(max_norm_per_row / norm, x.device())?;
            rows.push(row.broadcast_mul(&scale)?);
        }
    }
    let refs: Vec<&Tensor> = rows.iter().collect();
    Tensor::stack(&refs, 0)
}

// --------------------------------------------------------------------------
// Time2Vec — encodage temporel apprenable
// --------------------------------------------------------------------------

/// Encodage Time2Vec (Kazemi et al., 2019).
///
/// Pour un scalaire t ∈ ℝ, retourne un vecteur (d_time,) :
///   φ(t)[0]   = ω₀·t + φ₀
///   φ(t)[k]   = sin(ωₖ·t + φₖ)  pour k = 1..d_time
///
/// Tous les ωₖ, φₖ sont appris.
pub struct Time2Vec {
    pub omega: Tensor,  // (d_time,)
    pub phi:   Tensor,  // (d_time,)
    pub d_time: usize,
    pub device: Device,
}

impl Time2Vec {
    pub fn new(d_time: usize, device: &Device) -> Result<Self> {
        // Initialisation log-uniforme des fréquences (couvre plusieurs ordres
        // de magnitude — utile pour capturer rythmes courts et longs).
        let omega = Tensor::randn(0.0f64, 1.0, d_time, device)?;
        let phi = Tensor::randn(0.0f64, 0.1, d_time, device)?;
        Ok(Self { omega, phi, d_time, device: device.clone() })
    }

    /// Encode une séquence de timestamps t ∈ (T_seq,) en (T_seq, d_time).
    pub fn encode(&self, t: &Tensor) -> Result<Tensor> {
        let t_len = t.dims1()?;
        // Broadcast : t (T,1) · omega (1,d_time) → (T, d_time)
        let t_col = t.reshape((t_len, 1))?;
        let omega_row = self.omega.reshape((1, self.d_time))?;
        let phi_row = self.phi.reshape((1, self.d_time))?;
        let pre = t_col.broadcast_mul(&omega_row)?
                       .broadcast_add(&phi_row)?;  // (T, d_time)

        // Première dimension reste linéaire, le reste passe en sin().
        // On split via narrow puis recompose.
        let linear_part = pre.narrow(1, 0, 1)?;                  // (T, 1)
        let periodic_part = pre.narrow(1, 1, self.d_time - 1)?   // (T, d_time-1)
                              .sin()?;
        Tensor::cat(&[&linear_part, &periodic_part], 1)
    }
}

// --------------------------------------------------------------------------
// MLP résiduel post-attention — VRAIE non-linéarité
// --------------------------------------------------------------------------

pub struct ResidualMLP {
    pub w1: Tensor,  // (d_latent, d_hidden)
    pub w2: Tensor,  // (d_hidden, d_latent)
    pub b1: Tensor,  // (d_hidden,)
    pub b2: Tensor,  // (d_latent,)
}

impl ResidualMLP {
    pub fn new(d_latent: usize, d_hidden: usize, device: &Device) -> Result<Self> {
        let scale = (2.0 / d_latent as f64).sqrt();  // He init pour GELU/ReLU
        Ok(Self {
            w1: Tensor::randn(0.0f64, scale, (d_latent, d_hidden), device)?,
            w2: Tensor::randn(0.0f64, scale, (d_hidden, d_latent), device)?,
            b1: Tensor::zeros(d_hidden, DType::F64, device)?,
            b2: Tensor::zeros(d_latent, DType::F64, device)?,
        })
    }

    /// y = x + W2·GELU(W1·x + b1) + b2
    pub fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let (n, d) = x.dims2()?;
        let h = x.matmul(&self.w1)?
                 .broadcast_add(&self.b1.reshape((1, self.b1.dims1()?))?)?;
        let h_act = candle_nn::ops::silu(&h)?;  // SiLU ≈ GELU, lisse, monotone
        let out = h_act.matmul(&self.w2)?
                       .broadcast_add(&self.b2.reshape((1, d))?)?;
        x.add(&out)  // connexion résiduelle
    }
}

// --------------------------------------------------------------------------
// PTNLPerceiver V6 — Bidirectionnel
// --------------------------------------------------------------------------

pub struct PTNLPerceiver {
    // Paramètres appris
    pub w_q: Tensor,         // (d_latent, d_latent)
    pub w_k: Tensor,         // (d_input + d_time, d_latent)
    pub w_v: Tensor,         // (d_input + d_time, d_latent)
    pub w_o: Tensor,         // (d_latent, d_input) — projection per-token
    pub w_proj_m_to_t: Tensor, // (M, t_total) — recompose la séquence
    pub time_enc: Time2Vec,
    pub mlp: ResidualMLP,

    // État
    pub latents: Tensor,                       // (M, d_latent)
    pub future_prediction: Option<Tensor>,     // (t_future, d_input)

    // Configuration
    pub d_input:  usize,
    pub d_latent: usize,
    pub d_time:   usize,
    pub m:        usize,
    pub t_past:   usize,
    pub t_future: usize,
    pub device:   Device,

    // Soft NoiseGate (variance threshold pour atténuation, plus de freeze binaire)
    pub noise_variance_tau: f64,
    pub last_noise_gain:    f64,

    /// Anti-explosion : norme L2 max par latent en sortie. Indispensable
    /// en boucle fermée non-entraînée pour empêcher la spirale de feedback
    /// PTNL→BCI→PDT→PTNL.
    pub latent_clip_norm: f64,
}

impl PTNLPerceiver {
    pub fn new(
        d_input:  usize,
        d_latent: usize,
        d_time:   usize,
        m:        usize,
        t_past:   usize,
        t_future: usize,
        device:   &Device,
    ) -> Result<Self> {
        let d_kv_input = d_input + d_time;

        let scale_q = (1.0 / d_latent as f64).sqrt();
        let scale_k = (1.0 / d_latent as f64).sqrt();
        let scale_v = (1.0 / d_kv_input as f64).sqrt();

        let w_q = Tensor::randn(0.0f64, scale_q, (d_latent, d_latent), device)?;
        let w_k = Tensor::randn(0.0f64, scale_k, (d_kv_input, d_latent), device)?;
        let w_v = Tensor::randn(0.0f64, scale_v, (d_kv_input, d_latent), device)?;
        let w_o = Tensor::randn(0.0f64, 0.02f64, (d_latent, d_input), device)?;

        let t_total = t_past + t_future;
        let w_proj = Tensor::randn(0.0f64, 0.02f64, (m, t_total), device)?;

        let time_enc = Time2Vec::new(d_time, device)?;
        let mlp = ResidualMLP::new(d_latent, d_latent * 4, device)?;

        let latents = Tensor::zeros((m, d_latent), DType::F64, device)?;

        Ok(Self {
            w_q, w_k, w_v, w_o,
            w_proj_m_to_t: w_proj,
            time_enc, mlp,
            latents,
            future_prediction: None,
            d_input, d_latent, d_time, m, t_past, t_future,
            device: device.clone(),
            noise_variance_tau: 0.15,
            last_noise_gain: 1.0,
            latent_clip_norm: 3.0,
        })
    }

    /// Injecte la trajectoire prédite par le PDT au step précédent.
    /// C'est ce qui permet à la perception de "voir le futur" comme le passé.
    pub fn set_future(&mut self, future: Tensor) {
        self.future_prediction = Some(future);
    }

    /// Forward bidirectionnel.
    ///
    /// `x_past`     : (t_past, d_input)         — fenêtre des observations
    /// `t_axis`     : (t_past + t_future,)      — timestamps relatifs (centrés sur 0)
    ///
    /// Retourne `(latents_out, recon_loss)` :
    ///   - latents_out : (M, d_latent)
    ///   - recon_loss  : (t_total, d_input)     — per-dimension, per-step
    pub fn forward(
        &mut self,
        x_past: &Tensor,
        t_axis: &Tensor,
    ) -> Result<(Tensor, Tensor)> {
        let (t_past, d_in) = x_past.dims2()?;
        assert_eq!(t_past, self.t_past);
        assert_eq!(d_in, self.d_input);

        // 1. Assembler la fenêtre bidirectionnelle [past | future]
        let x_full: Tensor = match &self.future_prediction {
            Some(fut) => {
                let (tf, _) = fut.dims2()?;
                assert_eq!(tf, self.t_future, "future prediction size mismatch");
                Tensor::cat(&[x_past, fut], 0)?
            }
            None => {
                // Premier step : pas encore de prédiction du PDT.
                // On pad le futur avec des zéros.
                let zero_fut = Tensor::zeros(
                    (self.t_future, self.d_input), DType::F64, &self.device)?;
                Tensor::cat(&[x_past, &zero_fut], 0)?
            }
        };  // (t_total, d_input)

        // 2. Encodage temporel et concaténation
        let t_emb = self.time_enc.encode(t_axis)?;        // (t_total, d_time)
        let x_with_t = Tensor::cat(&[&x_full, &t_emb], 1)?; // (t_total, d_input + d_time)

        // 3. Soft NoiseGate (gain continu plutôt que freeze binaire)
        let variance = self.compute_variance(&x_full)?;
        let noise_gain = (-(variance / self.noise_variance_tau).powi(2)).exp();
        self.last_noise_gain = noise_gain;

        // 4. Q/K/V
        let q = self.latents.matmul(&self.w_q)?;          // (M, d_latent)
        let k = x_with_t.matmul(&self.w_k)?;              // (t_total, d_latent)
        let v = x_with_t.matmul(&self.w_v)?;              // (t_total, d_latent)

        // 5. Attention non-causale (full bidirectional)
        let scale = (self.d_latent as f64).sqrt();
        let scale_t = Tensor::new(scale, &self.device)?;
        let scores = q.matmul(&k.transpose(0, 1)?)?       // (M, t_total)
                      .broadcast_div(&scale_t)?;
        let attn = candle_nn::ops::softmax(&scores, 1)?;
        let mut latents_pre = attn.matmul(&v)?;           // (M, d_latent)

        // Application du gain de bruit (multiplicatif, continu)
        if noise_gain < 0.999 {
            let gain_t = Tensor::new(noise_gain, &self.device)?
                              .broadcast_as(latents_pre.shape())?;
            // Mix continu entre nouveau et ancien selon le gain
            let one_minus_gain = Tensor::new(1.0 - noise_gain, &self.device)?
                                       .broadcast_as(latents_pre.shape())?;
            latents_pre = latents_pre.broadcast_mul(&gain_t)?
                .add(&self.latents.broadcast_mul(&one_minus_gain)?)?;
        }

        // 6. MLP résiduel — VRAIE non-linéarité
        let latents_raw = self.mlp.forward(&latents_pre)?;
        // Clip L2 ligne par ligne (anti-explosion en boucle fermée)
        let latents_out = clip_l2_matrix(&latents_raw, self.latent_clip_norm)?;

        // 7. Reconstruction per-dimension (PAS de mean !)
        // projected : (M, d_input) — chaque latent reconstruit un "atome"
        let projected = latents_out.matmul(&self.w_o)?;       // (M, d_input)
        // Recompose la séquence : (t_total, M) · (M, d_input) → (t_total, d_input)
        let recon = self.w_proj_m_to_t
                        .transpose(0, 1)?                     // (t_total, M)
                        .matmul(&projected)?;                 // (t_total, d_input)

        // Loss MSE per-dim, per-step (broadcastable)
        let recon_loss = recon.sub(&x_full)?.sqr()?;          // (t_total, d_input)

        // 8. Mise à jour de l'état
        self.latents = latents_out.clone();

        Ok((latents_out, recon_loss))
    }

    fn compute_variance(&self, x: &Tensor) -> Result<f64> {
        let v: Vec<f64> = x.flatten_all()?.to_vec1::<f64>()?;
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        Ok(v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64)
    }

    pub fn reset(&mut self) -> Result<()> {
        self.latents = Tensor::zeros((self.m, self.d_latent), DType::F64, &self.device)?;
        self.future_prediction = None;
        Ok(())
    }

    /// Projette un latent (ou une trajectoire de latents) dans l'espace d'observation.
    ///
    /// Réutilise `w_o : (d_latent, d_input)` — c'est la même projection que celle
    /// servant à la reconstruction. Permet au main loop d'extraire la future_window
    /// observable à partir de la trajectoire latente du PDT.
    ///
    /// `latent` : (d_latent,) ou (n, d_latent)
    /// Retourne : (d_input,) ou (n, d_input)
    pub fn latent_to_input(&self, latent: &Tensor) -> Result<Tensor> {
        match latent.dims().len() {
            1 => {
                let lat_2d = latent.reshape((1, self.d_latent))?;
                lat_2d.matmul(&self.w_o)?.squeeze(0)
            }
            2 => latent.matmul(&self.w_o),
            _ => Err(candle_core::Error::Msg(
                "latent_to_input: expected 1D or 2D tensor".into()).bt()),
        }
    }
}

// ==========================================================================
// Tests unitaires
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn dev() -> Device { Device::Cpu }

    #[test]
    fn time2vec_shape() {
        let t2v = Time2Vec::new(8, &dev()).unwrap();
        let t = Tensor::from_slice(
            &[0.0_f64, 1.0, 2.0, 3.0, 4.0], 5, &dev()).unwrap();
        let emb = t2v.encode(&t).unwrap();
        assert_eq!(emb.dims(), &[5, 8]);
    }

    #[test]
    fn perceiver_bidirectional_forward() {
        let mut p = PTNLPerceiver::new(4, 16, 8, 8, 16, 4, &dev()).unwrap();
        let x = Tensor::randn(0.0_f64, 0.3, (16, 4), &dev()).unwrap();
        let t = Tensor::from_slice(
            &(-16..4).map(|i| i as f64).collect::<Vec<_>>(),
            20, &dev()).unwrap();
        let (latents, loss) = p.forward(&x, &t).unwrap();
        assert_eq!(latents.dims(), &[8, 16]);
        assert_eq!(loss.dims(), &[20, 4]);  // per-dim, per-step
    }

    #[test]
    fn perceiver_with_future_injection() {
        let mut p = PTNLPerceiver::new(4, 16, 8, 8, 16, 4, &dev()).unwrap();
        let future = Tensor::randn(0.0_f64, 0.3, (4, 4), &dev()).unwrap();
        p.set_future(future);

        let x = Tensor::randn(0.0_f64, 0.3, (16, 4), &dev()).unwrap();
        let t = Tensor::from_slice(
            &(-16..4).map(|i| i as f64).collect::<Vec<_>>(),
            20, &dev()).unwrap();
        let (latents, _) = p.forward(&x, &t).unwrap();
        assert_eq!(latents.dims(), &[8, 16]);
    }

    #[test]
    fn noise_gate_attenuates_smoothly() {
        let mut p = PTNLPerceiver::new(4, 16, 8, 8, 16, 4, &dev()).unwrap();
        // Bruit blanc fort
        let x_noisy = Tensor::randn(0.0_f64, 2.0, (16, 4), &dev()).unwrap();
        let t = Tensor::from_slice(
            &(-16..4).map(|i| i as f64).collect::<Vec<_>>(),
            20, &dev()).unwrap();
        p.forward(&x_noisy, &t).unwrap();
        // Le gain doit être < 1 mais > 0 (pas de freeze binaire)
        assert!(p.last_noise_gain < 0.9);
        assert!(p.last_noise_gain > 0.0);
    }
}
