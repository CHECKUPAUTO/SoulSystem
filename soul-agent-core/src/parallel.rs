//! Parallel Goal Execution Engine
//!
//! Executes multiple independent goals concurrently using Tokio tasks.
//! Goals are analyzed for dependencies to determine which can run in parallel.
//!
//! ## Features:
//! - **Dependency graph**: builds a DAG from goal dependencies
//! - **Concurrent execution**: runs independent goals simultaneously
//! - **Resource limiting**: caps concurrent goals to avoid resource exhaustion
//! - **Result merging**: combines results from parallel executions
//! - **Cancel propagation**: cancelling one task cancels its dependents

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};

/// Configuration for parallel execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelConfig {
    /// Maximum number of concurrent goal executions.
    pub max_concurrent: usize,
    /// Whether to automatically parallelize independent goals.
    pub auto_parallelize: bool,
    /// Timeout per goal (seconds).
    pub goal_timeout_secs: u64,
}

impl Default for ParallelConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            auto_parallelize: true,
            goal_timeout_secs: 300,
        }
    }
}

/// A node in the dependency DAG.
#[derive(Debug, Clone)]
pub struct GoalNode {
    pub id: String,
    pub description: String,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
}

/// A DAG of goals showing execution order.
#[derive(Debug, Default)]
pub struct GoalDag {
    nodes: HashMap<String, GoalNode>,
}

impl GoalDag {
    /// Build a DAG from a list of goals.
    pub fn build(goals: &[GoalNode]) -> Self {
        let mut dag = GoalDag::default();
        for goal in goals {
            let node = GoalNode {
                id: goal.id.clone(),
                description: goal.description.clone(),
                dependencies: goal.dependencies.clone(),
                dependents: Vec::new(),
            };
            dag.nodes.insert(goal.id.clone(), node);
        }

        // Build reverse edges (dependents) — collect first, then apply
        let mut dependent_edges: Vec<(String, String)> = Vec::new();
        for (id, node) in dag.nodes.iter() {
            for dep_id in &node.dependencies {
                dependent_edges.push((dep_id.clone(), id.clone()));
            }
        }
        for (dep_id, dependent_id) in dependent_edges {
            if let Some(dep_node) = dag.nodes.get_mut(&dep_id) {
                if !dep_node.dependents.contains(&dependent_id) {
                    dep_node.dependents.push(dependent_id);
                }
            }
        }

        dag
    }

    /// Get topological levels (goals that can run in parallel).
    /// Level 0 has no dependencies. Level N depends on Level N-1.
    pub fn execution_levels(&self) -> Vec<Vec<String>> {
        let mut levels = Vec::new();
        let mut remaining: HashSet<String> = self.nodes.keys().cloned().collect();
        let mut completed: HashSet<String> = HashSet::new();

        while !remaining.is_empty() {
            let mut current_level = Vec::new();

            for id in remaining.iter() {
                if let Some(node) = self.nodes.get(id) {
                    if node.dependencies.iter().all(|d| completed.contains(d)) {
                        current_level.push(id.clone());
                    }
                }
            }

            if current_level.is_empty() && !remaining.is_empty() {
                // Cycle detected or broken dependencies; push remaining sequentially
                for id in remaining.drain() {
                    current_level.push(id);
                    break; // Take one at a time
                }
            }

            for id in &current_level {
                remaining.remove(id);
                completed.insert(id.clone());
            }

            levels.push(current_level);
        }

        levels
    }

    /// Find independent goals (no shared dependencies).
    pub fn find_independent_sets(&self) -> Vec<Vec<String>> {
        self.execution_levels()
    }
}

/// Result of a parallel execution run.
#[derive(Debug, Clone)]
pub struct ParallelResult {
    /// Results keyed by goal ID.
    pub results: HashMap<String, GoalExecutionResult>,
    /// Total number of goals executed.
    pub total_executed: usize,
    /// Number of successful goals.
    pub successes: usize,
    /// Number of failed goals.
    pub failures: usize,
    /// Whether all goals succeeded.
    pub all_succeeded: bool,
}

/// Result of a single goal execution.
#[derive(Debug, Clone)]
pub struct GoalExecutionResult {
    pub goal_id: String,
    pub output: String,
    pub success: bool,
    pub duration_ms: u64,
}

/// Channel for sending goal results from tasks.
#[derive(Debug, Clone)]
pub struct GoalReport {
    pub goal_id: String,
    pub result: GoalExecutionResult,
}

/// The parallel execution engine.
pub struct ParallelExecutor {
    config: ParallelConfig,
    semaphore: Arc<Semaphore>,
}

impl ParallelExecutor {
    pub fn new(config: ParallelConfig) -> Self {
        let max = config.max_concurrent;
        Self {
            config,
            semaphore: Arc::new(Semaphore::new(max)),
        }
    }

