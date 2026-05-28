//! Bus Publisher — Publication d'événements EvoPulse.
//!
//! Publie les événements de curiosité et d'apprentissage. Actuellement,
//! les événements sont logués en stderr. La publication Redis sera
//! activée quand la dépendance `redis` sera ajoutée au Cargo.toml.
//!
//! Topics prévus :
//! - `evopulse.curiosity`   → événements d'inconfort
//! - `evopulse.learning_high` → événements de récompense

use serde::Serialize;

/// Événement de curiosité (inconfort).
#[derive(Debug, Clone, Serialize)]
pub struct CuriosityEvent {
    pub uncertainty: f64,
    pub ema_variance: f64,
    pub bci_potential: f64,
    pub reducible: bool,
}

/// Événement d'apprentissage (récompense).
#[derive(Debug, Clone, Serialize)]
pub struct LearningEvent {
    pub previous_error: f64,
    pub current_error: f64,
    pub reward: f64,
    pub habituation_counter: f64,
}

/// Publie un événement de curiosité (mode log uniquement).
pub fn publish_curiosity(event: CuriosityEvent) {
    let json = serde_json::to_string(&event).unwrap_or_default();
    eprintln!("[evopulse.curiosity] {}", json);
}

/// Publie un événement d'apprentissage (mode log uniquement).
pub fn publish_learning(event: LearningEvent) {
    let json = serde_json::to_string(&event).unwrap_or_default();
    eprintln!("[evopulse.learning_high] {}", json);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curiosity_event_serialization() {
        let ce = CuriosityEvent {
            uncertainty: 0.5,
            ema_variance: 0.3,
            bci_potential: -0.05,
            reducible: true,
        };
        let json = serde_json::to_string(&ce).unwrap();
        assert!(json.contains("0.5"));
    }

    #[test]
    fn test_learning_event_serialization() {
        let le = LearningEvent {
            previous_error: 1.0,
            current_error: 0.5,
            reward: 0.05,
            habituation_counter: 1.0,
        };
        let json = serde_json::to_string(&le).unwrap();
        assert!(json.contains("reward"));
    }
}
