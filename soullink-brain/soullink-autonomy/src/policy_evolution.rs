//! Policy evolution for adaptive autonomous agent.
//!
//! Implements the policy adaptation mechanism:
//! - π_(t+1) = π_t + α_π ∇(Q-G-E)
//! - Updates action selection policies based on learning

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Policy evolution engine for adaptive agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvolution {
    /// Current policy parameters
    pub policy_parameters: HashMap<String, f64>,

    /// Learning rate for policy updates
    pub learning_rate: f64,

    /// Policy weights
    pub weights: PolicyWeights,

    /// Policy history for tracking evolution
    pub policy_history: Vec<PolicyState>,
}

/// Policy weights for different components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyWeights {
    pub weight_q_value: f64,
    pub weight_goal_alignment: f64,
    pub weight_error: f64,
    pub weight_uncertainty: f64,
    pub weight_social: f64,
}

impl Default for PolicyWeights {
    fn default() -> Self {
        Self {
            weight_q_value: 0.3,
            weight_goal_alignment: 0.25,
            weight_error: 0.2,
            weight_uncertainty: 0.15,
            weight_social: 0.1,
        }
    }
}

/// Policy state at a given time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyState {
    pub timestamp: i64,
    pub parameters: HashMap<String, f64>,
    pub q_value: f64,
    pub goal_alignment: f64,
    pub error: f64,
    pub uncertainty: f64,
    pub social_factor: f64,
}

impl PolicyEvolution {
    /// Create a new policy evolution engine.
    pub fn new(learning_rate: f64, weights: PolicyWeights) -> Self {
        let mut policy_parameters = HashMap::new();
        policy_parameters.insert("exploration_rate".to_string(), 0.1);
        policy_parameters.insert("exploitation_rate".to_string(), 0.9);
        policy_parameters.insert("curiosity_factor".to_string(), 0.5);
        policy_parameters.insert("risk_tolerance".to_string(), 0.3);

        Self {
            policy_parameters,
            learning_rate,
            weights,
            policy_history: Vec::new(),
        }
    }

    /// Update policy based on error and performance metrics.
    pub fn update_policy(
        &mut self,
        q_value: f64,        // Q-value of current action
        goal_alignment: f64, // Alignment with goals (0.0 to 1.0)
        error: f64,          // Global error
        uncertainty: f64,    // Current uncertainty
        social_factor: f64,  // Social learning factor
    ) {
        let gradient =
            self.calculate_gradient(q_value, goal_alignment, error, uncertainty, social_factor);

        // Update policy parameters using gradient descent
        for (param_name, grad) in gradient {
            if let Some(current_val) = self.policy_parameters.get_mut(&param_name) {
                *current_val += self.learning_rate * grad;

                // Ensure parameters stay within reasonable bounds
                match param_name.as_str() {
                    "exploration_rate" | "exploitation_rate" => {
                        *current_val = current_val.clamp(0.0, 1.0);
                    }
                    "curiosity_factor" => {
                        *current_val = current_val.clamp(0.0, 1.0);
                    }
                    "risk_tolerance" => {
                        *current_val = current_val.clamp(0.0, 1.0);
                    }
                    _ => {}
                }
            }
        }

        // Record policy state
        self.record_policy_state(q_value, goal_alignment, error, uncertainty, social_factor);
    }

    /// Calculate gradient for policy update.
    fn calculate_gradient(
        &self,
        q_value: f64,
        goal_alignment: f64,
        error: f64,
        uncertainty: f64,
        social_factor: f64,
    ) -> HashMap<String, f64> {
        let mut gradient = HashMap::new();

        // Q-value gradient - encourage high Q-values
        gradient.insert(
            "exploration_rate".to_string(),
            self.weights.weight_q_value * q_value,
        );
        gradient.insert(
            "exploitation_rate".to_string(),
            self.weights.weight_q_value * q_value,
        );

        // Goal alignment gradient - encourage goal alignment
        gradient.insert(
            "curiosity_factor".to_string(),
            self.weights.weight_goal_alignment * goal_alignment,
        );

        // Error gradient - reduce error (negative)
        gradient.insert(
            "risk_tolerance".to_string(),
            -self.weights.weight_error * error,
        );

        // Uncertainty gradient - higher uncertainty = more exploration
        gradient.insert(
            "exploration_rate".to_string(),
            self.weights.weight_uncertainty * uncertainty,
        );

        // Social gradient - encourage social learning
        gradient.insert(
            "exploitation_rate".to_string(),
            self.weights.weight_social * social_factor,
        );

        gradient
    }

    /// Record current policy state.
    fn record_policy_state(
        &mut self,
        q_value: f64,
        goal_alignment: f64,
        error: f64,
        uncertainty: f64,
        social_factor: f64,
    ) {
        let state = PolicyState {
            timestamp: chrono::Utc::now().timestamp_millis(),
            parameters: self.policy_parameters.clone(),
            q_value,
            goal_alignment,
            error,
            uncertainty,
            social_factor,
        };

        self.policy_history.push(state);

        // Keep only recent history (last 100 entries)
        if self.policy_history.len() > 100 {
            self.policy_history.remove(0);
        }
    }

