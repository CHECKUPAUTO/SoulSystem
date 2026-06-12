//! Inference memory — traces reasoning paths and enables heuristic search.
//!
//! Each inference step is recorded, and full reasoning paths can be
//! reconstructed. Supports A* search over the inference space and
//! pruning of low-confidence paths.

use scirust_symbolic::Expr;
use std::collections::{HashMap, HashSet, BinaryHeap};
use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Inference Step
// ---------------------------------------------------------------------------

/// A single step in an inference chain.
#[derive(Debug, Clone)]
pub struct InferenceStep {
    pub step_id: String,
    pub hypothesis: Expr,
    pub conclusion: Option<Expr>,
    pub confidence: f64,
    pub explored: bool,
    pub timestamp: u64,
    pub parent_id: Option<String>,
}

impl InferenceStep {
    pub fn new(
        step_id: &str,
        hypothesis: Expr,
        confidence: f64,
        explored: bool,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            step_id: step_id.to_string(),
            hypothesis,
            conclusion: None,
            confidence,
            explored,
            timestamp,
            parent_id: None,
        }
    }

    pub fn with_conclusion(mut self, conclusion: Expr) -> Self {
        self.conclusion = Some(conclusion);
        self
    }

    pub fn with_parent(mut self, parent_id: &str) -> Self {
        self.parent_id = Some(parent_id.to_string());
        self
    }
}

// ---------------------------------------------------------------------------
// Inference Path
// ---------------------------------------------------------------------------

/// A complete reasoning path consisting of ordered steps.
#[derive(Debug, Clone)]
pub struct InferencePath {
    pub steps: Vec<InferenceStep>,
    pub path_id: String,
    pub total_confidence: f64,
    pub length: usize,
}

impl InferencePath {
    /// Create an inference path from a sequence of steps.
    pub fn new(steps: Vec<InferenceStep>) -> Self {
        let total_confidence: f64 = steps.iter().map(|s| s.confidence).product();
        let path_id = format!("path_{}", steps.first()
            .map(|s| s.step_id.as_str())
            .unwrap_or("empty"));
        let length = steps.len();
        Self { steps, path_id, total_confidence, length }
    }

    /// Get the initial hypothesis (first step).
    pub fn initial_hypothesis(&self) -> Option<&Expr> {
        self.steps.first().map(|s| &s.hypothesis)
    }

    /// Get the final conclusion (last step's conclusion).
    pub fn final_conclusion(&self) -> Option<&Expr> {
        self.steps.last().and_then(|s| s.conclusion.as_ref())
    }

    /// Check if this path is complete (final step has conclusion).
    pub fn is_complete(&self) -> bool {
        self.steps.last().and_then(|s| s.conclusion.as_ref()).is_some()
    }
}

