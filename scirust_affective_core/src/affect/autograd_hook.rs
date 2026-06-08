use scirust::autodiff::reverse::Tape;
use crate::affect::space::AffectiveState;
use crate::affect::drives::DriveRegistry;

pub struct EmotionalAutogradHook {
    pub sensitivity: f32,
}

impl EmotionalAutogradHook {
    pub fn new(sensitivity: f32) -> Self {
        Self { sensitivity }
    }

    /// Calcule le gradient de tension émotionnelle pour chaque drive.
    ///
    /// Pour chaque drive, le gradient est proportionnel à l'écart au setpoint
    /// cible, pondéré par le poids du drive. Plus l'écart est grand, plus le
    /// gradient est fort — cela permet au système de recentrer les drives
    /// déséquilibrés via un feedback différentiable.
    pub fn backpropagate_emotional_tension(
        &self,
        _tape: &mut Tape,
        registry: &DriveRegistry,
        state: &AffectiveState,
    ) -> Vec<f32> {
        let coords = state.get_coordinates();
        let arousal = coords[1]; // Arousal module l'intensité de la tension

        registry
            .drives
            .iter()
            .map(|drive| {
                // Gradient = poids * (valeur_courante - setpoint) * modulation_arousal
                let deviation = drive.current_value - drive.target_setpoint;
                let raw_gradient = drive.weight * deviation;

                // Module par l'arousal : plus l'arousal est élevé, plus la
                // tension émotionnelle est forte (résonance émotionnelle)
                let arousal_factor = 1.0 + arousal.abs();
                raw_gradient * arousal_factor
            })
            .collect()
    }

    pub fn compute_weight_gate(&self, gradients: &[f32]) -> f32 {
        let magnitude = (gradients.iter().map(|g| g * g).sum::<f32>()).sqrt();
        1.0 / (1.0 + (magnitude * self.sensitivity).exp())
    }
}
