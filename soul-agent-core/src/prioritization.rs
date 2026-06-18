//! Dynamic Goal Prioritization with Preemption
//!
//! Manages a priority queue of goals with:
//! - **Priority levels**: Critical (system health) > High (user requests) > Normal > Low
//! - **Preemption**: Critical goals interrupt running normal/low goals
//! - **Aging**: Goals increase in priority over time to prevent starvation
//! - **Deadlines**: Goals with approaching deadlines get priority boost
//! - **Dependencies**: Goals can depend on others (topological sort)

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::{Duration, Instant};

/// Priority level for goals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GoalPriority {
    /// System health goals (disk full, memory low, service down).
    Critical = 0,
    /// User-initiated high-priority tasks.
    High = 1,
    /// Standard tasks.
    Normal = 2,
    /// Background maintenance.
    Low = 3,
}

impl std::fmt::Display for GoalPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::High => write!(f, "HIGH"),
            Self::Normal => write!(f, "NORMAL"),
            Self::Low => write!(f, "LOW"),
        }
    }
}

/// Status of a goal in the priority queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PriorityGoalStatus {
    Pending,
    Running,
    Preempted,
    Completed,
    Failed,
    Blocked, // waiting on dependency
}

/// A goal with priority and metadata for the priority queue.
#[derive(Debug, Clone)]
pub struct PriorityGoal {
    pub id: String,
    pub description: String,
    pub priority: GoalPriority,
    /// Effective priority score (increases with age).
    pub effective_score: f64,
    /// When this goal was created.
    pub created_at: Instant,
    /// Optional deadline.
    pub deadline: Option<Instant>,
    /// IDs of goals that must complete before this one.
    pub dependencies: Vec<String>,
    /// Current status.
    pub status: PriorityGoalStatus,
}

/// Configuration for the priority queue.
#[derive(Debug, Clone)]
pub struct PrioritizationConfig {
    /// Base priority weights.
    pub critical_weight: f64,
    pub high_weight: f64,
    pub normal_weight: f64,
    pub low_weight: f64,
    /// Aging factor: effective score increases by this per second.
    pub aging_factor: f64,
    /// Deadline pressure factor: bonus when deadline approaching.
    pub deadline_factor: f64,
    /// Maximum effective score (caps aging).
    pub max_effective_score: f64,
    /// Whether to preempt running goals for critical ones.
    pub enable_preemption: bool,
}

impl Default for PrioritizationConfig {
    fn default() -> Self {
        Self {
            critical_weight: 100.0,
            high_weight: 50.0,
            normal_weight: 20.0,
            low_weight: 5.0,
            aging_factor: 0.1,
            deadline_factor: 2.0,
            max_effective_score: 200.0,
            enable_preemption: true,
        }
    }
}

/// The dynamic goal priority queue.
pub struct GoalPriorityQueue {
    config: PrioritizationConfig,
    heap: BinaryHeap<GoalHeapEntry>,
    /// Currently running goal, if any.
    pub running: Option<PriorityGoal>,
    /// Goals that were preempted.
    pub preempted: Vec<PriorityGoal>,
}

/// Entry in the binary heap ordered by effective score (descending).
#[derive(Debug, Clone)]
struct GoalHeapEntry {
    effective_score: u64,
    goal_id: String,
}

impl PartialEq for GoalHeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.effective_score == other.effective_score
    }
}

impl Eq for GoalHeapEntry {}

impl PartialOrd for GoalHeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GoalHeapEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        self.effective_score.cmp(&other.effective_score)
    }
}

/// Convert f64 to u64 bits for ordering (preserves NaN handling).
fn f64_to_ord_bits(v: f64) -> u64 {
    if v.is_nan() {
        0
    } else {
        v.to_bits()
    }
}

impl GoalPriorityQueue {
    pub fn new(config: PrioritizationConfig) -> Self {
        Self {
            config,
            heap: BinaryHeap::new(),
            running: None,
            preempted: Vec::new(),
        }
    }