    /// Get current policy parameters.
    pub fn get_policy(&self) -> &HashMap<String, f64> {
        &self.policy_parameters
    }

    /// Get a specific policy parameter.
    pub fn get_parameter(&self, name: &str) -> Option<f64> {
        self.policy_parameters.get(name).copied()
    }

    /// Set a policy parameter.
    pub fn set_parameter(&mut self, name: String, value: f64) {
        self.policy_parameters.insert(name, value);
    }

    /// Get policy evolution history.
    pub fn get_history(&self) -> &[PolicyState] {
        &self.policy_history
    }

    /// Reset policy to initial state.
    pub fn reset(&mut self) {
        *self = Self::new(self.learning_rate, self.weights.clone());
    }
}

/// Action selection based on evolved policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSelector {
    /// Policy evolution engine
    pub policy: PolicyEvolution,

    /// Q-value table for action selection
    pub q_table: HashMap<String, f64>,

    /// Exploration strategy
    pub exploration_strategy: ExplorationStrategy,
}

/// Exploration strategies.
#[derive(Debug, Clone, Serialize, Deserialize, Copy, PartialEq)]
pub enum ExplorationStrategy {
    EpsilonGreedy,
    UCB1,
    ThompsonSampling,
}

impl Default for ActionSelector {
    fn default() -> Self {
        let policy_weights = PolicyWeights::default();
        let mut policy = PolicyEvolution::new(0.01, policy_weights);

        // Initialize with some reasonable defaults
        policy.set_parameter("exploration_rate".to_string(), 0.1);
        policy.set_parameter("exploitation_rate".to_string(), 0.9);
        policy.set_parameter("curiosity_factor".to_string(), 0.5);
        policy.set_parameter("risk_tolerance".to_string(), 0.3);

        Self {
            policy,
            q_table: HashMap::new(),
            exploration_strategy: ExplorationStrategy::EpsilonGreedy,
        }
    }
}

impl ActionSelector {
    /// Create a new action selector.
    pub fn new() -> Self {
        Default::default()
    }

    /// Select next action based on current policy and Q-values.
    pub fn select_action(
        &mut self,
        available_actions: &[&str],
        global_error: f64,
        uncertainty: f64,
        social_factor: f64,
    ) -> String {
        // Update policy based on recent performance
        let q_values: Vec<f64> = available_actions
            .iter()
            .map(|&action| *self.q_table.get(action).unwrap_or(&0.0))
            .collect();

        let max_q = q_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min_q = q_values.iter().cloned().fold(f64::INFINITY, f64::min);

        // Normalize Q-values for policy update
        let normalized_q = if max_q > min_q {
            (max_q - min_q).max(0.001) // Avoid division by zero
        } else {
            1.0
        };

        // Update policy based on current state and global error
        self.policy.update_policy(
            max_q / normalized_q,                // Normalized Q-value
            1.0 - (global_error / 2.0).min(1.0), // Goal alignment (inverted error)
            global_error,
            uncertainty,
            social_factor,
        );

        // Choose action based on current policy and exploration strategy
        match self.exploration_strategy {
            ExplorationStrategy::EpsilonGreedy => self.select_epsilon_greedy(available_actions),
            ExplorationStrategy::UCB1 => self.select_ucb1(available_actions),
            ExplorationStrategy::ThompsonSampling => {
                self.select_thompson_sampling(available_actions)
            }
        }
    }

    /// Epsilon-greedy action selection.
    fn select_epsilon_greedy(&mut self, available_actions: &[&str]) -> String {
        let exploration_rate = self.policy.get_parameter("exploration_rate").unwrap_or(0.1);

        if rand::random::<f64>() < exploration_rate {
            // Explore - random action
            let idx = rand::random::<usize>() % available_actions.len();
            available_actions[idx].to_string()
        } else {
            // Exploit - best action from Q-table
            let mut best_action = available_actions[0];
            let mut best_q = f64::NEG_INFINITY;

            for &action in available_actions {
                let q_value = *self.q_table.get(action).unwrap_or(&0.0);
                if q_value > best_q {
                    best_q = q_value;
                    best_action = action;
                }
            }

            best_action.to_string()
        }
    }

    /// UCB1 action selection.
    fn select_ucb1(&mut self, available_actions: &[&str]) -> String {
        // Simple UCB1 implementation
        let total_visits: u64 = self.q_table.values().map(|_| 1).sum();

        if total_visits == 0 {
            // If no visits yet, choose randomly
            let idx = rand::random::<usize>() % available_actions.len();
            return available_actions[idx].to_string();
        }

        let mut best_action = available_actions[0];
        let mut best_ucb = f64::NEG_INFINITY;

        for &action in available_actions {
            let q_value = *self.q_table.get(action).unwrap_or(&0.0);
            let visits = self.q_table.get(action).map(|_| 1).unwrap_or(0) as u64;

            // UCB1 formula: Q + sqrt(2 * ln(total_visits) / visits)
            let ucb_value = q_value + (2.0 * (total_visits as f64).ln() / visits as f64).sqrt();

            if ucb_value > best_ucb {
                best_ucb = ucb_value;
                best_action = action;
            }
        }

        best_action.to_string()
    }

