//! Reward system for adaptive autonomous agent.
//!
//! Implements the reward calculation for:
//! - Action rewards (R_action)
//! - Social rewards (R_social)
//! - Information rewards (R_information)

use serde::{Deserialize, Serialize};

/// Complete reward calculation for the adaptive agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReward {
    /// Action reward
    pub action_reward: f64,

    /// Social reward
    pub social_reward: f64,

    /// Information reward
    pub information_reward: f64,

    /// Total reward
    pub total_reward: f64,

    /// Reward weights
    pub weights: RewardWeights,
}

/// Reward weights for combining different reward types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardWeights {
    pub weight_action: f64,
    pub weight_social: f64,
    pub weight_information: f64,
}

impl Default for RewardWeights {
    fn default() -> Self {
        Self {
            weight_action: 0.4,
            weight_social: 0.3,
            weight_information: 0.3,
        }
    }
}

impl AgentReward {
    /// Create a new reward instance.
    pub fn new(weights: RewardWeights) -> Self {
        Self {
            action_reward: 0.0,
            social_reward: 0.0,
            information_reward: 0.0,
            total_reward: 0.0,
            weights,
        }
    }

    /// Calculate total reward from individual components.
    pub fn calculate(&mut self) {
        self.total_reward =
            self.weights.weight_action * self.action_reward +
            self.weights.weight_social * self.social_reward +
            self.weights.weight_information * self.information_reward;
    }

    /// Update action reward.
    pub fn update_action_reward(&mut self, reward: f64) {
        self.action_reward = reward;
    }

    /// Update social reward.
    pub fn update_social_reward(&mut self, reward: f64) {
        self.social_reward = reward;
    }

    /// Update information reward.
    pub fn update_information_reward(&mut self, reward: f64) {
        self.information_reward = reward;
    }
}

/// Action reward calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReward {
    /// Reward for task completion
    pub completion_reward: f64,

    /// Penalty for failure
    pub failure_penalty: f64,

    /// Reward for quality
    pub quality_reward: f64,

    /// Reward for efficiency
    pub efficiency_reward: f64,
}

impl ActionReward {
    /// Calculate action reward based on task execution.
    pub fn calculate(
        completed_successfully: bool,
        quality_score: f64,  // 0.0 to 1.0
        execution_time: f64, // in seconds
        max_time: f64,       // maximum allowed time
    ) -> Self {
        let mut completion_reward = 0.0;
        let mut failure_penalty = 0.0;
        let mut efficiency_reward = 0.0;

        if completed_successfully {
            completion_reward = 1.0;
        } else {
            failure_penalty = -0.5; // Significant penalty for failure
        }

        // Quality reward (0 to 1)
        let quality_reward = quality_score;

        // Efficiency reward (based on time taken)
        if max_time > 0.0 {
            let efficiency = (max_time - execution_time).max(0.0) / max_time;
            efficiency_reward = efficiency.min(1.0);
        }

        Self {
            completion_reward,
            failure_penalty,
            quality_reward,
            efficiency_reward,
        }
    }

    /// Get total action reward.
    pub fn total(&self) -> f64 {
        self.completion_reward + self.failure_penalty + self.quality_reward + self.efficiency_reward
    }
}

/// Social reward calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialReward {
    /// Performance improvement compared to others
    pub performance_improvement: f64,

    /// Risk avoidance
    pub risk_avoidance: f64,

    /// Knowledge gain
    pub knowledge_gain: f64,

    /// Collaboration score (how well the agent worked with others)
    pub collaboration_score: f64,
}

impl SocialReward {
    /// Calculate social reward from social learning.
    pub fn calculate(
        performance_improvement: f64,  // Relative to other agents
        risk_avoidance: f64,           // 0.0 to 1.0
        knowledge_gain: f64,           // Amount of new knowledge gained
        collaboration_score: f64,      // 0.0 to 1.0
    ) -> Self {
        Self {
            performance_improvement,
            risk_avoidance,
            knowledge_gain,
            collaboration_score,
        }
    }

    /// Get total social reward.
    pub fn total(&self) -> f64 {
        self.performance_improvement +
        self.risk_avoidance +
        self.knowledge_gain +
        self.collaboration_score
    }
}

/// Information reward calculation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InformationReward {
    /// Change in uncertainty
    pub uncertainty_reduction: f64,

    /// Performance improvement
    pub performance_improvement: f64,

    /// Novelty of information
    pub novelty_score: f64,
}

impl InformationReward {
    /// Calculate information reward from learning.
    pub fn calculate(
        uncertainty_before: f64,      // Before learning
        uncertainty_after: f64,       // After learning
        performance_improvement: f64, // Performance improvement from learning
        novelty_score: f64,           // Novelty of information (0.0 to 1.0)
    ) -> Self {
        let uncertainty_reduction = (uncertainty_before - uncertainty_after).max(0.0);

        Self {
            uncertainty_reduction,
            performance_improvement,
            novelty_score,
        }
    }

    /// Get total information reward.
    pub fn total(&self) -> f64 {
        self.uncertainty_reduction +
        self.performance_improvement +
        self.novelty_score
    }
}

/// Reward calculation for social learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialLearningReward {
    /// Quality of the observation (0.0 to 1.0)
    pub quality: f64,

    /// Impact of the observation
    pub impact: f64,

    /// Learning value of the observation
    pub learning_value: f64,

    /// Social reward score
    pub social_reward_score: f64,
}

impl SocialLearningReward {
    /// Calculate social learning reward.
    pub fn calculate(
        quality: f64,          // 0.0 to 1.0
        impact: f64,           // 0.0 to 1.0
        learning_value: f64,   // 0.0 to 1.0
    ) -> Self {
        let social_reward_score = (quality * 0.4 + impact * 0.3 + learning_value * 0.3).min(1.0);

        Self {
            quality,
            impact,
            learning_value,
            social_reward_score,
        }
    }

    /// Get total reward score.
    pub fn total(&self) -> f64 {
        self.social_reward_score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_reward_calculation() {
        let weights = RewardWeights {
            weight_action: 0.4,
            weight_social: 0.3,
            weight_information: 0.3,
        };

        let mut reward = AgentReward::new(weights);
        reward.update_action_reward(0.8);
        reward.update_social_reward(0.6);
        reward.update_information_reward(0.7);
        reward.calculate();

        assert!(reward.total_reward > 0.0);
    }

    #[test]
    fn test_action_reward_calculation() {
        let action_reward = ActionReward::calculate(
            true,   // completed successfully
            0.9,    // quality score
            5.0,    // execution time
            10.0,   // max time
        );
        assert!(action_reward.total() >= 0.0);
    }

    #[test]
    fn test_social_reward_calculation() {
        let social_reward = SocialReward::calculate(
            0.8,    // performance improvement
            0.7,    // risk avoidance
            0.9,    // knowledge gain
            0.8,    // collaboration score
        );
        assert!(social_reward.total() >= 0.0);
    }

    #[test]
    fn test_information_reward_calculation() {
        let info_reward = InformationReward::calculate(
            0.8,    // uncertainty before
            0.3,    // uncertainty after
            0.7,    // performance improvement
            0.9,    // novelty score
        );
        assert!(info_reward.total() >= 0.0);
    }

    #[test]
    fn test_social_learning_reward_calculation() {
        let social_reward = SocialLearningReward::calculate(
            0.9,    // quality
            0.8,    // impact
            0.7,    // learning value
        );
        assert!(social_reward.total() >= 0.0);
    }
}