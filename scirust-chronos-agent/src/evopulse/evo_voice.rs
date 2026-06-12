//! EvoVoice — Verbalisation proactive des états EvoPulse.
//!
//! Expose l'état d'EvoPulse sous forme de messages structurés qui peuvent
//! être affichés dans l'interface de chat ou logués. Fournit des suggestions
//! d'exploration et des notifications d'apprentissage.

use serde::{Deserialize, Serialize};

/// Niveau de sévérité d'un message vocal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoiceLevel {
    Info,
    Suggestion,
    Achievement,
    Warning,
}

/// Message vocal produit par EvoVoice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceMessage {
    pub topic: String,
    pub level: VoiceLevel,
    pub text: String,
    pub timestamp: String,
    pub context: Option<String>,
}

/// Filtre de persistance pour la verbalisation.
pub struct EvoVoice {
    /// Seuil d'inconfort pour suggérer une exploration.
    pub discomfort_threshold: f64,
    /// Durée de persistance (en minutes) avant de suggérer une exploration.
    pub persistence_minutes: u64,
    /// Messages émis.
    pub messages: Vec<VoiceMessage>,
    /// Timestamp du dernier inconfort élevé.
    last_high_discomfort: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for EvoVoice {
    fn default() -> Self {
        Self {
            discomfort_threshold: 0.05,
            persistence_minutes: 5,
            messages: Vec::new(),
            last_high_discomfort: None,
        }
    }
}

impl EvoVoice {
    pub fn new(discomfort_threshold: f64, persistence_minutes: u64) -> Self {
        Self {
            discomfort_threshold,
            persistence_minutes,
            messages: Vec::new(),
            last_high_discomfort: None,
        }
    }

    /// Émet une notification de curiosité persistante.
    pub fn on_curiosity(&mut self, bci_potential: f64, uncertainty: f64) -> Option<VoiceMessage> {
        let discomfort = bci_potential.abs();
        let now = chrono::Utc::now();

        if discomfort >= self.discomfort_threshold {
            match self.last_high_discomfort {
                Some(last) => {
                    let elapsed = (now - last).num_minutes() as u64;
                    if elapsed >= self.persistence_minutes {
                        self.last_high_discomfort = Some(now);
                        return Some(VoiceMessage {
                            topic: "evopulse.curiosity".into(),
                            level: VoiceLevel::Suggestion,
                            text: format!(
                                "🧠 Curiosité persistante : incertitude={:.3}, inconfort={:.3}. Envisager une exploration.",
                                uncertainty, discomfort
                            ),
                            timestamp: now.to_rfc3339(),
                            context: Some(format!("uncertainty={:.3}, potential={:.3}", uncertainty, bci_potential)),
                        });
                    }
                }
                None => {
                    self.last_high_discomfort = Some(now);
                }
            }
        } else {
            // Inconfort résolu → reset
            self.last_high_discomfort = None;
        }
        None
    }

    /// Émet une notification de récompense d'apprentissage.
    pub fn on_learning(&mut self, reward: f64, error_drop: f64) -> Option<VoiceMessage> {
        if reward > 0.0 {
            let msg = VoiceMessage {
                topic: "evopulse.learning_high".into(),
                level: VoiceLevel::Achievement,
                text: format!(
                    "📈 Apprentissage détecté : chute d'erreur={:.1}%, récompense={:.4}.",
                    error_drop * 100.0, reward
                ),
                timestamp: chrono::Utc::now().to_rfc3339(),
                context: Some(format!("reward={:.4}, error_drop={:.2}%", reward, error_drop * 100.0)),
            };
            Some(msg)
        } else {
            None
        }
    }

    /// Génère un résumé des lacunes après un cycle SelfQuiz.
    pub fn on_quiz_complete(&mut self, gaps_found: usize, avg_confidence: f64) -> VoiceMessage {
        let level = if gaps_found > 5 { VoiceLevel::Warning }
                    else if gaps_found > 0 { VoiceLevel::Suggestion }
                    else { VoiceLevel::Info };
        VoiceMessage {
            topic: "evopulse.selfquiz".into(),
            level,
            text: format!(
                "📋 Auto-évaluation terminée : {} lacune(s) identifiée(s), confiance moyenne={:.1}%.",
                gaps_found, avg_confidence * 100.0
            ),
            timestamp: chrono::Utc::now().to_rfc3339(),
            context: None,
        }
    }

    /// Ajoute un message à l'historique.
    pub fn push(&mut self, msg: VoiceMessage) {
        self.messages.push(msg);
        if self.messages.len() > 100 {
            self.messages.remove(0);
        }
    }

    /// Exporte les messages récents.
    pub fn recent_messages(&self, n: usize) -> &[VoiceMessage] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_on_curiosity_persistence() {
        let mut voice = EvoVoice::new(0.05, 0); // persistance 0
        voice.on_curiosity(-0.1, 0.5); // first call primes the timer
        let msg = voice.on_curiosity(-0.1, 0.5); // second call triggers persistence
        assert!(msg.is_some());
        if let Some(m) = msg {
            assert_eq!(m.topic, "evopulse.curiosity");
        }
    }

    #[test]
    fn test_on_learning_reward() {
        let mut voice = EvoVoice::default();
        let msg = voice.on_learning(0.05, 0.2);
        assert!(msg.is_some());
    }

    #[test]
    fn test_no_message_for_low_discomfort() {
        let mut voice = EvoVoice::default();
        let msg = voice.on_curiosity(-0.01, 0.01);
        assert!(msg.is_none());
    }

    #[test]
    fn test_on_quiz_complete_warning() {
        let mut voice = EvoVoice::default();
        let msg = voice.on_quiz_complete(10, 0.6);
        assert!(msg.text.contains("10 lacune(s)"));
    }

    #[test]
    fn test_message_capacity() {
        let mut voice = EvoVoice::default();
        for _ in 0..150 {
            voice.push(VoiceMessage {
                topic: "test".into(), level: VoiceLevel::Info,
                text: "".into(), timestamp: "".into(), context: None,
            });
        }
        assert_eq!(voice.messages.len(), 100);
    }
}
