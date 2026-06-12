//! CuriosityEngine — Génération d'inconfort mathématique basé sur l'incertitude PDT.
//!
//! Convertit la variance des valeurs prédites par le PDT en potentiel BCI négatif
//! (inconfort), après filtrage du bruit stochastique via une moyenne mobile exponentielle.

/// Entrée : une seule prédiction de trajectoire du PDT.
pub struct Trajectory {
    pub predicted_state_value: f64,
}

/// Moteur de curiosité : calcule l'incertitude, filtre le bruit, génère le potentiel BCI.
pub struct CuriosityEngine {
    /// Coefficient de lissage de l'EMA (0.0–1.0).
    pub ema_alpha: f64,
    /// EMA courante de la variance.
    pub ema_variance: f64,
    /// Marge de réduction minimale pour considérer la variance comme réductible.
    pub reduction_margin: f64,
    /// Échelle de conversion en potentiel BCI.
    pub bci_scale: f64,
}

impl Default for CuriosityEngine {
    fn default() -> Self {
        Self {
            ema_alpha: 0.1,
            ema_variance: 0.0,
            reduction_margin: 0.05,
            bci_scale: 0.1,
        }
    }
}

impl CuriosityEngine {
    pub fn new(ema_alpha: f64, reduction_margin: f64, bci_scale: f64) -> Self {
        Self { ema_alpha, ema_variance: 0.0, reduction_margin, bci_scale }
    }

    /// Calcule la variance des valeurs prédites d'un ensemble de trajectoires.
    pub fn compute_uncertainty(&self, trajectories: &[Trajectory]) -> f64 {
        if trajectories.is_empty() { return 0.0; }
        let len = trajectories.len() as f64;
        let sum: f64 = trajectories.iter().map(|t| t.predicted_state_value).sum();
        let mean = sum / len;
        let variance = trajectories.iter()
            .map(|t| { let diff = t.predicted_state_value - mean; diff * diff })
            .sum::<f64>() / len;
        variance
    }

    /// Détermine si la variance courante est réductible (bruit non stochastique).
    ///
    /// Met à jour l'EMA interne. Retourne `true` si la variance courante est
    /// significativement inférieure à l'EMA précédente, indiquant une tendance
    /// baissière (donc un apprentissage possible).
    pub fn is_reducible(&mut self, current_variance: f64) -> bool {
        if self.ema_variance == 0.0 {
            self.ema_variance = current_variance;
            return false;
        }
        let old_ema = self.ema_variance;
        self.ema_variance = (self.ema_alpha * current_variance) + ((1.0 - self.ema_alpha) * old_ema);
        current_variance < (old_ema - self.reduction_margin)
    }

    /// Convertit une variance réductible en potentiel BCI négatif (inconfort).
    ///
    /// Formule : `-bci_scale * tanh(variance)`, borné dans `[-bci_scale, 0.0]`.
    /// Retourne 0.0 pour variance ≤ 0.0.
    pub fn to_bci_potential(&self, reducible_variance: f64) -> f64 {
        if reducible_variance <= 0.0 { return 0.0; }
        -self.bci_scale * reducible_variance.tanh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_uncertainty() {
        let engine = CuriosityEngine::default();
        let trajs = vec![
            Trajectory { predicted_state_value: 1.0 },
            Trajectory { predicted_state_value: 2.0 },
            Trajectory { predicted_state_value: 3.0 },
        ];
        let var = engine.compute_uncertainty(&trajs);
        assert!((var - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_empty_trajectories() {
        let engine = CuriosityEngine::default();
        assert_eq!(engine.compute_uncertainty(&[]), 0.0);
    }

    #[test]
    fn test_is_reducible_and_ema() {
        let mut engine = CuriosityEngine::new(0.5, 0.1, 0.1);
        assert_eq!(engine.is_reducible(1.0), false);
        assert_eq!(engine.ema_variance, 1.0);
        assert_eq!(engine.is_reducible(0.5), true);
        assert_eq!(engine.ema_variance, 0.75);
        assert_eq!(engine.is_reducible(0.8), false);
    }

    #[test]
    fn test_to_bci_potential_bounds() {
        let engine = CuriosityEngine::new(0.1, 0.05, 0.2);
        let pot = engine.to_bci_potential(1000.0);
        assert!(pot < 0.0);
        assert!(pot >= -0.2001);
    }

    #[test]
    fn test_to_bci_potential_zero() {
        let engine = CuriosityEngine::default();
        assert_eq!(engine.to_bci_potential(0.0), 0.0);
        assert_eq!(engine.to_bci_potential(-1.0), 0.0);
    }
}
