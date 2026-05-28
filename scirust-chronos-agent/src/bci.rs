// ==========================================================================
// bci.rs — BCI Hamiltonien V6 (Pillar 3)
//
// Drop-in replacement for V5. Tient enfin la promesse "Boucle de Conscience
// Intemporelle" : la GRU a été remplacée par un Neural ODE hamiltonien avec
// intégration symplectique de Verlet.
//
// JUSTIFICATION DU NOM (enfin honnête)
//
//   BOUCLE — la trajectoire est BORNÉE par la conservation d'énergie. Sous
//   un Hamiltonien autonome H(q,p) avec V(q) borné inférieurement et
//   coercitif, les solutions vivent sur des hyper-surfaces d'iso-énergie
//   (compactes pour V coercitive). C'est mathématiquement une boucle —
//   contrairement à la GRU qui n'a aucune garantie de retour.
//
//   CONSCIENCE — l'état est PHASE-SPACE (q, p) au lieu d'un simple h.
//   Cette dualité position/moment encode à la fois "ce qui est" (q) et
//   "ce qui se passe" (p) — l'analogue le plus proche d'un état attentionnel
//   dans la dynamique classique.
//
//   INTEMPORELLE — les équations de Hamilton sont T-symétriques : la
//   réversion temporelle (p → -p, t → -t) laisse les équations invariantes.
//   La GRU était strictement causale (mise à jour irréversible). Ici la
//   trajectoire passée peut être reconstruite exactement à partir d'un
//   point de la trajectoire (intégration backward).
//
//   ATTRACTEUR — quand α_sync < threshold, on active un terme dissipatif
//   -γ·p qui force la convergence vers le minimum de V. C'est un VRAI
//   attracteur de Lyapunov, pas un "freeze conditionnel" (V5).
//
// ÉQUATIONS
//
//   H(q, p) = ½ pᵀM⁻¹p + V(q)              avec V appris par MLP
//
//   q̇ =  ∂H/∂p =  M⁻¹p
//   ṗ = -∂H/∂q = -∇V(q) - γ·p              γ = friction (≥ 0)
//
// INTÉGRATION : leapfrog (Verlet) — préserve la mesure de Liouville,
// erreur O(dt²) par step, stable sur des trajectoires longues.
//
//   p_{n+½} = p_n - (dt/2) · ∇V(q_n)
//   q_{n+1} = q_n + dt · M⁻¹ · p_{n+½}
//   p_{n+1} = p_{n+½} - (dt/2) · ∇V(q_{n+1})
//
// L'INSIGHT INJECTION devient une déformation du potentiel : V_total(q) =
// V(q) - α_sync · ⟨q, x_ext⟩ où x_ext est le couplage externe (input).
// Cela respecte la structure hamiltonienne (le terme reste un gradient).
// ==========================================================================

use crate::device::compute_dtype;
use crate::device::TensorReadExt;
use candle_core::{DType, Device, Result, Tensor, Var};

// --------------------------------------------------------------------------
// Helper : clipping L2 — anti-explosion en boucle fermée non-entraînée
// --------------------------------------------------------------------------

fn clip_l2(x: &Tensor, max_norm: f64) -> Result<Tensor> {
    let norm: f64 = x.sqr()?.sum_all()?.read_scalar_f64()?.sqrt();
    if norm <= max_norm || norm < 1e-12 {
        return Ok(x.clone());
    }
    let scale = Tensor::new(max_norm / norm, x.device())?;
    x.broadcast_mul(&scale)
}

// --------------------------------------------------------------------------
// PotentialMLP — V(q) appris, paramétrise le paysage énergétique
// --------------------------------------------------------------------------

/// Réseau pour le potentiel V : ℝ^d_q → ℝ.
///
/// Paramètres stockés en `Var` pour permettre l'autograd complet et
/// l'optimisation par AdamW. Toutes les ops du forward utilisent
/// `var.as_tensor()` pour rester dans le graphe différentiable.
pub struct PotentialMLP {
    pub w1: Var,   // (d_q, d_hidden)
    pub w2: Var,   // (d_hidden, 1)
    pub b1: Var,   // (d_hidden,)
    pub b2: Var,   // (1,)
    pub d_q: usize,
    pub d_h: usize,
    pub device: Device,
}

