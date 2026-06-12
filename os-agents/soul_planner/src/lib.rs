//! # soul_planner — Boucle Cognitive
//!
//! Observe → Planifie → Agit → Évalue → Décide.
//!
//! ## Exemple
//! ```ignore
//! use soul_planner::*;
//! let mut planner = CognitiveLoop::new();
//! let goal = Goal {
//!     id: uuid::Uuid::new_v4().to_string(),
//!     description: "Analyser les logs".into(),
//!     priority: 5,
//!     created_at: chrono::Utc::now(),
//!     status: GoalStatus::Active,
//! };
//! planner.create_plan(&goal, &["ls", "grep"]);
//! ```

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const WORKING_MEMORY_CAPACITY: usize = 100;
const ACTION_HISTORY_CAPACITY: usize = 200;

// ── Types de buts ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub priority: u8, // 1-10
    pub created_at: chrono::DateTime<Utc>,
    #[serde(default = "active_default")]
    pub status: GoalStatus,
}

fn active_default() -> GoalStatus {
    GoalStatus::Active
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalStatus {
    Active,
    InProgress,
    Completed,
    Failed,
}

// ── Types de plan ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub goal_id: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub command: String,
    pub description: String,
    #[serde(default = "default_order")]
    pub order: usize,
}

fn default_order() -> usize {
    0
}

// ── Évaluation ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub score: f32, // 0.0 - 1.0
    pub feedback: String,
}

// ── Décision ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub action: String,
    pub reasoning: String,
    pub confidence: f32, // 0.0 - 1.0
}

// ── Mémoire de travail (buffer circulaire) ───────────────────

#[derive(Debug, Clone)]
pub struct WorkingMemory {
    buffer: Vec<String>,
    max_size: usize,
}

impl WorkingMemory {
    pub fn new(max_size: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn observe(&mut self, observation: String) {
        self.buffer.push(observation);
        if self.buffer.len() > self.max_size {
            self.buffer.remove(0);
        }
    }

    pub fn recent_observations(&self, n: usize) -> Vec<String> {
        let start = self.buffer.len().saturating_sub(n);
        self.buffer[start..].to_vec()
    }
}

// ── Historique d'actions ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub command: String,
    pub result: String,
    pub success: bool,
    pub timestamp: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ActionHistory {
    records: Vec<ActionRecord>,
    max_size: usize,
}

impl ActionHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            records: Vec::with_capacity(max_size),
            max_size,
        }
    }

    pub fn record(&mut self, command: String, result: String, success: bool) {
        self.records.push(ActionRecord {
            command,
            result,
            success,
            timestamp: Utc::now(),
        });
        if self.records.len() > self.max_size {
            self.records.remove(0);
        }
    }

    pub fn success_rate(&self) -> f32 {
        if self.records.is_empty() {
            return 0.0;
        }
        let success_count = self.records.iter().filter(|r| r.success).count();
        success_count as f32 / self.records.len() as f32
    }

    pub fn recent(&self, n: usize) -> Vec<&ActionRecord> {
        let start = self.records.len().saturating_sub(n);
        self.records[start..].iter().collect()
    }
}

// ── Boucle cognitive ────────────────────────────────────────

pub struct CognitiveLoop {
    pub memory: WorkingMemory,
    pub history: ActionHistory,
}

impl CognitiveLoop {
    pub fn new() -> Self {
        Self {
            memory: WorkingMemory::new(WORKING_MEMORY_CAPACITY),
            history: ActionHistory::new(ACTION_HISTORY_CAPACITY),
        }
    }

    pub fn create_plan(&self, goal: &Goal, step_commands: &[&str]) -> Plan {
        Plan {
            id: Uuid::new_v4().to_string(),
            goal_id: goal.id.clone(),
            steps: step_commands
                .iter()
                .enumerate()
                .map(|(i, cmd)| Step {
                    command: cmd.to_string(),
                    description: format!("Étape {}: {}", i + 1, goal.description),
                    order: i,
                })
                .collect(),
        }
    }

