//! Error metrics for adaptive autonomous agent.
//!
//! Implements the global error calculation combining multiple error types:
//! - Prediction error (E_p)
//! - Action error (δ_t)
//! - Goal error (E_g)
//! - Social error (E_social)
//! - Uncertainty (U_t)
//! - Initiative error (I_t)

use serde::{Deserialize, Serialize};

/// Global error calculation for the adaptive agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalError {
    /// Prediction error: ||o_(t+1) - ô_(t+1)||
    pub prediction_error: f64,

    /// Action error (reinforcement):
    /// δ_t = r_t + γQ(s_(t+1),a_(t+1)) - Q(s_t,a_t)
    pub action_error: f64,

    /// Goal error: ||G_t - R_t||
    pub goal_error: f64,

    /// Social error: ||Résultat_j - Prédiction_j||
    pub social_error: f64,

    /// Uncertainty: H(B_t)
    pub uncertainty: f64,

    /// Initiative error
    pub initiative_error: f64,

    /// Weighted global error
    pub global_error: f64,

    /// Error weights
    pub weights: ErrorWeights,
}

/// Error weights for combining different error types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorWeights {
    pub weight_prediction: f64,
    pub weight_action: f64,
    pub weight_goal: f64,
    pub weight_social: f64,
    pub weight_uncertainty: f64,
    pub weight_initiative: f64,
}

impl Default for ErrorWeights {
    fn default() -> Self {
        Self {
            weight_prediction: 0.2,
            weight_action: 0.2,
            weight_goal: 0.2,
            weight_social: 0.15,
            weight_uncertainty: 0.15,
            weight_initiative: 0.1,
        }
    }
}

impl GlobalError {
    /// Create a new global error instance.
    pub fn new(weights: ErrorWeights) -> Self {
        Self {
            prediction_error: 0.0,
            action_error: 0.0,
            goal_error: 0.0,
            social_error: 0.0,
            uncertainty: 0.0,
            initiative_error: 0.0,
            global_error: 0.0,
            weights,
        }
    }

    /// Calculate global error from individual components.
    pub fn calculate(&mut self) {
        self.global_error = self.weights.weight_prediction * self.prediction_error
            + self.weights.weight_action * self.action_error
            + self.weights.weight_goal * self.goal_error
            + self.weights.weight_social * self.social_error
            + self.weights.weight_uncertainty * self.uncertainty
            + self.weights.weight_initiative * self.initiative_error;
    }

    /// Update prediction error.
    pub fn update_prediction_error(&mut self, error: f64) {
        self.prediction_error = error.abs();
    }

    /// Update action error.
    pub fn update_action_error(&mut self, error: f64) {
        self.action_error = error.abs();
    }

    /// Update goal error.
    pub fn update_goal_error(&mut self, error: f64) {
        self.goal_error = error.abs();
    }

    /// Update social error.
    pub fn update_social_error(&mut self, error: f64) {
        self.social_error = error.abs();
    }

    /// Update uncertainty.
    pub fn update_uncertainty(&mut self, uncertainty: f64) {
        self.uncertainty = uncertainty;
    }

    /// Update initiative error.
    pub fn update_initiative_error(&mut self, error: f64) {
        self.initiative_error = error.abs();
    }
}

/// Prediction error calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionError {
    pub actual_observation: String,
    pub predicted_observation: String,
    pub similarity_score: f64,
}

impl PredictionError {
    /// Calculate prediction error between actual and predicted observations.
    pub fn calculate(actual: &str, predicted: &str) -> Self {
        // Simple cosine similarity for demonstration
        let similarity = Self::cosine_similarity(actual, predicted);
        // let error = 1.0 - similarity;  // unused variable

        Self {
            actual_observation: actual.to_string(),
            predicted_observation: predicted.to_string(),
            similarity_score: similarity,
        }
    }

    /// Simple cosine similarity between two strings.
    fn cosine_similarity(a: &str, b: &str) -> f64 {
        // This is a simplified implementation - in practice this would use embeddings
        let words_a: Vec<&str> = a.split_whitespace().collect();
        let words_b: Vec<&str> = b.split_whitespace().collect();

        if words_a.is_empty() || words_b.is_empty() {
            return 0.0;
        }

        let set_a: std::collections::HashSet<&str> = words_a.iter().copied().collect();
        let set_b: std::collections::HashSet<&str> = words_b.iter().copied().collect();

        let intersection = set_a.intersection(&set_b).count() as f64;
        let union = set_a.len().max(set_b.len()) as f64;

        if union == 0.0 {
            0.0
        } else {
            intersection / union
        }
    }
}

