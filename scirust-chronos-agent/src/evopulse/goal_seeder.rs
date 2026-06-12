//! GoalSeeder — Traduction des lacunes et signaux d'inconfort en objectifs.
//!
//! Lit les lacunes détectées par SelfQuiz et les signaux d'inconfort élevé
//! du CuriosityEngine, génère des objectifs structurés, et les fusionne
//! dans le goal_store existant avec déduplication et priorisation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Objectif structuré généré par GoalSeeder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    pub description: String,
    pub priority: f64,
    pub source: String,
    pub created_at: String,
    pub status: String,
    pub pinned: bool,
    pub last_accessed: Option<String>,
}

/// Rapport du cycle GoalSeeder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeederReport {
    pub goals_created: usize,
    pub goals_merged: usize,
    pub goals_garbage_collected: usize,
    pub total_goals: usize,
}

/// Seeds des objectifs à partir de lacunes et de l'inconfort.
pub struct GoalSeeder {
    /// Seuil minimum de priorité pour la création.
    pub min_priority: f64,
    /// Priorité en dessous de laquelle un objectif non consulté est éligible au GC.
    pub gc_priority_threshold: f64,
    /// Nombre de jours sans consultation pour le GC.
    pub gc_unused_days: u64,
    /// Compteur interne pour IDs.
    counter: u64,
}

impl Default for GoalSeeder {
    fn default() -> Self {
        Self {
            min_priority: 0.1,
            gc_priority_threshold: 0.1,
            gc_unused_days: 14,
            counter: 0,
        }
    }
}

impl GoalSeeder {
    pub fn new(min_priority: f64, gc_priority_threshold: f64, gc_unused_days: u64) -> Self {
        Self { min_priority, gc_priority_threshold, gc_unused_days, counter: 0 }
    }

    /// Génère un objectif à partir d'une lacune.
    pub fn goal_from_lacuna(&mut self, description: &str, confidence: f64, discomfort: f64) -> Goal {
        self.counter += 1;
        let priority = (discomfort * (1.0 - confidence)).max(0.0);
        Goal {
            id: format!("goal-evopulse-{:04}", self.counter),
            description: description.to_string(),
            priority,
            source: "evopulse-selfquiz".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            status: "active".into(),
            pinned: false,
            last_accessed: None,
        }
    }

    /// Fusionne une liste de nouveaux objectifs avec le store existant,
    /// en évitant les doublons par similarité de description.
    pub fn merge_goals(&mut self, new_goals: Vec<Goal>, existing: &mut Vec<Goal>) -> SeederReport {
        let mut created = 0;
        let mut merged = 0;

        for goal in new_goals {
            // Vérifier si un objectif similaire existe déjà
            let is_duplicate = existing.iter().any(|g| {
                simple_similarity(&g.description, &goal.description) > 0.7
            });

            if is_duplicate {
                // Fusionner : mettre à jour la priorité si plus élevée
                if let Some(existing_goal) = existing.iter_mut().find(|g| {
                    simple_similarity(&g.description, &goal.description) > 0.7
                }) {
                    existing_goal.priority = existing_goal.priority.max(goal.priority);
                    merged += 1;
                }
            } else if goal.priority >= self.min_priority {
                existing.push(goal);
                created += 1;
            }
        }

        SeederReport {
            goals_created: created,
            goals_merged: merged,
            goals_garbage_collected: 0,
            total_goals: existing.len(),
        }
    }

    /// Garbage collection : supprime les objectifs de basse priorité non consultés.
    pub fn garbage_collect(&self, goals: &mut Vec<Goal>) -> usize {
        let now = chrono::Utc::now();
        let cutoff_days = chrono::Duration::days(self.gc_unused_days as i64);
        let before = goals.len();

        goals.retain(|g| {
            if g.pinned {
                return true;
            }
            if g.priority >= self.gc_priority_threshold {
                return true;
            }
            match &g.last_accessed {
                Some(last) => {
                    let last_time = chrono::DateTime::parse_from_rfc3339(last)
                        .map(|dt| dt.naive_utc())
                        .unwrap_or(now.naive_utc());
                    let elapsed = now.naive_utc() - last_time;
                    elapsed < cutoff_days
                }
                None => true, // jamais consulté → garder
            }
        });

        before - goals.len()
    }

    /// Sauvegarde les objectifs dans un fichier JSON.
    pub fn save_goals(goals: &[Goal], path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(goals).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    /// Charge les objectifs depuis un fichier JSON.
    pub fn load_goals(path: &str) -> Result<Vec<Goal>, String> {
        let data = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&data).map_err(|e| e.to_string())
    }
}

/// Similarité cosinus simplifiée entre deux chaînes (basée sur le nombre de mots communs).
fn simple_similarity(a: &str, b: &str) -> f64 {
    let words_a: Vec<&str> = a.split_whitespace().collect();
    let words_b: Vec<&str> = b.split_whitespace().collect();
    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }
    let set_a: std::collections::HashSet<&&str> = words_a.iter().collect();
    let set_b: std::collections::HashSet<&&str> = words_b.iter().collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 { 0.0 } else { intersection as f64 / union as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goal_from_lacuna() {
        let mut seeder = GoalSeeder::default();
        let goal = seeder.goal_from_lacuna("comprendre la dépendance entre A et B", 0.6, 0.3);
        assert!(goal.id.starts_with("goal-evopulse"));
        assert_eq!(goal.status, "active");
        assert!((goal.priority - 0.3 * 0.4).abs() < 1e-6);
    }

    #[test]
    fn test_merge_goals_deduplicates() {
        let mut seeder = GoalSeeder::default();
        let mut existing = vec![
            Goal {
                id: "existing-1".into(), description: "apprendre le calcul tensoriel".into(),
                priority: 0.5, source: "manual".into(), created_at: "".into(),
                status: "active".into(), pinned: false, last_accessed: None,
            }
        ];
        let new_goals = vec![
            seeder.goal_from_lacuna("apprendre le calcul tensoriel", 0.5, 0.2),
        ];
        let report = seeder.merge_goals(new_goals, &mut existing);
        assert_eq!(report.goals_created, 0); // dédoublonné
        assert_eq!(report.goals_merged, 1);
    }

    #[test]
    fn test_simple_similarity() {
        let sim = simple_similarity("apprendre le calcul tensoriel", "apprendre le calcul matriciel");
        assert!(sim > 0.5);
        assert!(sim < 1.0);
    }

    #[test]
    fn test_garbage_collect_preserves_pinned() {
        let seeder = GoalSeeder::default();
        let mut goals = vec![
            Goal {
                id: "pinned-1".into(), description: "important".into(),
                priority: 0.01, source: "".into(), created_at: "".into(),
                status: "active".into(), pinned: true, last_accessed: None,
            }
        ];
        let removed = seeder.garbage_collect(&mut goals);
        assert_eq!(removed, 0);
        assert_eq!(goals.len(), 1);
    }
}