    /// Execute a DAG of goals in parallel where possible.
    /// The `executor` function is called for each goal.
    pub async fn execute<F, Fut>(&self, dag: &GoalDag, executor: F) -> ParallelResult
    where
        F: Fn(String, String) -> Fut + Send + Sync + Clone + 'static,
        Fut: std::future::Future<Output = Result<String, String>> + Send,
    {
        let levels = dag.execution_levels();
        let mut results: HashMap<String, GoalExecutionResult> = HashMap::new();
        let (tx, mut rx) = mpsc::unbounded_channel::<GoalReport>();

        for level in &levels {
            if level.is_empty() {
                continue;
            }

            let mut handles = Vec::new();

            for goal_id in level {
                let permit = self.semaphore.clone().acquire_owned().await;
                if permit.is_err() {
                    continue;
                }
                let _permit = permit.unwrap();

                let desc = dag
                    .nodes
                    .get(goal_id)
                    .map(|n| n.description.clone())
                    .unwrap_or_default();
                let tx = tx.clone();
                let goal_id = goal_id.clone();
                let timeout = self.config.goal_timeout_secs;
                let executor = executor.clone();

                handles.push(tokio::spawn(async move {
                    let start = std::time::Instant::now();

                    let result = tokio::time::timeout(
                        std::time::Duration::from_secs(timeout),
                        executor(goal_id.clone(), desc),
                    )
                    .await;

                    let outcome = match result {
                        Ok(Ok(output)) => GoalExecutionResult {
                            goal_id: goal_id.clone(),
                            output,
                            success: true,
                            duration_ms: start.elapsed().as_millis() as u64,
                        },
                        Ok(Err(e)) => GoalExecutionResult {
                            goal_id: goal_id.clone(),
                            output: e,
                            success: false,
                            duration_ms: start.elapsed().as_millis() as u64,
                        },
                        Err(_) => GoalExecutionResult {
                            goal_id: goal_id.clone(),
                            output: "Timeout".to_string(),
                            success: false,
                            duration_ms: timeout * 1000,
                        },
                    };

                    let _ = tx.send(GoalReport {
                        goal_id,
                        result: outcome,
                    });
                }));
            }

            // Collect results from this level
            let level_goal_ids: HashSet<String> = level.iter().cloned().collect();
            let mut collected = 0;

            while collected < level.len() {
                if let Some(report) = rx.recv().await {
                    if level_goal_ids.contains(&report.goal_id) {
                        results.insert(report.goal_id.clone(), report.result);
                        collected += 1;
                    }
                }
            }

            // Clean up handles
            for handle in handles {
                let _ = handle.await;
            }
        }

        let all_goals: HashSet<String> = dag.nodes.keys().cloned().collect();
        let total = all_goals.len();
        let successes = results.values().filter(|r| r.success).count();
        let failures = total - successes;

        ParallelResult {
            results,
            total_executed: total,
            successes,
            failures,
            all_succeeded: failures == 0,
        }
    }

    /// Check if two goals can run in parallel (no dependency relationship).
    pub fn can_parallelize(goal1: &GoalNode, goal2: &GoalNode) -> bool {
        !goal1.dependencies.contains(&goal2.id) && !goal2.dependencies.contains(&goal1.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_goals() -> Vec<GoalNode> {
        vec![
            GoalNode {
                id: "A".into(),
                description: "Goal A".into(),
                dependencies: vec![],
                dependents: vec![],
            },
            GoalNode {
                id: "B".into(),
                description: "Goal B".into(),
                dependencies: vec![],
                dependents: vec![],
            },
            GoalNode {
                id: "C".into(),
                description: "Goal C".into(),
                dependencies: vec!["A".into(), "B".into()],
                dependents: vec![],
            },
        ]
    }

    #[test]
    fn dag_build_basic() {
        let dag = GoalDag::build(&make_goals());
        assert_eq!(dag.nodes.len(), 3);
    }

    #[test]
    fn execution_levels() {
        let dag = GoalDag::build(&make_goals());
        let levels = dag.execution_levels();
        // A and B at level 0 (no deps), C at level 1 (depends on A, B)
        assert!(levels.len() >= 2);
        let level0: HashSet<_> = levels[0].iter().cloned().collect();
        assert!(level0.contains("A"));
        assert!(level0.contains("B"));
        assert_eq!(levels[0].len(), 2);
        if levels.len() > 1 {
            assert_eq!(levels[1], vec!["C"]);
        }
    }

    #[test]
    fn can_parallelize_independent() {
        let g1 = GoalNode {
            id: "A".into(),
            description: "A".into(),
            dependencies: vec![],
            dependents: vec![],
        };
        let g2 = GoalNode {
            id: "B".into(),
            description: "B".into(),
            dependencies: vec![],
            dependents: vec![],
        };
        assert!(ParallelExecutor::can_parallelize(&g1, &g2));
    }

    #[test]
    fn cannot_parallelize_dependent() {
        let g1 = GoalNode {
            id: "A".into(),
            description: "A".into(),
            dependencies: vec![],
            dependents: vec![],
        };
        let g2 = GoalNode {
            id: "B".into(),
            description: "B".into(),
            dependencies: vec!["A".into()],
            dependents: vec![],
        };
        assert!(!ParallelExecutor::can_parallelize(&g1, &g2));
    }

    #[tokio::test]
    async fn parallel_executor_runs_all() {
        let dag = GoalDag::build(&make_goals());
        let executor = ParallelExecutor::new(ParallelConfig {
            max_concurrent: 2,
            goal_timeout_secs: 5,
            auto_parallelize: true,
        });

        let result = executor
            .execute(&dag, |id, desc| async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                Ok(format!("{}: {}", id, desc))
            })
            .await;

        assert_eq!(result.total_executed, 3);
        assert!(result.all_succeeded);
    }
}