    /// Évalue un plan en fonction du résultat observé.
    ///
    /// Le score combine :
    /// - Le signal du résultat observé (succès/échec/error)
    /// - Le taux de succès historique de l'agent
    /// - La complexité du plan (nombre d'étapes)
    pub fn evaluate_plan(&self, plan: &Plan, outcome: &str) -> Evaluation {
        let outcome_lower = outcome.to_lowercase();

        // Signal du résultat observé
        let (base_score, signal) = if outcome_lower.contains("success")
            || outcome_lower.contains("ok")
            || outcome_lower.contains("completed")
        {
            (0.9, "succès détecté")
        } else if outcome_lower.contains("error")
            || outcome_lower.contains("fail")
            || outcome_lower.contains("panic")
        {
            (0.1, "échec détecté")
        } else if outcome_lower.contains("timeout") {
            (0.2, "timeout détecté")
        } else if outcome_lower.contains("partial") || outcome_lower.contains("retry") {
            (0.5, "résultat partiel")
        } else {
            (0.5, "résultat ambigu")
        };

        // Ajustement basé sur le taux de succès historique
        // Si l'historique est vide, pas d'ajustement
        let historical_rate = self.history.success_rate();
        let adjusted = if self.history.records.is_empty() {
            base_score
        } else if historical_rate > 0.8 && base_score > 0.5 {
            (base_score * 0.7 + historical_rate * 0.3).min(1.0)
        } else if historical_rate < 0.3 && base_score > 0.5 {
            (base_score * 0.8 + historical_rate * 0.2).min(1.0)
        } else {
            base_score
        };

        // Pénalité pour les plans très longs (plus d'étapes = plus de risques)
        let complexity_penalty = if plan.steps.len() > 5 {
            0.05 * (plan.steps.len() as f32 - 5.0).min(3.0)
        } else {
            0.0
        };

        let final_score = (adjusted - complexity_penalty).clamp(0.0, 1.0);

        Evaluation {
            score: final_score,
            feedback: format!("{} | historique: {:.0}%, étapes: {}", signal, historical_rate * 100.0, plan.steps.len()),
        }
    }

    /// Décide de la prochaine action en se basant sur le contexte et l'historique.
    ///
    /// La décision prend en compte :
    /// - Le taux de succès historique
    /// - Les signaux du contexte (erreurs, succès, patterns connus)
    /// - La confiance est proportionnelle à la fiabilité observée
    pub fn decide(&self, context: &str) -> Decision {
        let ctx_lower = context.to_lowercase();
        let historical_rate = self.history.success_rate();

        // Analyse du contexte
        let (action, reasoning) = if ctx_lower.contains("error") || ctx_lower.contains("fail") {
            if historical_rate < 0.3 {
                (
                    "abort".to_string(),
                    format!(
                        "Taux d'échec élevé ({:.0}%) et contexte d'erreur — arrêt recommandé",
                        historical_rate * 100.0
                    ),
                )
            } else {
                (
                    "retry_with_adjustment".to_string(),
                    format!(
                        "Erreur détectée mais historique OK ({:.0}%) — réessayer avec ajustement",
                        historical_rate * 100.0
                    ),
                )
            }
        } else if ctx_lower.contains("success") || ctx_lower.contains("completed") {
            (
                "proceed".to_string(),
                format!(
                    "Succès confirmé, historique: {:.0}% — continuer",
                    historical_rate * 100.0
                ),
            )
        } else if ctx_lower.contains("budget_exceeded") || ctx_lower.contains("token_limit") {
            (
                "pause_and_summarize".to_string(),
                "Budget dépassé — pauses pour résumer l'état d'avancement".to_string(),
            )
        } else if ctx_lower.contains("timeout") {
            (
                "simplify_and_retry".to_string(),
                "Timeout — simplifier le plan et réessayer".to_string(),
            )
        } else if historical_rate > 0.8 {
            (
                "continue".to_string(),
                format!(
                    "Historique fiable ({:.0}%) — continuer le plan actuel",
                    historical_rate * 100.0
                ),
            )
        } else if historical_rate < 0.3 {
            (
                "replan".to_string(),
                format!(
                    "Historique peu fiable ({:.0}%) — repenser l'approche",
                    historical_rate * 100.0
                ),
            )
        } else {
            (
                "continue".to_string(),
                format!(
                    "Contexte neutre, historique: {:.0}% — continuer prudemment",
                    historical_rate * 100.0
                ),
            )
        };

        // La confiance est basée sur la cohérence entre le signal du contexte
        // et le taux de succès historique
        let confidence = if historical_rate > 0.8 {
            0.85
        } else if historical_rate > 0.5 {
            0.65
        } else if historical_rate > 0.3 {
            0.45
        } else {
            0.25
        };

        Decision {
            action,
            reasoning,
            confidence,
        }
    }
}