impl std::fmt::Display for InferencePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Path '{}' (conf: {:.2}, {} steps):", 
            self.path_id, self.total_confidence, self.length)?;
        for (i, step) in self.steps.iter().enumerate() {
            write!(f, "  {}. {}", i + 1, step.hypothesis)?;
            if let Some(ref conc) = step.conclusion {
                write!(f, " → {}", conc)?;
            }
            writeln!(f, " [conf: {:.2}]", step.confidence)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Inference Memory
// ---------------------------------------------------------------------------

/// Stores and indexes inference steps for path reconstruction and search.
#[derive(Debug, Clone)]
pub struct InferenceMemory {
    steps: Vec<InferenceStep>,
    /// Maps conclusion ID to the step ID that produced it
    conclusion_map: HashMap<String, String>,
    /// All known paths
    paths: Vec<InferencePath>,
}

impl InferenceMemory {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            conclusion_map: HashMap::new(),
            paths: Vec::new(),
        }
    }

    /// Push a new inference step.
    pub fn push_step(&mut self, step_id: String, hypothesis: Expr, confidence: f64, explored: bool) {
        let step = InferenceStep::new(&step_id, hypothesis, confidence, explored);
        self.steps.push(step);
    }

    /// Record a conclusion for a step.
    pub fn record_conclusion(&mut self, step_id: &str, conclusion: Expr) -> Result<(), String> {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_id == step_id) {
            step.conclusion = Some(conclusion.clone());
            self.conclusion_map.insert(format!("{}", conclusion), step_id.to_string());
            Ok(())
        } else {
            Err(format!("Step '{}' not found", step_id))
        }
    }

    /// Link a step to a parent.
    pub fn link_step(&mut self, step_id: &str, parent_id: &str) -> Result<(), String> {
        if let Some(step) = self.steps.iter_mut().find(|s| s.step_id == step_id) {
            step.parent_id = Some(parent_id.to_string());
            Ok(())
        } else {
            Err(format!("Step '{}' not found", step_id))
        }
    }

    /// Reconstruct the path to a specific conclusion.
    pub fn get_path(&self, conclusion_expr: &Expr) -> Option<InferencePath> {
        let key = format!("{}", conclusion_expr);
        let step_id = self.conclusion_map.get(&key)?;
        self.reconstruct_path(step_id)
    }

    /// Reconstruct a full path from a step back to the root.
    fn reconstruct_path(&self, start_step_id: &str) -> Option<InferencePath> {
        let mut steps_rev = Vec::new();
        let mut current_id = Some(start_step_id.to_string());

        while let Some(ref sid) = current_id {
            if let Some(step) = self.steps.iter().find(|s| s.step_id == *sid) {
                steps_rev.push(step.clone());
                current_id = step.parent_id.clone();
            } else {
                break;
            }
        }

        steps_rev.reverse();
        if steps_rev.is_empty() {
            return None;
        }
        Some(InferencePath::new(steps_rev))
    }

    /// Get all inferred paths.
    pub fn explored_paths(&self) -> &[InferencePath] {
        &self.paths
    }

    /// Record a completed path.
    pub fn record_path(&mut self, path: InferencePath) {
        self.paths.push(path);
    }

    /// Get all steps.
    pub fn steps(&self) -> &[InferenceStep] {
        &self.steps
    }

    /// Get the number of steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Prune steps and paths below a confidence threshold.
    pub fn prune_low_confidence(&mut self, threshold: f64) {
        // Remove low-confidence steps
        self.steps.retain(|s| s.confidence >= threshold);

        // Remove low-confidence paths
        self.paths.retain(|p| p.total_confidence >= threshold);

        // Clean up conclusion map
        let valid_steps: HashSet<String> = self.steps.iter().map(|s| s.step_id.clone()).collect();
        self.conclusion_map.retain(|_, step_id| valid_steps.contains(step_id));
    }

    /// Find the most confident path to any conclusion matching the target expression.
    pub fn find_best_path(&self, target: &str) -> Option<&InferencePath> {
        self.paths.iter()
            .filter(|p| {
                p.final_conclusion()
                    .map(|c| format!("{}", c).contains(target))
                    .unwrap_or(false)
            })
            .max_by(|a, b| a.total_confidence.partial_cmp(&b.total_confidence).unwrap())
    }

    // -----------------------------------------------------------------------
    // A* Search over inference space
    // -----------------------------------------------------------------------

    /// A* search for a target proposition within max_depth steps.
    ///
    /// Uses confidence as the heuristic (higher confidence = closer to goal).
    pub fn heuristic_search(&self, target: &str, max_depth: usize) -> Option<InferencePath> {
        #[derive(Clone)]
        struct SearchNode {
            step_idx: usize,
            path: Vec<usize>,
            confidence: f64,
            heuristic: f64,
        }

        impl PartialEq for SearchNode {
            fn eq(&self, other: &Self) -> bool {
                self.confidence == other.confidence
            }
        }
        impl Eq for SearchNode {}
        impl PartialOrd for SearchNode {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                // Reverse: higher total = better, so we use BinaryHeap as max-heap
                (self.confidence + self.heuristic)
                    .partial_cmp(&(other.confidence + other.heuristic))
            }
        }
        impl Ord for SearchNode {
            fn cmp(&self, other: &Self) -> Ordering {
                self.partial_cmp(other).unwrap_or(Ordering::Equal)
            }
        }

        let mut heap = BinaryHeap::new();
        let mut visited = HashSet::new();

        // Start from each step
        for (idx, step) in self.steps.iter().enumerate() {
            let h = heuristic_match(&step.hypothesis, target);
            heap.push(SearchNode {
                step_idx: idx,
                path: vec![idx],
                confidence: step.confidence,
                heuristic: h,
            });
        }

        while let Some(node) = heap.pop() {
            let step = &self.steps[node.step_idx];

            // Check if this step's hypothesis or conclusion matches the target
            let stmt_str = format!("{}", step.hypothesis);
            if stmt_str.contains(target) {
                let path_steps: Vec<InferenceStep> = node.path.iter()
                    .map(|&i| self.steps[i].clone())
                    .collect();
                return Some(InferencePath::new(path_steps));
            }

            if let Some(ref conc) = step.conclusion {
                let conc_str = format!("{}", conc);
                if conc_str.contains(target) {
                    let path_steps: Vec<InferenceStep> = node.path.iter()
                        .map(|&i| self.steps[i].clone())
                        .collect();
                    return Some(InferencePath::new(path_steps));
                }
            }

            if node.path.len() >= max_depth {
                continue;
            }

            // Expand to connected steps (children)
            for (next_idx, next_step) in self.steps.iter().enumerate() {
                if node.path.contains(&next_idx) { continue; }

                if next_step.parent_id.as_deref() == Some(&step.step_id) {
                    let key = (node.step_idx, next_idx);
                    if visited.insert(key) {
                        let mut new_path = node.path.clone();
                        new_path.push(next_idx);
                        let h = heuristic_match(&next_step.hypothesis, target);
                        heap.push(SearchNode {
                            step_idx: next_idx,
                            path: new_path,
                            confidence: node.confidence * next_step.confidence,
                            heuristic: h,
                        });
                    }
                }
            }
        }

        None
    }
}

