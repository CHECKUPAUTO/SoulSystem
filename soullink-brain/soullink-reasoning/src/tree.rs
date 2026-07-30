//! ThoughtTree — Tree of Thoughts with eval-based pruning.
//!
//! Explores multiple reasoning paths, evaluates each with soullink-eval
//! (cosine similarity between query and thought embedding), and prunes
//! low-scoring branches to focus computation.

use crate::node::{NodeStatus, ThoughtNode};
use soullink_eval::loss::SemanticLoss;
use std::collections::HashMap;

/// Configuration for the thought tree.
#[derive(Debug, Clone)]
pub struct TreeConfig {
    /// Maximum depth of the tree.
    pub max_depth: usize,
    /// Maximum branching factor (children per node).
    pub max_branches: usize,
    /// Minimum score to accept a node (0.0–1.0).
    pub accept_threshold: f32,
    /// Learning rate for eval-based scoring.
    pub learning_rate: f32,
    /// Keep top-K nodes at each level (prune the rest).
    pub top_k: usize,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_branches: 3,
            accept_threshold: 0.5,
            learning_rate: 0.05,
            top_k: 3,
        }
    }
}

/// The Tree of Thoughts.
pub struct ThoughtTree {
    nodes: HashMap<usize, ThoughtNode>,
    next_id: usize,
    config: TreeConfig,
    loss: SemanticLoss,
}

impl ThoughtTree {
    pub fn new(config: TreeConfig) -> Self {
        Self {
            nodes: HashMap::new(),
            next_id: 0,
            loss: SemanticLoss::new(config.learning_rate),
            config,
        }
    }

    /// Add a root node (the original query/problem).
    pub fn add_root(&mut self, content: impl Into<String>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let node = ThoughtNode::root(id, content);
        self.nodes.insert(id, node);
        id
    }

    /// Add a child thought to a parent node.
    pub fn add_child(&mut self, parent_id: usize, content: impl Into<String>) -> Option<usize> {
        let parent_depth = self.nodes.get(&parent_id)?.depth;
        if parent_depth >= self.config.max_depth {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        let node = ThoughtNode::new(id, content, Some(parent_id), parent_depth + 1);
        self.nodes.insert(id, node);
        self.nodes.get_mut(&parent_id)?.children.push(id);
        Some(id)
    }

    /// Whether an embedding carries enough variation to be scored against.
    ///
    /// A constant vector — every element identical, `vec![1.0; 64]` being the
    /// case that motivated this — has cosine similarity 1.0 against any other
    /// constant vector, whatever the underlying text. Feeding two of them to
    /// `evaluate_node` produced a perfect score for every node in the tree,
    /// so nothing was ever pruned and `best_path` chose arbitrarily among
    /// equals, while the scores still looked like semantic judgements.
    ///
    /// An empty vector is likewise unscoreable.
    fn embedding_is_informative(emb: &[f32]) -> bool {
        let Some(first) = emb.first() else {
            return false;
        };
        emb.iter().any(|v| (v - first).abs() > f32::EPSILON)
    }

    /// Evaluate a node using semantic loss against a reference embedding.
    ///
    /// Returns `None` when the node does not exist **or** when either
    /// embedding carries no information; in the latter case the node is left
    /// [`NodeStatus::Unscored`] rather than being given a fabricated score.
    /// Callers that need to tell the two apart should check the node's status.
    pub fn evaluate_node(
        &mut self,
        node_id: usize,
        query_emb: &[f32],
        thought_emb: &[f32],
    ) -> Option<f32> {
        let scoreable = Self::embedding_is_informative(query_emb)
            && Self::embedding_is_informative(thought_emb);
        // Computed before borrowing `self.nodes` mutably.
        let result = scoreable.then(|| self.loss.compute(query_emb, thought_emb));
        let accept_threshold = self.config.accept_threshold;

        let node = self.nodes.get_mut(&node_id)?;

        if !scoreable {
            // Refusing to score is the point. Returning 1.0 here — which is
            // what cosine similarity of two constant vectors gives — would
            // accept every node and report a confident number for a
            // comparison that never happened.
            node.status = NodeStatus::Unscored;
            node.score = 0.0;
            node.loss = f32::NAN;
            return None;
        }

        let result = result.expect("scoreable implies a computed result");
        node.score = result.similarity.max(0.0);
        node.loss = result.loss;

        if node.score >= accept_threshold {
            node.status = NodeStatus::Accepted;
        } else {
            node.status = NodeStatus::Pruned;
        }

        Some(node.score)
    }

    /// Record that a node could not be semantically evaluated.
    ///
    /// For callers that have no embedding provider wired. The alternative —
    /// passing a placeholder vector to [`Self::evaluate_node`] — is what
    /// MED-004 was: it produced a perfect score for every node and made
    /// pruning a no-op while still reporting numbers that read as semantic
    /// judgements.
    ///
    /// Returns `false` if the node does not exist.
    pub fn mark_unscored(&mut self, node_id: usize) -> bool {
        match self.nodes.get_mut(&node_id) {
            Some(node) => {
                node.status = NodeStatus::Unscored;
                node.score = 0.0;
                node.loss = f32::NAN;
                true
            }
            None => false,
        }
    }

    /// Get the best path from root to a leaf (highest cumulative score).
    ///
    /// Only `Accepted` leaves are eligible, so a tree whose nodes are all
    /// [`NodeStatus::Unscored`] yields an empty path rather than an arbitrary
    /// one. That emptiness is the correct answer when no semantic judgement
    /// was possible: there is no basis on which to call any path best.
    pub fn best_path(&self) -> Vec<&ThoughtNode> {
        let mut best_path = Vec::new();
        let mut best_score = f32::NEG_INFINITY;

        for (&id, node) in &self.nodes {
            if node.is_leaf() && node.status == NodeStatus::Accepted {
                let path = self.path_to_root(id);
                let score: f32 =
                    path.iter().map(|n| n.score).sum::<f32>() / path.len().max(1) as f32;
                if score > best_score {
                    best_score = score;
                    best_path = path;
                }
            }
        }

        best_path
    }

    /// Trace path from a node back to the root.
    fn path_to_root(&self, start_id: usize) -> Vec<&ThoughtNode> {
        let mut path = Vec::new();
        let mut current = Some(start_id);

        while let Some(id) = current {
            if let Some(node) = self.nodes.get(&id) {
                path.push(node);
                current = node.parent_id;
            } else {
                break;
            }
        }

        path.reverse();
        path
    }

    /// Prune nodes below the accept threshold.
    /// `Unscored` nodes deliberately survive: discarding a thought because no
    /// semantic judgement was possible would be the same error as accepting
    /// it, just inverted. Absence of evidence is not evidence of low quality.
    pub fn prune(&mut self) -> usize {
        let to_prune: Vec<usize> = self
            .nodes
            .iter()
            .filter(|(_, n)| {
                n.status == NodeStatus::Pending
                    || (n.status == NodeStatus::Accepted && n.score < self.config.accept_threshold)
            })
            .map(|(id, _)| *id)
            .collect();

        let count = to_prune.len();
        for id in to_prune {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.prune();
            }
        }
        count
    }

