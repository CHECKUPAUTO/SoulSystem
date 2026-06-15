//! Tree Attention Mask — causal attention mask for simultaneous evaluation
//! of multiple speculative token paths in a single forward pass.
//!
//! In MTP (Multi-Token Prediction), the main model must evaluate N speculative
//! continuations simultaneously. The Tree Attention mask ensures each speculative
//! token only attends to its ancestors and all prefix tokens, maintaining causality.
//!
//! # Mask Structure
//! ```text
//!          prefix[0..P]  |  tree[0..T]
//!         ---------------+---------------
//! prefix   causal        |  0
//! tree     1 (all ones)  |  ancestor-mask
//! ```
//! `ancestor_mask[i][j] = 1.0` iff j is an ancestor of i or j == i in the tree.

use nalgebra::DMatrix;

// ── MtpTreeAttention ──────────────────────────────────────────────────────────

/// Builds and stores a 2D causal tree attention mask for speculative decoding.
///
/// The tree is a full K-ary tree: root + K nodes at level 1 + K² at level 2 + ...
/// Total nodes = (K^(num_steps+1) - 1) / (K - 1).
///
/// Each tree node can only attend to:
/// 1. All prefix tokens (full causal attention to the past),
/// 2. Its ancestors in the speculation tree,
/// 3. Itself.
///
/// The mask is stored as a flat `DMatrix<f32>` for direct use with matrix ops.
pub struct MtpTreeAttention {
    /// Number of speculative steps (depth excluding root).
    pub num_steps: usize,
    /// Branching factor: number of candidates per speculative step.
    pub branch_factor: usize,
    /// Total nodes in the speculation tree (root + speculative nodes).
    pub total_nodes: usize,
    /// Parent index for each tree node: `parent[i]` points to node i's parent.
    /// Root has `parent[0] = -1` (i.e., no parent).
    pub parent: Vec<isize>,
    /// Depth level of each node (root = 0, first speculations = 1, ...).
    pub level: Vec<usize>,
    /// Causal tree attention mask [total_nodes × total_nodes].
    /// `mask[(node, ancestor)] = 1.0` iff ancestor is reachable from node
    /// by following parent links (ancestor == node included).
    pub mask: DMatrix<f32>,
}

impl MtpTreeAttention {
    /// Create a tree attention structure.
    ///
    /// Builds a full K-ary tree: root (level 0) with K children (level 1),
    /// each with K children (level 2), etc. — total `num_steps` speculative
    /// levels beyond the root.
    ///
    /// # Panics
    /// Panics if `num_steps == 0` or `branch_factor == 0`.
    pub fn new(num_steps: usize, branch_factor: usize) -> Self {
        assert!(num_steps > 0, "num_steps must be > 0");
        assert!(branch_factor > 0, "branch_factor must be > 0");

        let total_nodes = Self::total_nodes_for(num_steps, branch_factor);
        let (parent, level) = Self::build_tree(num_steps, branch_factor, total_nodes);
        let mask = Self::build_mask(&parent, total_nodes);

        Self {
            num_steps,
            branch_factor,
            total_nodes,
            parent,
            level,
            mask,
        }
    }

    /// Compute total nodes: 1 + K + K² + ... + K^depth.
    #[inline]
    fn total_nodes_for(depth: usize, factor: usize) -> usize {
        if factor == 1 {
            return depth + 1;
        }
        let mut pow = 1usize;
        let mut sum = 0usize;
        for _ in 0..=depth {
            sum += pow;
            pow = pow.saturating_mul(factor);
        }
        sum
    }

    /// Build parent links and depth levels for a full K-ary tree.
    /// Node 0 is the root.
    fn build_tree(depth: usize, factor: usize, total: usize) -> (Vec<isize>, Vec<usize>) {
        let mut parent = vec![-1isize; total];
        let mut level = vec![0usize; total];

        // Breadth-first assignment: node i's children start at i*K + 1
        let mut next_child = 1usize;
        for node in 0..total {
            let lev = level[node];
            if lev < depth && next_child < total {
                let end = (next_child + factor).min(total);
                for child in next_child..end {
                    parent[child] = node as isize;
                    level[child] = lev + 1;
                }
                next_child = end;
            }
        }

        (parent, level)
    }

    /// Build the causal tree attention mask.
    ///
    /// For each node i, walk its ancestor chain upwards (including itself)
    /// and set `mask[(i, ancestor)] = 1.0`.
    fn build_mask(parent: &[isize], total: usize) -> DMatrix<f32> {
        let mut mask = DMatrix::zeros(total, total);

        for i in 0..total {
            // Self-attention
            mask[(i, i)] = 1.0;

            // Walk ancestors (parent chain to root)
            let mut cur = parent[i];
            while cur >= 0 {
                mask[(i, cur as usize)] = 1.0;
                cur = parent[cur as usize];
            }
        }

        mask
    }