impl Default for CognitiveLoop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_goal(description: &str) -> Goal {
        Goal {
            id: Uuid::new_v4().to_string(),
            description: description.to_string(),
            priority: 5,
            created_at: Utc::now(),
            status: GoalStatus::Active,
        }
    }

    #[test]
    fn create_plan_produces_steps() {
        let planner = CognitiveLoop::new();
        let goal = make_goal("Analyser les logs");
        let plan = planner.create_plan(&goal, &["ls", "grep", "sort"]);
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[0].command, "ls");
        assert_eq!(plan.steps[1].command, "grep");
        assert_eq!(plan.steps[2].command, "sort");
        assert_eq!(plan.steps[0].order, 0);
        assert_eq!(plan.steps[2].order, 2);
    }

    #[test]
    fn evaluate_plan_success_score_high() {
        let planner = CognitiveLoop::new();
        let goal = make_goal("test");
        let plan = planner.create_plan(&goal, &["step1"]);
        let eval = planner.evaluate_plan(&plan, "success");
        assert!(eval.score > 0.8, "score should be > 0.8, got {}", eval.score);
    }

    #[test]
    fn evaluate_plan_error_score_low() {
        let planner = CognitiveLoop::new();
        let goal = make_goal("test");
        let plan = planner.create_plan(&goal, &["step1"]);
        let eval = planner.evaluate_plan(&plan, "error: something failed");
        assert!(eval.score < 0.3, "score should be < 0.3, got {}", eval.score);
    }

    #[test]
    fn evaluate_plan_complexity_penalty() {
        let planner = CognitiveLoop::new();
        let goal = make_goal("test");
        let long_plan = planner.create_plan(&goal, &["a", "b", "c", "d", "e", "f", "g"]);
        let short_plan = planner.create_plan(&goal, &["a"]);
        let eval_long = planner.evaluate_plan(&long_plan, "success");
        let eval_short = planner.evaluate_plan(&short_plan, "success");
        assert!(
            eval_long.score < eval_short.score,
            "long plan should score lower: {} < {}",
            eval_long.score,
            eval_short.score
        );
    }

    #[test]
    fn decide_on_error_with_low_history_returns_abort() {
        let mut planner = CognitiveLoop::new();
        // Enregistrer des échecs pour abaisser le taux de succès
        for _ in 0..10 {
            planner.history.record("cmd".into(), "error".into(), false);
        }
        let decision = planner.decide("error occurred");
        assert_eq!(decision.action, "abort");
        assert!(decision.confidence < 0.5);
    }

    #[test]
    fn decide_on_success_with_high_history_returns_proceed() {
        let mut planner = CognitiveLoop::new();
        for _ in 0..10 {
            planner.history.record("cmd".into(), "ok".into(), true);
        }
        let decision = planner.decide("success");
        assert_eq!(decision.action, "proceed");
        assert!(decision.confidence > 0.7);
    }

    #[test]
    fn decide_on_timeout_returns_simplify() {
        let planner = CognitiveLoop::new();
        let decision = planner.decide("timeout after 30s");
        assert_eq!(decision.action, "simplify_and_retry");
    }

    #[test]
    fn working_memory_circular_buffer() {
        let mut mem = WorkingMemory::new(3);
        mem.observe("a".into());
        mem.observe("b".into());
        mem.observe("c".into());
        mem.observe("d".into()); // "a" evicted
        let recent = mem.recent_observations(10);
        assert_eq!(recent, vec!["b", "c", "d"]);
    }

    #[test]
    fn action_history_success_rate() {
        let mut hist = ActionHistory::new(100);
        hist.record("cmd1".into(), "ok".into(), true);
        hist.record("cmd2".into(), "ok".into(), true);
        hist.record("cmd3".into(), "error".into(), false);
        let rate = hist.success_rate();
        assert!((rate - 2.0 / 3.0).abs() < 0.01);
    }
}
