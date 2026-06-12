pub mod integrator;
pub mod learn;
pub mod hamiltonian;

pub use hamiltonian::HamiltonianNN;
pub use integrator::SymplecticIntegrator;
pub use learn::HNNTrainer;

use scirust_autodiff::Dual;

/// Réseau d'énergie apprenant U(q) = alpha·(q-mu)^2 + beta·(q-mu)^4
///
/// Paramètres libres (alpha, beta, mu) appris à partir de données.
/// Utilise les Dual numbers de scirust-autodiff pour le calcul exact
/// des forces via dérivation automatique forward-mode.
#[derive(Debug, Clone)]
pub struct EnergyNetwork {
    pub alpha: f64,
    pub beta: f64,
    pub mu: f64,
}

impl EnergyNetwork {
    pub fn new(alpha: f64, beta: f64, mu: f64) -> Self {
        Self { alpha, beta, mu }
    }

    /// Énergie potentielle U(q) calculée avec des Dual numbers.
    /// U(q) = alpha·(q-mu)^2 + beta·(q-mu)^4
    ///
    /// Le Dual propage la dérivée, ce qui permet d'obtenir la force
    /// sans approximation numérique.
    pub fn potential(&self, q: Dual) -> Dual {
        let delta = q - Dual::primal(self.mu);
        let delta_sq = delta * delta;
        Dual::primal(self.alpha) * delta_sq + Dual::primal(self.beta) * (delta_sq * delta_sq)
    }

    /// Force -dU/dq calculée via differentiation automatique Dual.
    ///
    /// # Formule analytique (vérification)
    /// U(q) = alpha·(q-mu)^2 + beta·(q-mu)^4
    /// dU/dq = 2·alpha·(q-mu) + 4·beta·(q-mu)^3
    /// F = -dU/dq
    pub fn force(&self, q: f64) -> f64 {
        let q_dual = Dual::var(q);
        let u = self.potential(q_dual);
        -u.grad()
    }

    /// Énergie totale H(q,p) = U(q) + p²/2
    ///
    /// Hamiltonien du système : somme de l'énergie potentielle
    /// et de l'énergie cinétique.
    pub fn total_energy(&self, q: f64, p: f64) -> f64 {
        let q_dual = Dual::primal(q);
        let u = self.potential(q_dual);
        u.val() + 0.5 * p * p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Énergie potentielle positive pour tout q.
    /// Vérifie que U(q) >= 0 pour alpha, beta >= 0.
    #[test]
    fn test_potential_positive() {
        let nn = EnergyNetwork::new(1.0, 0.5, 0.0);
        // q de -5 à 5
        for i in -50i32..=50 {
            let q = i as f64 * 0.1;
            let u = nn.potential(Dual::primal(q));
            assert!(u.val() >= 0.0, "U({}) = {} < 0", q, u.val());
        }
    }

    /// Vérifie que force = -dU/dq en comparant avec la formule analytique.
    #[test]
    fn test_force_from_potential() {
        let nn = EnergyNetwork::new(2.0, 0.3, 1.5);
        for i in -20i32..=20 {
            let q = i as f64 * 0.2 + 1.5;

            // Force via Dual (exact)
            let f_dual = nn.force(q);

            // Force via formule analytique : -dU/dq = -(2*alpha*delta + 4*beta*delta^3)
            let delta = q - nn.mu;
            let f_analytic = -(2.0 * nn.alpha * delta + 4.0 * nn.beta * delta.powi(3));

            let abs_err = (f_dual - f_analytic).abs();
            assert!(
                abs_err < 1e-14,
                "q={}: force Dual={} analytic={} err={}",
                q,
                f_dual,
                f_analytic,
                abs_err
            );
        }
    }

    /// Vérification de l'énergie totale positive.
    #[test]
    fn test_total_energy_positive() {
        let nn = EnergyNetwork::new(1.0, 0.0, 0.0);
        let h = nn.total_energy(0.5, 1.0);
        // U(0.5) = 1.0 * 0.25 = 0.25, K = 0.5, total = 0.75
        assert!((h - 0.75).abs() < 1e-12);
    }
}
