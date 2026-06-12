//! LearningHigh — Génération de récompense BCI suite à une réduction d'incertitude.
//!
//! Détecte les chutes significatives de l'erreur de prédiction PDT et génère un
//! potentiel BCI positif (récompense), avec habituation pour éviter l'auto-stimulation.

/// Générateur de récompense pour l'apprentissage détecté.
pub struct LearningHigh {
    /// Échelle maximale de la récompense.
    pub base_reward_scale: f64,
    /// Seuil de chute relative pour déclencher une récompense.
    pub drop_threshold: f64,
    /// Coefficient de réduction par habituation.
    pub habituation_k: f64,
    /// Compteur d'habituation (augmente à chaque récompense).
    pub habituation_counter: f64,
    /// Taux de décroissance temporelle de l'habituation (0.0–1.0).
    pub habituation_decay_rate: f64,
}

impl Default for LearningHigh {
    fn default() -> Self {
        Self {
            base_reward_scale: 0.1,
            drop_threshold: 0.15,
            habituation_k: 0.5,
            habituation_counter: 0.0,
            habituation_decay_rate: 0.99,
        }
    }
}

impl LearningHigh {
    pub fn new(base_reward_scale: f64, drop_threshold: f64, habituation_k: f64, habituation_decay_rate: f64) -> Self {
        Self { base_reward_scale, drop_threshold, habituation_k, habituation_counter: 0.0, habituation_decay_rate }
    }

    /// Décroissance temporelle de l'habituation (appelée chaque cycle).
    ///
    /// Multiplie le compteur par `habituation_decay_rate`, ce qui permet
    /// à l'agent de retrouver sa sensibilité après une période sans récompense.
    pub fn tick_decay(&mut self) {
        self.habituation_counter *= self.habituation_decay_rate;
    }

    /// Détecte l'apprentissage et génère une récompense.
    ///
    /// Compare `previous_error` et `current_error`. Si la chute relative
    /// dépasse `drop_threshold`, une récompense est calculée avec habituation.
    /// La récompense est bornée entre 0 et `base_reward_scale`.
    pub fn detect_and_reward(&mut self, previous_error: f64, current_error: f64) -> f64 {
        if previous_error <= 0.0 || current_error < 0.0 { return 0.0; }
        let error_drop = previous_error - current_error;
        let relative_drop = error_drop / previous_error;
        if relative_drop > self.drop_threshold {
            let effective_scale = self.base_reward_scale / (1.0 + (self.habituation_counter * self.habituation_k));
            let reward = effective_scale * relative_drop.tanh();
            self.habituation_counter += 1.0;
            reward.min(self.base_reward_scale).max(0.0)
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reward_generation() {
        let mut lh = LearningHigh::default();
        let reward = lh.detect_and_reward(1.0, 0.5);
        assert!(reward > 0.04 && reward < 0.05);
        assert_eq!(lh.habituation_counter, 1.0);
    }

    #[test]
    fn test_insufficient_drop() {
        let mut lh = LearningHigh::default();
        let reward = lh.detect_and_reward(1.0, 0.9);
        assert_eq!(reward, 0.0);
        assert_eq!(lh.habituation_counter, 0.0);
    }

    #[test]
    fn test_zero_previous_error() {
        let mut lh = LearningHigh::default();
        assert_eq!(lh.detect_and_reward(0.0, 0.5), 0.0);
        assert_eq!(lh.habituation_counter, 0.0);
    }

    #[test]
    fn test_negative_current_error() {
        let mut lh = LearningHigh::default();
        assert_eq!(lh.detect_and_reward(1.0, -1.0), 0.0);
    }

    #[test]
    fn test_habituation_decay() {
        let mut lh = LearningHigh::default();
        lh.habituation_counter = 5.0;
        let reward = lh.detect_and_reward(1.0, 0.5);
        assert!(reward < 0.015);
        assert_eq!(lh.habituation_counter, 6.0);
        lh.tick_decay();
        assert!((lh.habituation_counter - 5.94).abs() < 1e-6);
    }

    #[test]
    fn test_reward_upper_bound() {
        let mut lh = LearningHigh::default();
        lh.habituation_counter = 0.0;
        let reward = lh.detect_and_reward(1.0, 0.0001);
        // Very large relative drop → reward should be bounded by base_reward_scale
        assert!(reward <= lh.base_reward_scale);
    }
}