/// Goal error calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalError {
    pub target_goal: String,
    pub achieved_result: String,
    pub goal_similarity: f64,
}

impl GoalError {
    /// Calculate goal error between target and actual results.
    pub fn calculate(target: &str, result: &str) -> Self {
        let similarity = PredictionError::cosine_similarity(target, result);

        Self {
            target_goal: target.to_string(),
            achieved_result: result.to_string(),
            goal_similarity: similarity,
        }
    }
}

/// Social learning error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialError {
    pub observed_trajectory: String,
    pub prediction: String,
    pub error: f64,
}

impl SocialError {
    /// Calculate social error from observed trajectory and prediction.
    pub fn calculate(observed: &str, predicted: &str) -> Self {
        let similarity = PredictionError::cosine_similarity(observed, predicted);
        let error = 1.0 - similarity;

        Self {
            observed_trajectory: observed.to_string(),
            prediction: predicted.to_string(),
            error,
        }
    }
}

/// Uncertainty calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyMetric {
    pub entropy: f64,
    pub confidence: f64,
    pub information_gain: f64,
}

impl UncertaintyMetric {
    /// Calculate uncertainty from model predictions and knowledge.
    pub fn calculate(model_state: &str, memory_state: &str) -> Self {
        // This is a simplified implementation
        let entropy = Self::calculate_entropy(model_state, memory_state);
        let confidence = 1.0 - entropy;
        let information_gain = Self::calculate_information_gain(model_state, memory_state);

        Self {
            entropy,
            confidence,
            information_gain,
        }
    }

    fn calculate_entropy(model_state: &str, memory_state: &str) -> f64 {
        // Simplified entropy calculation based on text content
        let model_words: Vec<&str> = model_state.split_whitespace().collect();
        let memory_words: Vec<&str> = memory_state.split_whitespace().collect();

        if model_words.is_empty() || memory_words.is_empty() {
            return 0.0;
        }

        // Simple measure of information overlap
        let set_model: std::collections::HashSet<&str> = model_words.iter().copied().collect();
        let set_memory: std::collections::HashSet<&str> = memory_words.iter().copied().collect();

        let intersection = set_model.intersection(&set_memory).count() as f64;
        let union = set_model.len().max(set_memory.len()) as f64;

        if union == 0.0 {
            0.0
        } else {
            // Simple entropy measure (lower is more certain)
            intersection / union
        }
    }

    fn calculate_information_gain(model_state: &str, memory_state: &str) -> f64 {
        // Simplified information gain calculation
        let model_words: Vec<&str> = model_state.split_whitespace().collect();
        let memory_words: Vec<&str> = memory_state.split_whitespace().collect();

        if model_words.is_empty() || memory_words.is_empty() {
            return 0.0;
        }

        let set_model: std::collections::HashSet<&str> = model_words.iter().copied().collect();
        let set_memory: std::collections::HashSet<&str> = memory_words.iter().copied().collect();

        let intersection = set_model.intersection(&set_memory).count() as f64;
        let union = set_model.len().max(set_memory.len()) as f64;

        if union == 0.0 {
            0.0
        } else {
            // Information gain - how much the model state improves upon memory
            intersection / union
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_error_calculation() {
        let weights = ErrorWeights {
            weight_prediction: 0.2,
            weight_action: 0.2,
            weight_goal: 0.2,
            weight_social: 0.15,
            weight_uncertainty: 0.15,
            weight_initiative: 0.1,
        };

        let mut error = GlobalError::new(weights);
        error.update_prediction_error(0.5);
        error.update_action_error(0.3);
        error.update_goal_error(0.4);
        error.update_social_error(0.2);
        error.update_uncertainty(0.6);
        error.update_initiative_error(0.1);
        error.calculate();

        assert!(error.global_error > 0.0);
    }

    #[test]
    fn test_prediction_error_calculation() {
        let error = PredictionError::calculate("Hello world", "Hello universe");
        assert!(error.similarity_score >= 0.0 && error.similarity_score <= 1.0);
    }

    #[test]
    fn test_goal_error_calculation() {
        let error = GoalError::calculate("Complete task", "Task completed successfully");
        assert!(error.goal_similarity >= 0.0 && error.goal_similarity <= 1.0);
    }

    #[test]
    fn test_uncertainty_calculation() {
        let uncertainty = UncertaintyMetric::calculate("model state", "memory state");
        assert!(uncertainty.entropy >= 0.0 && uncertainty.entropy <= 1.0);
        assert!(uncertainty.confidence >= 0.0 && uncertainty.confidence <= 1.0);
    }
}