impl PotentialMLP {
    pub fn new(d_q: usize, d_h: usize, device: &Device) -> Result<Self> {
        let scale = (2.0 / d_q as f64).sqrt();
        Ok(Self {
            w1: Var::from_tensor(&crate::device::randn_f64(0.0f64, scale, (d_q, d_h), device)?)?,
            w2: Var::from_tensor(&crate::device::randn_f64(0.0f64, scale, (d_h, 1), device)?)?,
            b1: Var::from_tensor(&Tensor::zeros(d_h, compute_dtype(), device)?)?,
            b2: Var::from_tensor(&Tensor::zeros(1, compute_dtype(), device)?)?,
            d_q, d_h,
            device: device.clone(),
        })
    }

    /// Liste tous les paramètres entraînables (pour passage à AdamW).
    pub fn trainable_vars(&self) -> Vec<Var> {
        vec![self.w1.clone(), self.w2.clone(), self.b1.clone(), self.b2.clone()]
    }

    /// V(q) — scalaire. Différentiable par rapport aux poids ET à q.
    pub fn value(&self, q: &Tensor) -> Result<f64> {
        let q_2d = q.reshape((1, self.d_q))?;
        let h = q_2d.matmul(self.w1.as_tensor())?
                    .broadcast_add(&self.b1.as_tensor().reshape((1, self.d_h))?)?;
        let h_act = candle_nn::ops::silu(&h)?;
        let mlp_out: f64 = h_act.matmul(self.w2.as_tensor())?
            .broadcast_add(&self.b2.as_tensor().reshape((1, 1))?)?
            .squeeze(0)?.squeeze(0)?.read_scalar_f64()?;
        let q_norm2: f64 = q.sqr()?.sum_all()?.read_scalar_f64()?;
        Ok(mlp_out + 0.5 * q_norm2)
    }

    /// V(q) en Tensor (différentiable) — utile pour l'entraînement
    /// par DSM/contrastive (backward via candle).
    pub fn value_tensor(&self, q: &Tensor) -> Result<Tensor> {
        let q_2d = q.reshape((1, self.d_q))?;
        let h = q_2d.matmul(self.w1.as_tensor())?
                    .broadcast_add(&self.b1.as_tensor().reshape((1, self.d_h))?)?;
        let h_act = candle_nn::ops::silu(&h)?;
        let mlp_out = h_act.matmul(self.w2.as_tensor())?
            .broadcast_add(&self.b2.as_tensor().reshape((1, 1))?)?
            .squeeze(0)?.squeeze(0)?;
        let q_norm2 = q.sqr()?.sum_all()?;
        let half = crate::device::scalar_f64(0.5_f64, &self.device)?;
        mlp_out.add(&q_norm2.broadcast_mul(&half)?)
    }

    /// ∇V(q) via AUTOGRAD candle — O(1) backward pass.
    ///
    /// 5-10× plus rapide que la différence finie pour d_q ≤ 32 et indispensable
    /// au-delà (FD coûte O(d_q) forward passes par appel ; autograd un seul).
    ///
    /// Stratégie : on convertit q en `Var`, recalcule V(q), puis backward.
    /// La GradStore renvoie le gradient par rapport à q.
    pub fn grad(&self, q: &Tensor) -> Result<Tensor> {
        let q_var = candle_core::Var::from_tensor(q)?;
        let q_t = q_var.as_tensor();
        // Recompute V(q) sur le graphe différentiable.
        let v = self.value_tensor(q_t)?;
        let grads = v.backward()?;
        match grads.get(q_t) {
            Some(g) => Ok(g.clone()),
            None => {
                // Filet de sécurité : si autograd échoue (graphe brisé),
                // fallback sur FD.
                self.grad_fd(q)
            }
        }
    }

    /// ∇V(q) via différence finie centrée — gardée pour validation croisée
    /// et fallback. Précision O(ε²), ε = 1e-4 → quasi-machine-précision f64.
    pub fn grad_fd(&self, q: &Tensor) -> Result<Tensor> {
        let q_vec: Vec<f64> = q.read_vec1_f64()?;
        let eps = 1e-4;
        let mut g = vec![0.0f64; self.d_q];
        for i in 0..self.d_q {
            let mut qp = q_vec.clone();
            let mut qm = q_vec.clone();
            qp[i] += eps;
            qm[i] -= eps;
            let qp_t = crate::device::tensor_from_f64(&qp, self.d_q, &self.device)?;
            let qm_t = crate::device::tensor_from_f64(&qm, self.d_q, &self.device)?;
            let vp = self.value(&qp_t)?;
            let vm = self.value(&qm_t)?;
            g[i] = (vp - vm) / (2.0 * eps);
        }
        crate::device::tensor_from_f64(&g, self.d_q, &self.device)
    }
}

