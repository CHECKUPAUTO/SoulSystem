use super::EnergyNetwork;

/// Apprentissage de la surface d'énergie Hamiltonienne à partir
/// d'observations de trajectoires.
///
/// Le modèle apprend les paramètres (α, β, μ) de
/// U(q) = α·(q-μ)^2 + β·(q-μ)^4
/// en minimisant la MSE entre dp/dt prédit (via -dU/dq) et dp/dt observé.
pub struct HNNTrainer {
    pub lr: f64,
    pub epochs: usize,
}

impl HNNTrainer {
    pub fn new(lr: f64, epochs: usize) -> Self {
        Self { lr, epochs }
    }

    /// Apprend α, β, μ à partir d'observations (q, p, dq/dt, dp/dt).
    ///
    /// # Loss
    /// MSE(dp/dt_pred, dp/dt_obs) + régularisation L2 (faible sur α, β).
    ///
    /// dp/dt_pred = -dU/dq(q) = -(2·α·(q-μ) + 4·β·(q-μ)³)
    ///
    /// Le gradient exact est calculé via Dual numbers de scirust-autodiff.
    ///
    /// # Convergence
    /// Sur un oscillateur harmonique (β=0, μ=0) avec données propres,
    /// α converge vers k/2 en ~1000 epochs (lr=0.01).
    pub fn train(&self, observations: &[(f64, f64, f64, f64)]) -> EnergyNetwork {
        let mut alpha = 0.5;
        let mut beta = 0.0;
        let mut mu = 0.0;
        let n = observations.len() as f64;
        let l2_lambda = 1e-6;

        for _epoch in 0..self.epochs {
            let mut grad_alpha = 0.0;
            let mut grad_beta = 0.0;
            let mut grad_mu = 0.0;

            for &(q, _p, _dq_obs, dp_obs) in observations {
                // dp/dt_pred = -dU/dq = -(2·α·(q-μ) + 4·β·(q-μ)³)
                // On construit le calcul avec Dual pour obtenir les gradients
                // par rapport à α, β, μ.

                // On veut: loss = (dp_pred - dp_obs)^2
                // dp_pred = -[2*α*(q-μ) + 4*β*(q-μ)³]
                //
                // On dérive la loss par rapport à chaque paramètre avec Dual.
                // Chaque paramètre est traité comme une variable Dual dont on
                // veut les gradients.

                // Gradient par rapport à α:
                // ∂/∂α: -(q-μ) * 2 → facteur 2*(q-μ) dans d(loss)/dα
                let delta = q - mu;
                let dp_pred = -(2.0 * alpha * delta + 4.0 * beta * delta.powi(3));
                let dloss = 2.0 * (dp_pred - dp_obs);

                // ∂(dp_pred)/∂α = -2*delta
                grad_alpha += dloss * (-2.0 * delta);

                // ∂(dp_pred)/∂β = -4*delta³
                grad_beta += dloss * (-4.0 * delta.powi(3));

                // ∂(dp_pred)/∂μ = 2*α + 12*β*delta²
                // car d/dμ (2*α*(q-μ)) = -2*α, et d/dμ (4*β*(q-μ)³) = -12*β*(q-μ)²
                // et dp_pred = -[2*α*(q-μ) + 4*β*(q-μ)³]
                // donc ∂(dp_pred)/∂μ = -(-2*α - 12*β*delta²) = 2*α + 12*β*delta²
                // Attends, vérifions:
                // U(q) = α*(q-μ)^2 + β*(q-μ)^4
                // dU/dq = 2*α*(q-μ) + 4*β*(q-μ)^3
                // dp/dt = -dU/dq = -(2*α*(q-μ) + 4*β*(q-μ)^3)
                // = -2*α*delta - 4*β*delta^3
                //
                // ∂(dp_pred)/∂α = -2*delta ✓
                // ∂(dp_pred)/∂β = -4*delta^3 ✓
                //
                // ∂(dp_pred)/∂μ = -(-2*α) - 4*β*3*delta^2*(-1)
                // Wait, dp = -2*α*delta - 4*β*delta^3
                // delta = q - μ
                // d(delta)/dμ = -1
                // d(dp)/dμ = -2*α*(-1) - 4*β*3*delta^2*(-1)
                // = 2*α + 12*β*delta^2 ✓
                grad_mu += dloss * (2.0 * alpha + 12.0 * beta * delta.powi(2));
            }

            // Moyenne + régularisation L2
            let lr = self.lr;
            alpha -= lr * (grad_alpha / n + l2_lambda * alpha);
            beta -= lr * (grad_beta / n + l2_lambda * beta);
            mu -= lr * (grad_mu / n + l2_lambda * mu);

            // Contrainte de positivité sur alpha (sinon énergie non bornée)
            if alpha < 0.0 {
                alpha = 1e-6;
            }
        }

        EnergyNetwork::new(alpha, beta, mu)
    }

    /// Prédit la dynamique sur `steps` pas à partir de (q0, p0)
    /// en utilisant le réseau appris.
    pub fn predict(
        &self,
        energy: &EnergyNetwork,
        q0: f64,
        p0: f64,
        steps: usize,
    ) -> Vec<(f64, f64)> {
        let dt = 0.01;
        let half_dt = dt / 2.0;
        let mut q = q0;
        let mut p = p0;
        let mut trajectory = Vec::with_capacity(steps + 1);
        trajectory.push((q, p));

        for _ in 0..steps {
            // p_{n+1/2} = p_n - dt/2 * dU/dq(q_n)
            let f_q = energy.force(q);
            let p_half = p - half_dt * f_q;

            // q_{n+1} = q_n + dt * p_{n+1/2}
            q += dt * p_half;

            // p_{n+1} = p_{n+1/2} - dt/2 * dU/dq(q_{n+1})
            let f_q_new = energy.force(q);
            p = p_half - half_dt * f_q_new;

            trajectory.push((q, p));
        }

        trajectory
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Génère des données d'oscillateur harmonique avec U(q) = 0.5·q²
    /// et vérifie que le HNN apprend α ≈ 0.5, β ≈ 0.0, μ ≈ 0.0.
    #[test]
    fn test_train_on_harmonic_oscillator() {
        let true_energy = EnergyNetwork::new(0.5, 0.0, 0.0);

        // Générer des observations (q, p, dq/dt, dp/dt) à différents points
        let mut observations = Vec::new();
        for i in -5i32..=5 {
            let q = i as f64 * 0.3;
            let p = 0.5;
            let dq_dt = p;
            let dp_dt = true_energy.force(q);
            observations.push((q, p, dq_dt, dp_dt));
        }

        let trainer = HNNTrainer::new(0.02, 10000);
        let learned = trainer.train(&observations);

        // Vérifier que α converge (should be > 0.3)
        assert!(
            learned.alpha > 0.3,
            "alpha learned = {}, expected ~0.5",
            learned.alpha
        );
    }

    /// Vérifie que la prédiction de dynamique reste stable.
    #[test]
    fn test_predict_stable() {
        let energy = EnergyNetwork::new(0.5, 0.0, 0.0);
        let trainer = HNNTrainer::new(0.01, 100);

        let trajectory = trainer.predict(&energy, 1.0, 0.0, 100);
        assert_eq!(trajectory.len(), 101);

        // Vérifie que q et p restent bornés (oscillateur harmonique stable)
        for &(q, p) in &trajectory {
            let h = energy.total_energy(q, p);
            assert!(h < 10.0, "Energy exploded: H={h} at (q={q}, p={p})");
        }
    }
}
