use crate::types::{LlmConfig, TokenUsage};
use dashmap::DashMap;
use parking_lot::Mutex;

const CHARS_PER_TOKEN: usize = 4;

/// Budget LLM avec suivi de consommation par goal et par minute.
#[derive(Debug)]
pub struct LlmBudget {
    config: LlmConfig,
    goal_usage: DashMap<String, TokenUsage>,
    minute_usage: DashMap<String, TokenUsage>,
    minute_reset: Mutex<chrono::DateTime<chrono::Utc>>,
}

impl LlmBudget {
    pub fn new(config: LlmConfig) -> Self {
        Self {
            config,
            goal_usage: DashMap::new(),
            minute_usage: DashMap::new(),
            minute_reset: Mutex::new(chrono::Utc::now()),
        }
    }

    /// Estime le nombre de tokens à partir d'un prompt.
    pub fn estimate_tokens(prompt: &str, max_tokens: usize) -> usize {
        prompt.len() / CHARS_PER_TOKEN + max_tokens
    }

    fn current_minute_key(&self) -> String {
        chrono::Utc::now().format("%Y-%m-%d-%H-%M").to_string()
    }

    fn maybe_reset_minute(&self) {
        let mut reset = self.minute_reset.lock();
        let now = chrono::Utc::now();
        if now.signed_duration_since(*reset).num_minutes() >= 1 {
            self.minute_usage.clear();
            *reset = now;
        }
    }

    /// Vérifie si la requête peut être faite sans dépasser les budgets.
    pub fn check_budget(&self, goal_id: &str, estimated_tokens: usize) -> Result<(), String> {
        if self.config.goal_token_budget > 0 {
            if let Some(usage) = self.goal_usage.get(goal_id) {
                if usage.total_tokens + estimated_tokens > self.config.goal_token_budget {
                    return Err(format!(
                        "Goal {goal_id} exceeded token budget: {} > {}",
                        usage.total_tokens + estimated_tokens,
                        self.config.goal_token_budget
                    ));
                }
            }
        }

        if self.config.tokens_per_minute_budget > 0 {
            self.maybe_reset_minute();
            let minute_key = self.current_minute_key();
            if let Some(usage) = self.minute_usage.get(&minute_key) {
                if usage.total_tokens + estimated_tokens > self.config.tokens_per_minute_budget {
                    return Err(format!(
                        "Minute token budget exceeded: {} > {}",
                        usage.total_tokens + estimated_tokens,
                        self.config.tokens_per_minute_budget
                    ));
                }
            }
        }
        Ok(())
    }

    /// Enregistre l'usage après une requête (fourni par le provider).
    pub fn record_usage(&self, goal_id: &str, usage: &TokenUsage) {
        self.goal_usage
            .entry(goal_id.to_string())
            .and_modify(|u| {
                u.prompt_tokens += usage.prompt_tokens;
                u.completion_tokens += usage.completion_tokens;
                u.total_tokens += usage.total_tokens;
            })
        .or_insert_with(|| usage.clone());

        if self.config.tokens_per_minute_budget > 0 {
            let minute_key = self.current_minute_key();
            self.minute_usage
                .entry(minute_key)
                .and_modify(|u| {
                    u.prompt_tokens += usage.prompt_tokens;
                    u.completion_tokens += usage.completion_tokens;
                    u.total_tokens += usage.total_tokens;
                })
                .or_insert_with(|| usage.clone());
        }
    }

    /// Obtient l'usage pour un goal.
    pub fn get_goal_usage(&self, goal_id: &str) -> Option<TokenUsage> {
        self.goal_usage.get(goal_id).map(|u| u.clone())
    }

    /// Obtient l'usage de la minute courante.
    pub fn get_minute_usage(&self) -> TokenUsage {
        let minute_key = self.current_minute_key();
        self.minute_usage
            .get(&minute_key)
            .map(|u| u.clone())
            .unwrap_or_default()
    }

    /// Reset l'usage d'un goal (ex: après completion).
    pub fn reset_goal(&self, goal_id: &str) {
        self.goal_usage.remove(goal_id);
    }

    /// Retourne tous les usages par goal.
    pub fn all_goal_usages(&self) -> Vec<(String, TokenUsage)> {
        self.goal_usage
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }
}