/// Simple heuristic: how much does the expression text overlap with the target?
fn heuristic_match(expr: &Expr, target: &str) -> f64 {
    let s = format!("{}", expr);
    if s.contains(target) {
        return 1.0;
    }
    // Count matching words/characters
    let common: usize = target.chars().filter(|c| s.contains(*c)).count();
    if target.is_empty() { return 0.0; }
    common as f64 / target.len() as f64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use scirust_symbolic::parse;

    #[test]
    fn test_inference_step_creation() {
        let expr = parse("x").unwrap();
        let step = InferenceStep::new("step_1", expr, 0.9, true);
        assert_eq!(step.step_id, "step_1");
        assert!((step.confidence - 0.9).abs() < 1e-10);
        assert!(step.explored);
    }

    #[test]
    fn test_inference_path_creation() {
        let steps = vec![
            InferenceStep::new("s1", parse("a").unwrap(), 0.9, true),
            InferenceStep::new("s2", parse("b").unwrap(), 0.8, true)
                .with_conclusion(parse("c").unwrap())
                .with_parent("s1"),
        ];

        let path = InferencePath::new(steps);
        assert_eq!(path.length, 2);
        assert!((path.total_confidence - 0.72).abs() < 1e-10);
        assert!(path.is_complete());
    }

    #[test]
    fn test_memory_push_and_path() {
        let mut memory = InferenceMemory::new();
        memory.push_step("s1".to_string(), parse("a").unwrap(), 0.9, true);
        memory.push_step("s2".to_string(), parse("b").unwrap(), 0.8, true);
        memory.record_conclusion("s2", parse("c").unwrap()).unwrap();
        memory.link_step("s2", "s1").unwrap();

        let path = memory.get_path(&parse("c").unwrap());
        assert!(path.is_some());
        assert_eq!(path.unwrap().length, 2);
    }

    #[test]
    fn test_memory_pruning() {
        let mut memory = InferenceMemory::new();
        memory.push_step("s1".to_string(), parse("a").unwrap(), 0.9, true);
        memory.push_step("s2".to_string(), parse("b").unwrap(), 0.1, true);

        memory.prune_low_confidence(0.5);
        assert_eq!(memory.len(), 1);
    }

    #[test]
    fn test_heuristic_search() {
        let mut memory = InferenceMemory::new();
        memory.push_step("s1".to_string(), parse("a").unwrap(), 0.9, true);
        memory.push_step("s2".to_string(), parse("b").unwrap(), 0.8, true);
        memory.record_conclusion("s2", parse("c").unwrap()).unwrap();
        memory.link_step("s2", "s1").unwrap();
        memory.push_step("s3".to_string(), parse("d").unwrap(), 0.7, true);

        // Search for 'c' in the inference space
        let result = memory.heuristic_search("c", 5);
        assert!(result.is_some());
    }

    #[test]
    fn test_find_best_path() {
        let mut memory = InferenceMemory::new();

        // Path 1: low confidence
        let steps1 = vec![
            InferenceStep::new("p1_s1", parse("a").unwrap(), 0.5, true),
            InferenceStep::new("p1_s2", parse("b").unwrap(), 0.5, true)
                .with_conclusion(parse("result").unwrap()),
        ];
        memory.record_path(InferencePath::new(steps1));

        // Path 2: high confidence
        let steps2 = vec![
            InferenceStep::new("p2_s1", parse("x").unwrap(), 0.9, true),
            InferenceStep::new("p2_s2", parse("y").unwrap(), 0.9, true)
                .with_conclusion(parse("result").unwrap()),
        ];
        memory.record_path(InferencePath::new(steps2));

        let best = memory.find_best_path("result");
        assert!(best.is_some());
        assert!((best.unwrap().total_confidence - 0.81).abs() < 1e-10);
    }
}