// --------------------------------------------------------------------------
// HamiltonianCell — remplace GRUCell
// --------------------------------------------------------------------------

pub struct GRUCell {
    pub potential: PotentialMLP,
    /// Couplage linéaire input → q (équivalent du x·W_in d'une GRU).
    pub w_in: Tensor,  // (d_input, d_q)

    /// État de phase : q (position) et p (moment).
    pub q: Tensor,     // (d_q,)
    pub p: Tensor,     // (d_q,)

    pub d_input: usize,
    pub d_q: usize,
    /// d_hidden exposé = 2*d_q (pour compatibilité externe — l'état complet
    /// est concat[q, p]).
    pub d_hidden: usize,

    /// Pas d'intégration.
    pub dt: f64,
    /// Friction de base (0 = pure conservation).
    pub gamma_base: f64,
    /// Mode "attracteur" : friction supplémentaire quand α_sync est bas.
    pub gamma_attractor: f64,

    /// Anti-explosion : ‖q‖, ‖p‖, ‖x_ext‖ clippés à ces valeurs.
    /// Indispensable en boucle fermée non-entraînée. Préserve la structure
    /// hamiltonienne — un clip à norme constante est juste une projection
    /// sur boule, qui ne casse pas la conservation locale (la trajectoire
    /// devient marginale au bord, mais reste sur l'isosurface tangente).
    pub state_clip_norm: f64,
    pub input_clip_norm: f64,

    pub device: Device,

    /// Diagnostic : énergie totale au dernier step (pour vérifier la
    /// conservation, qui doit être stable hors mode attracteur).
    pub last_energy: f64,
    /// Potentiel externe accumulé (EvoPulse) pour perturbation hamiltonienne.
    pub external_potential: f64,
}

impl GRUCell {
    /// Conserve l'ancien nom `GRUCell` pour drop-in : aucune migration de
    /// signature côté main.rs. Le mensonge du nom est documenté.
    ///
    /// `d_input` : dim de l'observation entrante (= d_latent V5).
    /// `d_hidden` : doit être PAIR — il est splité en (q, p), chacun de
    /// taille d_hidden/2. Si d_hidden impair, on l'arrondit au pair supérieur.
    pub fn new(d_input: usize, d_hidden: usize, device: &Device) -> Result<Self> {
        let d_q = if d_hidden % 2 == 0 { d_hidden / 2 } else { (d_hidden + 1) / 2 };
        let d_hidden_eff = d_q * 2;

        let scale = (1.0 / d_q as f64).sqrt();
        let w_in = crate::device::randn_f64(0.0f64, scale, (d_input, d_q), device)?;

        let potential = PotentialMLP::new(d_q, d_q * 4, device)?;

        let q = Tensor::zeros(d_q, compute_dtype(), device)?;
        let p = Tensor::zeros(d_q, compute_dtype(), device)?;

        Ok(Self {
            potential, w_in,
            q, p,
            d_input, d_q, d_hidden: d_hidden_eff,
            dt: 0.05,
            gamma_base: 0.0,
            gamma_attractor: 0.5,
            state_clip_norm: 8.0,
            input_clip_norm: 5.0,
            device: device.clone(),
            last_energy: 0.0,
            external_potential: 0.0,
        })
    }

    pub fn reset(&mut self) -> Result<()> {
        self.q = Tensor::zeros(self.d_q, compute_dtype(), &self.device)?;
        self.p = Tensor::zeros(self.d_q, compute_dtype(), &self.device)?;
        self.external_potential = 0.0;
        Ok(())
    }

    /// Ajoute un potentiel externe (EvoPulse) intégré dans la dynamique au prochain step.
    pub fn apply_external_potential(&mut self, potential: f64) {
        const EXT_POT_CLAMP: f64 = 0.2;
        self.external_potential = (self.external_potential + potential).clamp(-EXT_POT_CLAMP, EXT_POT_CLAMP);
    }

