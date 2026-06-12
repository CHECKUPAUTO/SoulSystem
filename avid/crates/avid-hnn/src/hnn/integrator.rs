use super::EnergyNetwork;

/// Intégrateur symplectique Verlet (Velocity-Verlet) pour HNN.
///
/// Préserve la symplecticité (structure symplectique du flot hamiltonien),
/// ce qui garantit la conservation de l'énergie sur le long terme
/// (pas de dissipation artificielle).
#[derive(Debug, Clone)]
pub struct SymplecticIntegrator {
    pub dt: f64,
    pub steps: usize,
}

impl SymplecticIntegrator {
    pub fn new(dt: f64, steps: usize) -> Self {
        Self { dt, steps }
    }

    /// Intègre les équations dq/dt = p, dp/dt = -dU/dq sur `steps` itérations.
    ///
    /// Schéma Velocity-Verlet (symplectique d'ordre 2) :
    /// 1. p_{n+1/2} = p_n + dt/2 · force(q_n)   (force = -dU/dq)
    /// 2. q_{n+1}   = q_n + dt · p_{n+1/2}
    /// 3. p_{n+1}   = p_{n+1/2} + dt/2 · force(q_{n+1})
    ///
    /// Retourne la trajectoire [(q_0, p_0), (q_1, p_1), ..., (q_{steps}, p_{steps})].
    pub fn integrate(&self, q0: f64, p0: f64, energy: &EnergyNetwork) -> Vec<(f64, f64)> {
        let mut trajectory = Vec::with_capacity(self.steps + 1);
        let mut q = q0;
        let mut p = p0;
        let dt = self.dt;
        let half_dt = dt / 2.0;

        trajectory.push((q, p));

        for _ in 0..self.steps {
            // p_{n+1/2} = p_n + dt/2 · force(q_n)  où force = -dU/dq
            let f_q = energy.force(q);
            let p_half = p + half_dt * f_q;

            // q_{n+1} = q_n + dt · p_{n+1/2}
            q += dt * p_half;

            // p_{n+1} = p_{n+1/2} + dt/2 · force(q_{n+1})
            let f_q_new = energy.force(q);
            p = p_half + half_dt * f_q_new;

            trajectory.push((q, p));
        }

        trajectory
    }

    /// Mesure la conservation d'énergie sur toute la trajectoire.
    ///
    /// Retourne l'écart-type relatif : σ(H) / μ(H)
    /// où H(n) = U(q_n) + p_n²/2.
    ///
    /// Un intégrateur symplectique conserve H à O(dt²) près ;
    /// la dérive tend vers zéro quand dt → 0.
    pub fn energy_drift(&self, trajectory: &[(f64, f64)], energy: &EnergyNetwork) -> f64 {
        let n = trajectory.len();
        if n < 2 {
            return 0.0;
        }

        let energies: Vec<f64> = trajectory
            .iter()
            .map(|&(q, p)| energy.total_energy(q, p))
            .collect();

        let mean = energies.iter().sum::<f64>() / n as f64;
        let variance = energies.iter().map(|e| (e - mean).powi(2)).sum::<f64>() / (n - 1) as f64;

        if mean.abs() < f64::EPSILON {
            variance.sqrt()
        } else {
            variance.sqrt() / mean.abs()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oscillateur harmonique : U(q) = 0.5·k·q², m=1, k=1.
    /// Vérifie que la dérive d'énergie < 1% sur 1000 pas avec dt = 0.01.
    #[test]
    fn test_integrator_conserves_energy() {
        // U(q) = 0.5*q^2 → alpha=0.5, beta=0.0, mu=0.0
        let energy = EnergyNetwork::new(0.5, 0.0, 0.0);
        let integrator = SymplecticIntegrator::new(0.01, 1000);

        let trajectory = integrator.integrate(1.0, 0.0, &energy);
        let drift = integrator.energy_drift(&trajectory, &energy);

        assert!(
            drift < 0.01,
            "Energy drift too large: {drift:.6} (should be < 0.01)"
        );
    }

    /// Période de l'oscillateur harmonique : T = 2π / ω = 2π pour k=1, m=1.
    /// Vérifie q(t+T) ≈ q(t) avec 1% de tolérance.
    #[test]
    fn test_integrator_period() {
        let energy = EnergyNetwork::new(0.5, 0.0, 0.0);
        let dt = 0.0001;
        let period = 2.0 * std::f64::consts::PI;
        let steps = (period / dt).round() as usize;

        let integrator = SymplecticIntegrator::new(dt, steps);
        let trajectory = integrator.integrate(1.0, 0.0, &energy);

        let (q_final, p_final) = trajectory.last().copied().unwrap();
        assert!(
            (q_final - 1.0).abs() < 0.1,
            "q({steps} steps) = {q_final}, expected ~1.0"
        );
        assert!(
            p_final.abs() < 0.1,
            "p({steps} steps) = {p_final}, expected ~0.0"
        );
    }

    /// Dérive d'énergie sur une longue trajectoire d'oscillateur harmonique.
    /// L'écart-type relatif doit être < 1%.
    #[test]
    fn test_energy_drift_small() {
        let energy = EnergyNetwork::new(0.5, 0.0, 0.0);
        let integrator = SymplecticIntegrator::new(0.01, 2000);

        let trajectory = integrator.integrate(1.0, 0.0, &energy);
        let drift = integrator.energy_drift(&trajectory, &energy);

        assert!(
            drift < 0.01,
            "Energy drift {:.6} >= 0.01 over {} steps",
            drift,
            2000
        );
    }
}