    /// Get a node by ID.
    pub fn get(&self, id: usize) -> Option<&ThoughtNode> {
        self.nodes.get(&id)
    }

    /// Total number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Number of accepted nodes.
    pub fn accepted_count(&self) -> usize {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Accepted)
            .count()
    }

    /// Number of pruned nodes.
    pub fn pruned_count(&self) -> usize {
        self.nodes
            .values()
            .filter(|n| n.status == NodeStatus::Pruned)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree() -> ThoughtTree {
        ThoughtTree::new(TreeConfig {
            max_depth: 3,
            max_branches: 3,
            accept_threshold: 0.3,
            learning_rate: 0.05,
            top_k: 3,
        })
    }

    #[test]
    fn add_root_and_children() {
        let mut tree = make_tree();
        let root = tree.add_root("What is Rust?");
        let c1 = tree.add_child(root, "A systems language").unwrap();
        let c2 = tree.add_child(root, "A ferrous metal").unwrap();
        assert_eq!(tree.len(), 3);
        assert_eq!(tree.get(root).unwrap().children.len(), 2);
        assert_eq!(tree.get(c1).unwrap().depth, 1);
        assert_eq!(tree.get(c2).unwrap().depth, 1);
    }

    #[test]
    fn max_depth_respected() {
        let mut tree = make_tree();
        let root = tree.add_root("root");
        let c1 = tree.add_child(root, "depth 1").unwrap();
        let c2 = tree.add_child(c1, "depth 2").unwrap();
        let c3 = tree.add_child(c2, "depth 3").unwrap();
        // depth 4 > max_depth 3
        let c4 = tree.add_child(c3, "depth 4");
        assert!(c4.is_none());
    }

    #[test]
    fn evaluate_accepts_high_score() {
        let mut tree = make_tree();
        let root = tree.add_root("What is Rust?");
        let child = tree
            .add_child(root, "A systems programming language")
            .unwrap();

        // Identical *informative* vectors. A constant vector like
        // `vec![1.0; 64]` is now refused outright — it scores 1.0 against any
        // other constant vector regardless of content, which is what MED-004
        // was — so the "identical means similar" intent is expressed with a
        // vector that actually carries information.
        let query: Vec<f32> = (0..64).map(|i| i as f32 * 0.1).collect();
        let thought = query.clone();

        let score = tree.evaluate_node(child, &query, &thought).unwrap();
        assert!(
            score > 0.9,
            "identical vectors should have high score, got {}",
            score
        );
        assert_eq!(tree.get(child).unwrap().status, NodeStatus::Accepted);
    }

    #[test]
    fn evaluate_prunes_low_score() {
        let mut tree = make_tree();
        let root = tree.add_root("What is Rust?");
        let child = tree.add_child(root, "A ferrous metal").unwrap();

        // Orthogonal vectors = low similarity
        let mut query = vec![0.0f32; 64];
        query[0] = 1.0;
        let mut thought = vec![0.0f32; 64];
        thought[1] = 1.0;

        let score = tree.evaluate_node(child, &query, &thought).unwrap();
        assert!(
            score < 0.3,
            "orthogonal vectors should have low score, got {}",
            score
        );
        assert_eq!(tree.get(child).unwrap().status, NodeStatus::Pruned);
    }

    /// The exact placeholder MED-004 was built on is refused.
    #[test]
    fn a_constant_embedding_is_refused_rather_than_scored() {
        let mut tree = make_tree();
        let root = tree.add_root("What is Rust?");
        let child = tree.add_child(root, "A systems language").unwrap();

        // The literal placeholder from soul-agent-core.
        let query = vec![1.0_f32; 64];
        let thought = vec![1.0_f32; 64];

        let score = tree.evaluate_node(child, &query, &thought);
        assert!(
            score.is_none(),
            "a constant embedding must not yield a score; cosine similarity of \
             two constant vectors is 1.0 whatever the underlying text"
        );
        assert_eq!(tree.get(child).unwrap().status, NodeStatus::Unscored);
        assert_eq!(tree.get(child).unwrap().score, 0.0);
    }

    /// Zero vectors are unscoreable too, and an empty one cannot be scored.
    #[test]
    fn degenerate_embeddings_are_all_refused() {
        let mut tree = make_tree();
        let root = tree.add_root("q");
        let child = tree.add_child(root, "a").unwrap();
        let informative: Vec<f32> = (0..8).map(|i| i as f32).collect();

        assert!(tree
            .evaluate_node(child, &[0.0; 8], &informative)
            .is_none());
        assert!(tree
            .evaluate_node(child, &informative, &[0.0; 8])
            .is_none());
        assert!(tree.evaluate_node(child, &[], &informative).is_none());
    }

    /// With nothing scored, no path is claimed to be best.
    #[test]
    fn an_unscored_tree_yields_no_best_path() {
        let mut tree = make_tree();
        let root = tree.add_root("question");
        let a = tree.add_child(root, "one").unwrap();
        let b = tree.add_child(root, "two").unwrap();

        assert!(tree.mark_unscored(a));
        assert!(tree.mark_unscored(b));

        assert!(
            tree.best_path().is_empty(),
            "with no semantic judgement there is no basis for calling any path \
             best; an arbitrary pick presented as best is the MED-004 failure"
        );
    }

    /// Unscored nodes survive pruning.
    #[test]
    fn pruning_does_not_discard_unscored_nodes() {
        let mut tree = make_tree();
        let root = tree.add_root("question");
        let child = tree.add_child(root, "thought").unwrap();
        tree.mark_unscored(child);

        tree.prune();

        assert_eq!(
            tree.get(child).unwrap().status,
            NodeStatus::Unscored,
            "absence of evidence is not evidence of low quality"
        );
    }

    #[test]
    fn best_path_returns_highest_scoring() {
        let mut tree = make_tree();
        let root = tree.add_root("question");
        let good = tree.add_child(root, "good answer").unwrap();
        let bad = tree.add_child(root, "bad answer").unwrap();

        // Informative rather than constant; see `evaluate_accepts_high_score`.
        let emb: Vec<f32> = (0..64).map(|i| i as f32 * 0.1).collect();
        tree.evaluate_node(good, &emb, &emb); // perfect match
        let mut orth_a = vec![0.0f32; 64];
        orth_a[0] = 1.0;
        let mut orth_b = vec![0.0f32; 64];
        orth_b[1] = 1.0;
        tree.evaluate_node(bad, &orth_a, &orth_b); // orthogonal

        let path = tree.best_path();
        assert!(!path.is_empty());
        // Best path should include the good node
        assert!(path.iter().any(|n| n.id == good));
    }
}