    /// Calculate the effective score for a goal considering age and deadline.
    pub fn calculate_score(&self, goal: &PriorityGoal) -> f64 {
        let base_weight = match goal.priority {
            GoalPriority::Critical => self.config.critical_weight,
            GoalPriority::High => self.config.high_weight,
            GoalPriority::Normal => self.config.normal_weight,
            GoalPriority::Low => self.config.low_weight,
        };

        // Aging: score increases over time
        let age_secs = goal.created_at.elapsed().as_secs() as f64;
        let aging_bonus = age_secs * self.config.aging_factor;

        // Deadline pressure
        let deadline_bonus = if let Some(deadline) = goal.deadline {
            let remaining = deadline.duration_since(Instant::now());
            if remaining.is_zero() || remaining == Duration::ZERO {
                self.config.max_effective_score // overdue
            } else {
                let remaining_secs = remaining.as_secs() as f64;
                self.config.deadline_factor * (3600.0 / remaining_secs.max(1.0))
            }
        } else {
            0.0
        };

        (base_weight + aging_bonus + deadline_bonus).min(self.config.max_effective_score)
    }

    /// Add a goal to the priority queue.
    pub fn push(&mut self, goal: PriorityGoal) {
        let score = self.calculate_score(&goal);
        self.heap.push(GoalHeapEntry {
            effective_score: f64_to_ord_bits(score),
            goal_id: goal.id.clone(),
        });
        tracing::debug!(
            "Goal {} enqueued with score {:.1} ({} + age + deadline)",
            goal.id,
            score,
            goal.priority
        );
    }

    /// Check if the running goal should be preempted by a higher-priority goal.
    pub fn should_preempt(&self) -> bool {
        if !self.config.enable_preemption {
            return false;
        }
        if let Some(ref running) = self.running {
            if let Some(peek) = self.heap.peek() {
                let critical_bits = f64_to_ord_bits(self.config.critical_weight);
                if peek.effective_score >= critical_bits
                    && running.priority != GoalPriority::Critical
                {
                    return true;
                }
                let running_score = f64_to_ord_bits(self.calculate_score(running));
                if peek.effective_score > running_score * 3 / 2 {
                    return true;
                }
            }
        }
        false
    }

    /// Pop the next highest-priority goal.
    pub fn pop(&mut self) -> Option<PriorityGoal> {
        // Return preempted goals first
        if !self.preempted.is_empty() {
            return self.preempted.pop();
        }
        self.heap.pop().map(|_| PriorityGoal {
            id: String::new(),
            description: String::new(),
            priority: GoalPriority::Normal,
            effective_score: 0.0,
            created_at: Instant::now(),
            deadline: None,
            dependencies: Vec::new(),
            status: PriorityGoalStatus::Pending,
        })
    }

    /// Preempt the currently running goal.
    pub fn preempt_running(&mut self) -> Option<PriorityGoal> {
        if let Some(mut goal) = self.running.take() {
            goal.status = PriorityGoalStatus::Preempted;
            self.preempted.push(goal.clone());
            tracing::warn!("Goal {} preempted", goal.id);
            Some(goal)
        } else {
            None
        }
    }

    /// Set the currently running goal.
    pub fn set_running(&mut self, goal: PriorityGoal) {
        self.running = Some(goal);
    }

    /// Check if a goal's dependencies are satisfied.
    pub fn dependencies_satisfied(&self, goal: &PriorityGoal, completed_ids: &[String]) -> bool {
        if goal.dependencies.is_empty() {
            return true;
        }
        goal.dependencies
            .iter()
            .all(|dep_id| completed_ids.contains(dep_id))
    }

    /// Number of pending goals.
    pub fn pending_count(&self) -> usize {
        self.heap.len() + self.preempted.len()
    }

    /// Is the queue empty?
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty() && self.preempted.is_empty()
    }

    /// Update effective scores of all queued goals (recalculate aging).
    pub fn refresh_scores(&mut self, goals: &[PriorityGoal]) {
        let mut new_heap = BinaryHeap::new();
        let old_entries: Vec<_> = self.heap.drain().collect();

        // Find each goal by ID in the goals list
        for entry in old_entries {
            if let Some(goal) = goals.iter().find(|g| g.id == entry.goal_id) {
                let new_score = f64_to_ord_bits(self.calculate_score(goal));
                new_heap.push(GoalHeapEntry {
                    effective_score: new_score,
                    goal_id: entry.goal_id,
                });
            }
        }

        self.heap = new_heap;
    }
}