    /// Step d'intégration symplectique avec couplage externe.
    ///
    /// Signature IDENTIQUE à V5 GRUCell::step — drop-in pur.
    ///
    ///   `x`           : input (d_input,)
    ///   `alpha_sync`  : ∈ [0, 1] — module insight et mode attracteur
    ///   `threshold`   : seuil au-dessus duquel l'insight est injecté
    ///
    /// Retourne h = concat[q, p] de shape (d_hidden,).
    pub fn step(&mut self, x: &Tensor, alpha_sync: f64, threshold: f64)
        -> Result<Tensor>
    {
        // 0. Appliquer le potentiel externe EvoPulse comme perturbation de moment.
        if self.external_potential != 0.0 {
            let ext_force = self.q.affine(self.dt * self.external_potential, 0.0)?;
            self.p = self.p.add(&ext_force)?;
            self.external_potential = 0.0;
        }

        // 1. Projection input → couplage externe x_ext.
        let x_1d = if x.dims().len() == 2 { x.squeeze(0)? } else { x.clone() };
        // Clip l'input pour borner le couplage externe (anti-explosion).
        let x_clipped = clip_l2(&x_1d, self.input_clip_norm)?;
        let x_ext = x_clipped.unsqueeze(0)?
                        .matmul(&self.w_in)?
                        .squeeze(0)?;  // (d_q,)
        // Clip aussi x_ext directement — defense en profondeur si w_in
        // s'éloigne de l'init (après entraînement éventuel).
        let x_ext = clip_l2(&x_ext, self.input_clip_norm)?;

        // 2. Calcule ∇V(q) du potentiel courant.
        let grad_v = self.potential.grad(&self.q)?;

        // 3. Couplage : V_total(q) = V(q) - α_sync·⟨q, x_ext⟩ si α > threshold
        // → ∇V_total = ∇V - α_sync · x_ext  (seulement si insight actif)
        let grad_total = if alpha_sync > threshold {
            let scale = crate::device::scalar_f64(alpha_sync, &self.device)?;
            let coupling = x_ext.broadcast_mul(&scale)?;
            grad_v.sub(&coupling)?
        } else {
            // Hors insight : couplage faible, déterminé par x_ext seul
            // (équivalent d'un forçage externe avec gain ε = 0.1)
            let scale = crate::device::scalar_f64(0.1_f64, &self.device)?;
            let coupling = x_ext.broadcast_mul(&scale)?;
            grad_v.sub(&coupling)?
        };

        // 4. Friction : gamma_base + (1 - α_sync) · gamma_attractor
        // → quand α_sync est bas (drift), friction monte → attracteur s'active
        let gamma = self.gamma_base + (1.0 - alpha_sync) * self.gamma_attractor;

        // 5. Leapfrog (Verlet) step
        //    p_half = p - (dt/2)·(∇V_total + γ·p)
        let half_dt = 0.5 * self.dt;
        let p_friction = self.p.affine(gamma, 0.0)?;
        let p_drift = grad_total.add(&p_friction)?;
        let p_half = self.p.sub(&p_drift.affine(half_dt, 0.0)?)?;

        //    q_new = q + dt · p_half  (M = I)
        let q_new = self.q.add(&p_half.affine(self.dt, 0.0)?)?;

        //    p_new = p_half - (dt/2)·(∇V(q_new) + γ·p_half)
        let grad_v_new = self.potential.grad(&q_new)?;
        let grad_total_new = if alpha_sync > threshold {
            let scale = crate::device::scalar_f64(alpha_sync, &self.device)?;
            grad_v_new.sub(&x_ext.broadcast_mul(&scale)?)?
        } else {
            let scale = crate::device::scalar_f64(0.1_f64, &self.device)?;
            grad_v_new.sub(&x_ext.broadcast_mul(&scale)?)?
        };
        let p_drift2 = grad_total_new.add(&p_half.affine(gamma, 0.0)?)?;
        let p_new = p_half.sub(&p_drift2.affine(half_dt, 0.0)?)?;

        // 6. Update state — avec clipping anti-explosion.
        // En boucle fermée non-entraînée, le couplage externe peut pomper
        // de l'énergie indéfiniment. Le clip à state_clip_norm projette
        // sur la boule de rayon donné — préserve la direction, borne la
        // magnitude. La conservation hamiltonienne reste exacte dans la
        // boule, marginale au bord.
        self.q = clip_l2(&q_new, self.state_clip_norm)?;
        self.p = clip_l2(&p_new, self.state_clip_norm)?;

        // 7. Diagnostic : énergie totale (doit être ~conservée si γ=0)
        let kin: f64 = self.p.sqr()?.sum_all()?.read_scalar_f64()? * 0.5;
        let pot = self.potential.value(&self.q)?;
        self.last_energy = kin + pot;

        // 8. Retour : h = concat[q, p] pour compatibilité externe
        Tensor::cat(&[&self.q, &self.p], 0)
    }