    /// Thompson sampling action selection.
    fn select_thompson_sampling(&mut self, available_actions: &[&str]) -> String {
        // Simple implementation - return best action
        let mut best_action = available_actions[0];
        let mut best_q = f64::NEG_INFINITY;

        for &action in available_actions {
            let q_value = *self.q_table.get(action).unwrap_or(&0.0);
            if q_value > best_q {
                best_q = q_value;
                best_action = action;
            }
        }

        best_action.to_string()
    }

    /// Update Q-value for an action.
    pub fn update_q_value(&mut self, action: &str, reward: f64, next_state_q: f64) {
        let current_q = *self.q_table.get(action).unwrap_or(&0.0);
        let alpha = 0.1; // Learning rate
        let gamma = 0.9; // Discount factor

        let new_q = current_q + alpha * (reward + gamma * next_state_q - current_q);
        self.q_table.insert(action.to_string(), new_q);
    }

    /// Get Q-values for all available actions.
    pub fn get_q_values(&self) -> &HashMap<String, f64> {
        &self.q_table
    }
}

/// Policy evolution metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMetrics {
    pub policy_stability: f64,
    pub adaptation_rate: f64,
    pub exploration_explotation_balance: f64,
    pub learning_progress: f64,
    pub timestamp: i64,
}

impl PolicyMetrics {
    /// Calculate policy evolution metrics.
    pub fn calculate(policy_evolution: &PolicyEvolution) -> Self {
        let mut stability = 0.0;
        let mut adaptation_rate = 0.0;
        let mut exploration_exploration_balance = 0.0;
        let mut learning_progress = 0.0;

        if !policy_evolution.policy_history.is_empty() {
            // Calculate stability as variance of policy parameters
            let history_len = policy_evolution.policy_history.len();
            let mut avg_params: HashMap<String, f64> = policy_evolution
                .policy_history
                .iter()
                .flat_map(|state| state.parameters.iter())
                .fold(HashMap::new(), |mut acc, (key, value)| {
                    *acc.entry(key.clone()).or_insert(0.0) += value;
                    acc
                });

            for (_, sum) in &mut avg_params {
                *sum /= history_len as f64;
            }

            let variance: f64 = policy_evolution
                .policy_history
                .iter()
                .map(|state| {
                    state
                        .parameters
                        .iter()
                        .map(|(key, value)| {
                            let avg = *avg_params.get(key).unwrap_or(&0.0);
                            (value - avg).powi(2)
                        })
                        .sum::<f64>()
                })
                .sum();

            stability = 1.0 - (variance / history_len as f64).min(1.0);

            // Calculate adaptation rate
            if policy_evolution.policy_history.len() >= 2 {
                let first_state = &policy_evolution.policy_history[0];
                let last_state =
                    &policy_evolution.policy_history[policy_evolution.policy_history.len() - 1];

                let param_changes: f64 = first_state
                    .parameters
                    .iter()
                    .zip(last_state.parameters.iter())
                    .map(|((key, first_val), (_, last_val))| (first_val - last_val).abs())
                    .sum();

                adaptation_rate = param_changes / first_state.parameters.len() as f64;
            }

            // Calculate exploration/exploration balance
            let exploration_rate = policy_evolution
                .get_parameter("exploration_rate")
                .unwrap_or(0.1);
            let exploitation_rate = policy_evolution
                .get_parameter("exploitation_rate")
                .unwrap_or(0.9);

            exploration_exploration_balance = (exploration_rate - exploitation_rate).abs();

            // Learning progress based on parameter changes over time
            learning_progress = stability * adaptation_rate;
        }

        Self {
            policy_stability: stability,
            adaptation_rate,
            exploration_explotation_balance: exploration_exploration_balance,
            learning_progress,
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_evolution_creation() {
        let weights = PolicyWeights::default();
        let mut policy = PolicyEvolution::new(0.01, weights);

        assert!(!policy.get_policy().is_empty());
        assert_eq!(policy.learning_rate, 0.01);
    }

    #[test]
    fn test_policy_update() {
        let weights = PolicyWeights::default();
        let mut policy = PolicyEvolution::new(0.01, weights);

        policy.update_policy(0.8, 0.9, 0.2, 0.3, 0.7);

        assert!(!policy.get_policy().is_empty());
    }

    #[test]
    fn test_action_selector_creation() {
        let selector = ActionSelector::new();
        assert!(selector.q_table.is_empty());
    }

    #[test]
    fn test_policy_metrics_calculation() {
        let weights = PolicyWeights::default();
        let mut policy = PolicyEvolution::new(0.01, weights);

        // Update policy a few times to create history
        for _ in 0..5 {
            policy.update_policy(0.8, 0.9, 0.2, 0.3, 0.7);
        }

        let metrics = PolicyMetrics::calculate(&policy);
        assert!(metrics.policy_stability >= 0.0);
        assert!(metrics.adaptation_rate >= 0.0);
    }
}
