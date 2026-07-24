use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionScore {
    pub attempts: u64,
    pub successes: u64,
    #[serde(skip, default = "Instant::now")]
    pub last_success: Instant,
}

impl EvolutionScore {
    pub fn fitness(&self) -> f64 {
        if self.attempts == 0 {
            return 0.5;
        }
        self.successes as f64 / self.attempts as f64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionTracker {
    #[serde(skip, default = "default_hashmap")]
    family_scores: HashMap<String, EvolutionScore>,
    pub total_mutations: u64,
    pub exploration_rate: f64,
}

fn default_hashmap<T>() -> HashMap<String, T> {
    HashMap::new()
}

impl EvolutionTracker {
    pub fn new(exploration_rate: f64) -> Self {
        EvolutionTracker {
            family_scores: HashMap::new(),
            total_mutations: 0,
            exploration_rate,
        }
    }

    pub fn record(&mut self, algo: &str, success: bool) {
        self.total_mutations += 1;
        let score = self
            .family_scores
            .entry(algo.to_string())
            .or_insert(EvolutionScore {
                attempts: 0,
                successes: 0,
                last_success: Instant::now(),
            });
        score.attempts += 1;
        if success {
            score.successes += 1;
            score.last_success = Instant::now();
        }
    }

    pub fn record_with_fitness(&mut self, algo: &str, fitness: f64) {
        self.total_mutations += 1;
        let score = self
            .family_scores
            .entry(algo.to_string())
            .or_insert(EvolutionScore {
                attempts: 0,
                successes: 0,
                last_success: Instant::now(),
            });
        score.attempts += 1;
        if fitness > 0.5 {
            score.successes += 1;
            score.last_success = Instant::now();
        }
    }

    pub fn fitness(&self, algo: &str) -> f64 {
        self.family_scores
            .get(algo)
            .map(|s| s.fitness())
            .unwrap_or(0.5)
    }

    pub fn best_family(&self) -> Option<(String, f64)> {
        self.family_scores
            .iter()
            .filter(|(_, s)| s.attempts >= 2)
            .max_by(|(_, a), (_, b)| {
                a.fitness()
                    .partial_cmp(&b.fitness())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(name, score)| (name.clone(), score.fitness()))
    }

    pub fn should_explore(&self) -> bool {
        rand::random::<f64>() < self.exploration_rate
    }

    pub fn total_mutations(&self) -> u64 {
        self.total_mutations
    }

    pub fn summary(&self) -> Vec<(String, u64, u64, f64)> {
        let mut result: Vec<_> = self
            .family_scores
            .iter()
            .map(|(name, score)| {
                (
                    name.clone(),
                    score.attempts,
                    score.successes,
                    score.fitness(),
                )
            })
            .collect();
        result.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
        result
    }

    pub fn get_family_score(&self, algo: &str) -> Option<EvolutionScore> {
        self.family_scores.get(algo).cloned()
    }

    pub fn merge(&mut self, other: &EvolutionTracker) {
        for (algo, score) in &other.family_scores {
            let entry = self
                .family_scores
                .entry(algo.clone())
                .or_insert(EvolutionScore {
                    attempts: 0,
                    successes: 0,
                    last_success: Instant::now(),
                });
            entry.attempts += score.attempts;
            entry.successes += score.successes;
            if score.last_success > entry.last_success {
                entry.last_success = score.last_success;
            }
        }
        self.total_mutations += other.total_mutations;
    }

    pub fn decay_old_scores(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.family_scores
            .retain(|_, score| now.duration_since(score.last_success) < max_age);
    }
}

pub type SharedEvolutionTracker = Arc<RwLock<EvolutionTracker>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_tracker() -> EvolutionTracker {
        EvolutionTracker::new(0.5)
    }

    #[test]
    fn test_new_tracker_default_fitness() {
        let t = fresh_tracker();
        assert_eq!(t.fitness("nonexistent"), 0.5);
        assert!(!t.best_family().is_some());
    }

    #[test]
    fn test_record_success() {
        let mut t = fresh_tracker();
        t.record("identity", true);
        assert_eq!(t.total_mutations(), 1);
        assert_eq!(t.fitness("identity"), 1.0);
    }

    #[test]
    fn test_record_failure() {
        let mut t = fresh_tracker();
        t.record("amplify", false);
        assert_eq!(t.fitness("amplify"), 0.0);
    }

    #[test]
    fn test_multiple_records() {
        let mut t = fresh_tracker();
        t.record("identity", true);
        t.record("identity", true);
        t.record("identity", false);
        assert_eq!(t.fitness("identity"), 2.0 / 3.0);
    }

    #[test]
    fn test_best_family() {
        let mut t = fresh_tracker();
        t.record("identity", true);
        t.record("identity", true);
        t.record("amplify", false);
        t.record("amplify", false);
        let best = t.best_family();
        assert!(best.is_some());
        let (name, _) = best.unwrap();
        assert_eq!(name, "identity");
    }

    #[test]
    fn test_best_family_needs_two_attempts() {
        let mut t = fresh_tracker();
        t.record("identity", true);
        assert!(t.best_family().is_none());
    }

    #[test]
    fn test_summary_order() {
        let mut t = fresh_tracker();
        t.record("identity", true);
        t.record("identity", true);
        t.record("amplify", true);
        t.record("amplify", false);
        let summary = t.summary();
        assert_eq!(summary[0].0, "identity");
        assert_eq!(summary[1].0, "amplify");
    }

    #[test]
    fn test_merge() {
        let mut t1 = fresh_tracker();
        let mut t2 = fresh_tracker();
        t1.record("identity", true);
        t2.record("amplify", true);
        t1.merge(&t2);
        assert_eq!(t1.total_mutations(), 2);
        assert!(t1.fitness("amplify") > 0.0);
    }

    #[test]
    fn test_evolution_score_fitness() {
        let score = EvolutionScore {
            attempts: 10,
            successes: 7,
            last_success: Instant::now(),
        };
        assert!((score.fitness() - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_evolution_score_zero_attempts() {
        let score = EvolutionScore {
            attempts: 0,
            successes: 0,
            last_success: Instant::now(),
        };
        assert_eq!(score.fitness(), 0.5);
    }

    #[test]
    fn test_decay_old_scores() {
        let mut t = fresh_tracker();
        t.record("identity", true);
        t.decay_old_scores(Duration::from_secs(0));
        assert_eq!(t.fitness("identity"), 0.5);
    }

    #[test]
    fn test_should_explore() {
        let t = EvolutionTracker::new(0.0);
        assert!(!t.should_explore());
        let t = EvolutionTracker::new(1.0);
        assert!(t.should_explore());
    }

    #[test]
    fn test_get_family_score() {
        let mut t = fresh_tracker();
        t.record("identity", true);
        let score = t.get_family_score("identity");
        assert!(score.is_some());
        assert_eq!(score.unwrap().attempts, 1);
    }

    #[test]
    fn test_get_family_score_nonexistent() {
        let t = fresh_tracker();
        assert!(t.get_family_score("nothing").is_none());
    }
}