    /// Number of nodes at each depth level [0 .. num_steps].
    /// Useful for batching by step.
    pub fn nodes_per_level(&self) -> Vec<usize> {
        let mut counts = vec![0usize; self.num_steps + 1];
        for &lev in &self.level {
            counts[lev] += 1;
        }
        counts
    }

    /// Flat indices of all nodes at the given depth level.
    pub fn nodes_at_level(&self, depth: usize) -> Vec<usize> {
        self.level
            .iter()
            .enumerate()
            .filter(|(_, &l)| l == depth)
            .map(|(i, _)| i)
            .collect()
    }

    /// Build the full attention mask for a sequence of `prefix_len` prefix
    /// tokens followed by the tree nodes.
    ///
    /// Returns a [total_seq × total_seq] mask where `total_seq = prefix_len + total_nodes`.
    ///
    /// - Prefix rows (0..prefix_len): standard causal (lower triangular), zero
    ///   attention to future tree nodes.
    /// - Tree rows: full attention to ali prefix tokens + tree-structured mask.
    pub fn full_attention_mask(&self, prefix_len: usize) -> DMatrix<f32> {
        let total = prefix_len + self.total_nodes;
        let mut full = DMatrix::zeros(total, total);

        // Prefix section: standard causal (lower triangular)
        for i in 0..prefix_len {
            for j in 0..=i {
                full[(i, j)] = 1.0;
            }
        }

        // Tree nodes: full attention to prefix
        let t0 = prefix_len;
        for ti in 0..self.total_nodes {
            let row = t0 + ti;
            for pj in 0..prefix_len {
                full[(row, pj)] = 1.0;
            }
        }

        // Tree section: copy the tree attention block
        for ti in 0..self.total_nodes {
            let row = t0 + ti;
            for tj in 0..self.total_nodes {
                full[(row, t0 + tj)] = self.mask[(ti, tj)];
            }
        }

        full
    }

    /// Builds a compact tree-only mask augmented with prefix attention.
    ///
    /// Returns [total_nodes × (prefix_len + total_nodes)].
    /// Each tree-node row has 1.0 for all prefix positions and 1.0 for its
    /// ancestors in the tree — suitable for direct attention computation with
    /// a [seq_q × seq_k] layout where q = tree nodes, k = prefix + tree.
    pub fn attendable_mask(&self, prefix_len: usize) -> DMatrix<f32> {
        let k_len = prefix_len + self.total_nodes;
        let mut mask = DMatrix::zeros(self.total_nodes, k_len);

        for ti in 0..self.total_nodes {
            // Full attention to prefix
            for pj in 0..prefix_len {
                mask[(ti, pj)] = 1.0;
            }
            // Tree-structured attention
            for tj in 0..self.total_nodes {
                mask[(ti, prefix_len + tj)] = self.mask[(ti, tj)];
            }
        }

        mask
    }

    /// Path from the root to the given tree node (including the node itself).
    /// Returns indices in order from root → ... → node.
    pub fn path_to_root(&self, node_idx: usize) -> Vec<usize> {
        let mut path = Vec::with_capacity(self.num_steps + 1);
        let mut cur = node_idx as isize;
        while cur >= 0 {
            path.push(cur as usize);
            cur = self.parent[cur as usize];
        }
        path.reverse();
        path
    }

    /// Check whether `ancestor` is an ancestor of `node` in the speculation tree.
    #[inline]
    pub fn is_ancestor(&self, ancestor: usize, node: usize) -> bool {
        self.mask[(node, ancestor)] > 0.5
    }

    /// Number of descendants for a given tree node (including itself).
    pub fn subtree_size(&self, node_idx: usize) -> usize {
        let node_level = self.level[node_idx];
        if node_level == self.num_steps {
            return 1; // leaf
        }
        let mut count = 1usize;
        for i in node_idx + 1..self.total_nodes {
            if self.is_ancestor(node_idx, i) {
                count += 1;
            }
        }
        count
    }

    /// Flatten the tree into a depth-first token sequence for a given path.
    ///
    /// Given a leaf node, returns all nodes along the path from root to leaf
    /// (the tokens along that speculative branch).
    pub fn branch_tokens(&self, leaf_idx: usize) -> Vec<usize> {
        self.path_to_root(leaf_idx)
    }