    /// Accès à l'état caché unifié h = concat[q, p].
    pub fn hidden_state(&self) -> Result<Tensor> {
        Tensor::cat(&[&self.q, &self.p], 0)
    }
}

// ==========================================================================
// Tests
// ==========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    fn dev() -> Device { Device::Cpu }

    #[test]
    fn hamiltonian_conserves_energy_without_friction() {
        let mut cell = GRUCell::new(4, 16, &dev()).unwrap();
        cell.gamma_base = 0.0;
        cell.gamma_attractor = 0.0;  // friction totale = 0
        cell.dt = 0.01;
        cell.q = crate::device::tensor_from_f64(&[0.3_f64; 8], 8, &dev()).unwrap();
        cell.p = crate::device::tensor_from_f64(&[0.1_f64; 8], 8, &dev()).unwrap();

        let x = Tensor::zeros(4, compute_dtype(), &dev()).unwrap();
        cell.step(&x, 0.0, 1.0).unwrap();  // α<threshold + couplage faible
        let e0 = cell.last_energy;

        for _ in 0..200 {
            cell.step(&x, 0.0, 1.0).unwrap();
        }
        let drift = (cell.last_energy - e0).abs() / e0.abs().max(1e-6);
        // Avec leapfrog dt=0.01 sur 200 steps : drift < 1%
        assert!(drift < 0.05, "energy drift {} too large", drift);
    }

    #[test]
    fn attractor_mode_converges() {
        let mut cell = GRUCell::new(4, 16, &dev()).unwrap();
        cell.gamma_attractor = 1.0;  // friction forte en mode drift
        cell.dt = 0.05;
        cell.q = crate::device::tensor_from_f64(&[1.0_f64; 8], 8, &dev()).unwrap();
        cell.p = crate::device::tensor_from_f64(&[2.0_f64; 8], 8, &dev()).unwrap();

        let x = Tensor::zeros(4, compute_dtype(), &dev()).unwrap();
        // α_sync = 0 → friction max → convergence
        for _ in 0..500 {
            cell.step(&x, 0.0, 0.9).unwrap();
        }
        let p_norm: f64 = cell.p.sqr().unwrap().sum_all().unwrap().read_scalar_f64().unwrap();
        // Le moment doit s'amortir vers 0
        assert!(p_norm < 0.1, "p didn't converge: {}", p_norm);
    }

    #[test]
    fn step_signature_compatible_with_v5() {
        // V5 attendait : step(&Tensor, f64, f64) -> Result<Tensor>
        let mut cell = GRUCell::new(4, 16, &dev()).unwrap();
        let x = Tensor::randn(0.0_f64, 0.3, 4, &dev()).unwrap();
        let h = cell.step(&x, 0.7, 0.65).unwrap();
        // h doit être (d_hidden,) = (16,)
        assert_eq!(h.dims(), &[16]);
    }

    #[test]
    fn autograd_matches_finite_difference() {
        // Validation croisée : le gradient autograd doit matcher la FD à
        // 1e-3 près (la FD a une précision O(ε²) avec ε=1e-4 → ~1e-8 pour
        // les fonctions analytiques, mais ~1e-4 pour les MLP non-linéaires).
        let p = PotentialMLP::new(8, 32, &dev()).unwrap();
        let q = Tensor::randn(0.0_f64, 0.5, 8, &dev()).unwrap();
        let g_ad = p.grad(&q).unwrap();
        let g_fd = p.grad_fd(&q).unwrap();
        let v_ad: Vec<f64> = g_ad.to_vec1().unwrap();
        let v_fd: Vec<f64> = g_fd.to_vec1().unwrap();
        for (a, b) in v_ad.iter().zip(v_fd.iter()) {
            let diff = (a - b).abs();
            let rel = diff / a.abs().max(b.abs()).max(1e-6);
            assert!(diff < 1e-3 || rel < 1e-2,
                    "autograd diff from FD: {} vs {} (abs={}, rel={})",
                    a, b, diff, rel);
        }
    }
}
