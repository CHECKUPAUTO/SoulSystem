//! Planner — Planificateur de buts avec prioritisation (urgence × impact × énergie)

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub description: String,
    pub priority: u8,
    pub urgency: f32,
    pub impact: f32,
    pub energy_cost: f32,
    pub source: GoalSource,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GoalSource {
    Llm,
    User,
    SystemAlert,
    AutoEvolution,
    Maintenance,
}

impl Goal {
    pub fn new(description: &str, priority: u8, source: GoalSource) -> Self {
        Self {
            description: description.to_string(),
            priority: priority.clamp(1, 10),
            urgency: 1.0,
            impact: 1.0,
            energy_cost: 0.5,
            source,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn score(&self) -> f32 {
        (self.priority as f32) * self.urgency * self.impact / (self.energy_cost + 0.1)
    }

    pub fn with_system_state(mut self, cpu: f32, mem: f32, _disk: f32, alerts: bool) -> Self {
        self.urgency = if alerts {
            2.0
        } else if cpu > 80.0 || mem > 90.0 {
            1.8
        } else if cpu > 50.0 || mem > 70.0 {
            1.4
        } else {
            1.0
        };
        self
    }

    pub fn with_action(mut self, action: &str) -> Self {
        self.impact = match action {
            "restart_service" => 1.5,
            "optimize_system" => 1.3,
            "alert" => 1.4,
            "investigate" => 1.2,
            "report" => 0.9,
            "explore" => 0.8,
            "wait" => 0.5,
            _ => 1.0,
        };
        self
    }
}

/// Planificateur de buts — utilisation d'un Vec trié
pub struct GoalPlanner {
    pub goals: Vec<Goal>,
    pub max_size: usize,
}

impl GoalPlanner {
    pub fn new(max_size: usize) -> Self {
        Self {
            goals: Vec::new(),
            max_size,
        }
    }

    pub fn push(&mut self, goal: Goal) {
        if self.goals.len() >= self.max_size {
            // Retirer le but le moins prioritaire
            self.goals
                .sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(Ordering::Equal));
            self.goals.pop();
        }
        self.goals.push(goal);
    }

    pub fn pop(&mut self) -> Option<Goal> {
        if self.goals.is_empty() {
            return None;
        }
        // Trouver le but le plus prioritaire
        let idx = self
            .goals
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.score().partial_cmp(&b.score()).unwrap_or(Ordering::Equal))
            .map(|(i, _)| i)?;
        Some(self.goals.remove(idx))
    }

    #[allow(dead_code)]
    pub fn peek(&self) -> Option<&Goal> {
        self.goals
            .iter()
            .max_by(|a, b| a.score().partial_cmp(&b.score()).unwrap_or(Ordering::Equal))
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.goals.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.goals.is_empty()
    }

    #[allow(dead_code)]
    pub fn to_vec(&self) -> Vec<Goal> {
        self.goals.clone()
    }

    #[allow(dead_code)]
    pub fn from_vec(goals: Vec<Goal>) -> Self {
        let mut planner = Self::new(goals.len().max(50));
        for goal in goals {
            planner.push(goal);
        }
        planner
    }
}