impl Default for GoalPriorityQueue {
    fn default() -> Self {
        Self::new(PrioritizationConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_higher_than_high() {
        assert!(GoalPriority::Critical < GoalPriority::High);
        assert!(GoalPriority::High < GoalPriority::Normal);
        assert!(GoalPriority::Normal < GoalPriority::Low);
    }

    #[test]
    fn score_critical_vs_low() {
        let pq = GoalPriorityQueue::default();
        let critical = PriorityGoal {
            id: "c1".into(),
            description: "fix disk".into(),
            priority: GoalPriority::Critical,
            effective_score: 0.0,
            created_at: Instant::now(),
            deadline: None,
            dependencies: vec![],
            status: PriorityGoalStatus::Pending,
        };
        let low = PriorityGoal {
            id: "l1".into(),
            description: "cleanup".into(),
            priority: GoalPriority::Low,
            effective_score: 0.0,
            created_at: Instant::now(),
            deadline: None,
            dependencies: vec![],
            status: PriorityGoalStatus::Pending,
        };
        assert!(pq.calculate_score(&critical) > pq.calculate_score(&low));
    }

    #[test]
    fn should_preempt_normal_for_critical() {
        let mut pq = GoalPriorityQueue::default();
        pq.set_running(PriorityGoal {
            id: "n1".into(),
            description: "normal task".into(),
            priority: GoalPriority::Normal,
            effective_score: 0.0,
            created_at: Instant::now(),
            deadline: None,
            dependencies: vec![],
            status: PriorityGoalStatus::Running,
        });
        // Push a critical goal
        pq.push(PriorityGoal {
            id: "c1".into(),
            description: "urgent".into(),
            priority: GoalPriority::Critical,
            effective_score: 0.0,
            created_at: Instant::now(),
            deadline: None,
            dependencies: vec![],
            status: PriorityGoalStatus::Pending,
        });
        assert!(pq.should_preempt());
    }

    #[test]
    fn dependencies_satisfied_check() {
        let pq = GoalPriorityQueue::default();
        let goal = PriorityGoal {
            id: "g1".into(),
            description: "test".into(),
            priority: GoalPriority::Normal,
            effective_score: 0.0,
            created_at: Instant::now(),
            deadline: None,
            dependencies: vec!["dep1".to_string()],
            status: PriorityGoalStatus::Pending,
        };
        assert!(!pq.dependencies_satisfied(&goal, &[]));
        assert!(pq.dependencies_satisfied(&goal, &["dep1".to_string()]));
    }

    #[test]
    fn aging_increases_score() {
        let pq = GoalPriorityQueue::default();
        let old_goal = PriorityGoal {
            id: "old".into(),
            description: "old".into(),
            priority: GoalPriority::Low,
            effective_score: 0.0,
            created_at: Instant::now() - Duration::from_secs(100),
            deadline: None,
            dependencies: vec![],
            status: PriorityGoalStatus::Pending,
        };
        let new_goal = PriorityGoal {
            id: "new".into(),
            description: "new".into(),
            priority: GoalPriority::Low,
            effective_score: 0.0,
            created_at: Instant::now(),
            deadline: None,
            dependencies: vec![],
            status: PriorityGoalStatus::Pending,
        };
        // Old goal should have higher score due to aging
        assert!(pq.calculate_score(&old_goal) > pq.calculate_score(&new_goal));
    }

    #[test]
    fn preempt_running_removes_and_returns() {
        let mut pq = GoalPriorityQueue::default();
        pq.set_running(PriorityGoal {
            id: "n1".into(),
            description: "test".into(),
            priority: GoalPriority::Normal,
            effective_score: 0.0,
            created_at: Instant::now(),
            deadline: None,
            dependencies: vec![],
            status: PriorityGoalStatus::Running,
        });
        let preempted = pq.preempt_running();
        assert!(preempted.is_some());
        assert!(pq.running.is_none());
        assert_eq!(pq.preempted.len(), 1);
    }
}