    /// Iterate all leaf nodes (nodes at depth == num_steps).
    pub fn leaves(&self) -> Vec<usize> {
        self.level
            .iter()
            .enumerate()
            .filter(|(_, &l)| l == self.num_steps)
            .map(|(i, _)| i)
            .collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_sizes_match_formula() {
        for (depth, factor, expected) in [
            (1, 2, 3), // root + 2
            (2, 2, 7), // root + 2 + 4
            (1, 3, 4), // root + 3
            (3, 1, 4), // root + 1 + 1 + 1
            (3, 2, 15),
        ] {
            let t = MtpTreeAttention::new(depth, factor);
            assert_eq!(
                t.total_nodes, expected,
                "depth={depth}, factor={factor}, expected={expected}, got={}",
                t.total_nodes
            );
        }
    }

    #[test]
    fn root_has_no_parent() {
        let t = MtpTreeAttention::new(2, 2);
        assert_eq!(t.parent[0], -1);
        assert_eq!(t.level[0], 0);
    }

    #[test]
    fn level_1_nodes_have_root_as_parent() {
        let t = MtpTreeAttention::new(2, 3);
        for i in 1..=3 {
            assert_eq!(t.parent[i], 0, "node {i} should have parent 0");
            assert_eq!(t.level[i], 1, "node {i} should be at level 1");
        }
    }

    #[test]
    fn mask_is_causal_tree() {
        let t = MtpTreeAttention::new(2, 2);
        // root (0) can only attend to itself
        assert_eq!(t.mask[(0, 0)], 1.0);
        assert_eq!(t.mask[(0, 1)], 0.0);
        // node 1 (level 1, child of 0) attends to 0 and 1
        assert_eq!(t.mask[(1, 0)], 1.0);
        assert_eq!(t.mask[(1, 1)], 1.0);
        assert_eq!(t.mask[(1, 2)], 0.0); // sibling, not ancestor
                                         // node 3 (level 2, child of 1) attends to 0, 1, 3
        assert_eq!(t.mask[(3, 0)], 1.0);
        assert_eq!(t.mask[(3, 1)], 1.0);
        assert_eq!(t.mask[(3, 2)], 0.0); // uncle, not ancestor
        assert_eq!(t.mask[(3, 3)], 1.0);
    }

    #[test]
    fn nodes_per_level() {
        let t = MtpTreeAttention::new(3, 2);
        let counts = t.nodes_per_level();
        assert_eq!(counts, vec![1, 2, 4, 8]);
    }

    #[test]
    fn leaves_count_matches_branch_factor_power() {
        let t = MtpTreeAttention::new(3, 2);
        assert_eq!(t.leaves().len(), 8); // 2^3
        let t2 = MtpTreeAttention::new(2, 3);
        assert_eq!(t2.leaves().len(), 9); // 3^2
    }

    #[test]
    fn path_to_root_is_correct() {
        let t = MtpTreeAttention::new(2, 2);
        // Tree: 0 -> {1, 2} ; 1 -> {3, 4} ; 2 -> {5, 6}
        assert_eq!(t.path_to_root(0), vec![0]);
        assert_eq!(t.path_to_root(1), vec![0, 1]);
        assert_eq!(t.path_to_root(4), vec![0, 1, 4]);
        assert_eq!(t.path_to_root(6), vec![0, 2, 6]);
    }

    #[test]
    fn is_ancestor_checks() {
        let t = MtpTreeAttention::new(2, 2);
        assert!(t.is_ancestor(0, 0));
        assert!(t.is_ancestor(0, 1));
        assert!(t.is_ancestor(0, 4));
        assert!(t.is_ancestor(1, 3));
        assert!(!t.is_ancestor(1, 2)); // sibling
        assert!(!t.is_ancestor(4, 0)); // reversed
        assert!(!t.is_ancestor(1, 6)); // different branch
    }

    #[test]
    fn full_attention_mask_dims() {
        let t = MtpTreeAttention::new(2, 2);
        let mask = t.full_attention_mask(4);
        assert_eq!(mask.nrows(), 4 + 7); // prefix + 7 tree nodes
        assert_eq!(mask.ncols(), 4 + 7);
    }

    #[test]
    fn full_attention_mask_prefix_is_causal() {
        let t = MtpTreeAttention::new(1, 2);
        let mask = t.full_attention_mask(3);
        // prefix[0] attends to [0]
        assert_eq!(mask[(0, 0)], 1.0);
        assert_eq!(mask[(0, 1)], 0.0);
        // prefix[2] attends to [0, 1, 2]
        assert_eq!(mask[(2, 0)], 1.0);
        assert_eq!(mask[(2, 1)], 1.0);
        assert_eq!(mask[(2, 2)], 1.0);
        assert_eq!(mask[(2, 3)], 0.0); // no future access into tree
    }

    #[test]
    fn attendable_mask_shape() {
        let t = MtpTreeAttention::new(2, 2);
        let mask = t.attendable_mask(5);
        assert_eq!(mask.nrows(), 7);
        assert_eq!(mask.ncols(), 5 + 7);
    }

    #[test]
    fn subtree_sizes() {
        let t = MtpTreeAttention::new(2, 2);
        // root subtree = all 7 nodes
        assert_eq!(t.subtree_size(0), 7);
        // level-1 node subtree: itself + its 2 level-2 children = 3
        assert_eq!(t.subtree_size(1), 3);
        // level-2 node = leaf = 1
        assert_eq!(t.subtree_size(4), 1);
    }

    #[test]
    #[should_panic(expected = "num_steps must be > 0")]
    fn zero_steps_panics() {
        MtpTreeAttention::new(0, 2);
    }

    #[test]
    #[should_panic(expected = "branch_factor must be > 0")]
    fn zero_branch_factor_panics() {
        MtpTreeAttention::new(2, 0);
    }
}
