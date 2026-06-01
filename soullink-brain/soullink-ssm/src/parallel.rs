//! Parallel (associative) scan for SSM training efficiency.
//!
//! The standard SSM recurrence is sequential:
//!   h_t = Ā · h_{t-1} + B̄ · x_t
//!
//! The parallel (prefix) scan reformulates this as an associative
//! operator, enabling O(L log L) computation instead of O(L²).
//!
//! For SSMs with input-dependent parameters (Mamba), the parallel scan
//! is crucial for training efficiency on modern hardware.

/// A binary associative operator element for the parallel scan.
///
/// For each timestep t, we define a tuple (A_t, B_t) representing
/// the linear transformation:
///   h_t = A_t · h_{t-1} + B_t
///
/// The associative composition of (A_2, B_2) ∘ (A_1, B_1) is:
///   h_2 = A_2 · (A_1 · h_0 + B_1) + B_2
///       = (A_2 · A_1) · h_0 + (A_2 · B_1 + B_2)
///
/// So: (A_2, B_2) ∘ (A_1, B_1) = (A_2·A_1, A_2·B_1 + B_2)
#[derive(Debug, Clone)]
pub struct ScanElement {
    /// Linear part (state transition multiplier)
    pub a: f64,
    /// Additive part (input contribution)
    pub b: f64,
}

impl ScanElement {
    /// Create a new scan element.
    pub fn new(a: f64, b: f64) -> Self {
        Self { a, b }
    }

    /// Binary associative composition.
    ///
    /// `self` is the *later* element (t=2), `other` is the *earlier* (t=1).
    /// The combined element gives the net transformation from h_0 to h_2.
    pub fn compose(&self, other: &ScanElement) -> ScanElement {
        ScanElement {
            a: self.a * other.a,
            b: self.a * other.b + self.b,
        }
    }

    /// Identity element: h_t = h_{t-1} (identity transition, no input)
    pub fn identity() -> Self {
        ScanElement { a: 1.0, b: 0.0 }
    }
}

/// Parallel (associative) prefix scan for SSM recurrence.
///
/// Uses a standard binary-tree reduction that runs in O(L log L)
/// and is parallelizable across the sequence dimension.
pub struct ParallelScan;

impl ParallelScan {
    /// Scan over a sequence of scalar (Ā, B̄) values.
    ///
    /// Equivalent to the recurrence: h_t = Ā_t · h_{t-1} + B̄_t
    ///
    /// # Arguments
    ///
    /// * `elements` - Sequence of (Ā_t, B̄_t) pairs, one per timestep
    /// * `h0` - Initial state h_{0}
    ///
    /// # Returns
    ///
    /// The state sequence h_1, ..., h_L
    pub fn scan(elements: &[ScanElement], h0: f64) -> Vec<f64> {
        let l = elements.len();
        if l == 0 {
            return vec![];
        }

        // Convert to prefix scan: compute h_t relative to h0
        // We need the cumulative composition from t=1 to t
        let mut h = Vec::with_capacity(l);

        let mut running_a: f64 = 1.0;
        let mut running_b: f64 = 0.0;

        for elem in elements {
            // Compose: (new_a, new_b) = (Ā_t, B̄_t) ∘ (running_a, running_b)
            running_a = elem.a * running_a;
            running_b = elem.a * running_b + elem.b;

            // h_t = running_a · h_0 + running_b
            h.push(running_a * h0 + running_b);
        }

        h
    }

    /// Parallel scan implementation using binary tree reduction (Blelloch).
    ///
    /// Phase 1 (up-sweep): Compute partial compositions up the tree.
    /// Phase 2 (down-sweep): Propagate prefixes to compute all inclusive prefixes.
    ///
    /// The output `result[i]` = elements[i] ∘ elements[i-1] ∘ ... ∘ elements[0].
    ///
    /// # Arguments
    ///
    /// * `elements` - Sequence of (Ā_t, B̄_t) pairs
    ///
    /// # Returns
    ///
    /// Inclusive prefix composition of all elements
    pub fn parallel_scan(elements: &[ScanElement]) -> Vec<ScanElement> {
        let l = elements.len();
        if l == 0 {
            return vec![];
        }
        if l == 1 {
            return vec![elements[0].clone()];
        }

        // Ensure power-of-two length for binary tree structure
        let next_pow2 = l.next_power_of_two();
        let mut tree: Vec<ScanElement> = elements.to_vec();
        tree.resize(next_pow2, ScanElement::identity());

        // Phase 1: Up-sweep (compute partial reductions)
        // At level d, stride = 2^(d+1), combine pairs of stride/2
        let mut step = 1;
        while step < next_pow2 {
            let stride = step * 2;
            for i in (0..next_pow2).step_by(stride) {
                // right segment ∘ left segment → rightmost position
                tree[i + stride - 1] = tree[i + stride - 1].compose(&tree[i + step - 1]);
            }
            step = stride;
        }

        // Phase 2: Down-sweep (compute exclusive prefixes)
        // tree[last] = identity converts total sum to exclusive prefix before last element
        tree[next_pow2 - 1] = ScanElement::identity();
        step = next_pow2 / 2;
        while step > 0 {
            let stride = step * 2;
            for i in (0..next_pow2).step_by(stride) {
                // For segment [i, i+stride):
                //   tree[i+step-1] = composition of left half [i, i+step)
                //   tree[i+stride-1] = exclusive prefix before this segment (from parent)
                let parent = tree[i + stride - 1].clone();
                let left_comp = tree[i + step - 1].clone();
                // Left child gets exclusive prefix before this segment
                tree[i + step - 1] = parent.clone();
                // Right child gets left_composition ∘ parent = exclusive prefix before right segment
                tree[i + stride - 1] = left_comp.compose(&parent);
            }
            step /= 2;
        }

        // Now tree holds EXCLUSIVE prefixes: tree[i] = composition of elements[0..i)
        // Convert to INCLUSIVE: result[i] = elements[i] ∘ tree[i]
        let mut result = Vec::with_capacity(l);
        for i in 0..l {
            result.push(elements[i].compose(&tree[i]));
        }
        result
    }

    /// Compute the prefix sums for a 1D sequence using parallel scan.
    ///
    /// Simple exclusive prefix sum: output[t] = sum_{i < t} input[i]
    /// This demonstrates the parallel scan pattern.
    ///
    /// # Arguments
    ///
    /// * `input` - Sequence of values
    ///
    /// # Returns
    ///
    /// Prefix sums (first element is 0.0)
    pub fn prefix_sum(input: &[f64]) -> Vec<f64> {
        let l = input.len();
        if l == 0 {
            return vec![];
        }

        // Convert to scan elements: A_t = 1, B_t = input[t]
        // So h_t = 1 · h_{t-1} + input[t]  ⇒  prefix sum
        let elements: Vec<ScanElement> = input.iter().map(|&x| ScanElement::new(1.0, x)).collect();

        let prefixes = Self::parallel_scan(&elements);

        // Extract the B component of each prefix element
        let mut result = Vec::with_capacity(l);
        for p in &prefixes {
            result.push(p.b);
        }

        result
    }

    /// Perform a selective parallel scan.
    ///
    /// This is the Mamba-style scan where A_t and B_t are
    /// input-dependent: B_t already incorporates x_t.
    ///
    /// # Arguments
    ///
    /// * `a_seq` - Transition coefficients Ā_t (L,)
    /// * `b_seq` - Input contributions B̄_t (L,)
    /// * `h0` - Initial state
    ///
    /// # Returns
    ///
    /// State sequence h_1, ..., h_L
    pub fn selective_scan_1d(a_seq: &[f64], b_seq: &[f64], h0: f64) -> Vec<f64> {
        assert_eq!(
            a_seq.len(),
            b_seq.len(),
            "a_seq and b_seq must have same length"
        );

        let elements: Vec<ScanElement> = a_seq
            .iter()
            .zip(b_seq.iter())
            .map(|(&a, &b)| ScanElement::new(a, b))
            .collect();

        Self::scan(&elements, h0)
    }
}

/// Multi-dimensional parallel scan for SSM state updates.
///
/// Handles the full state vector h ∈ R^N where we have
/// (Ā_t, B̄_t) for each dimension.
use ndarray::{Array1, Array2};

/// A binary associative operator element for multi-dimensional parallel scan.
///
/// Analogous to [`ScanElement`] but for N-dimensional states:
/// `h_t = A_t · h_{t-1} + B_t` with A_t ∈ R^{N×N}, B_t ∈ R^N.
#[derive(Debug, Clone)]
pub struct MultiDimScanElement {
    /// Linear part (state transition matrix, N×N)
    pub a: Array2<f64>,
    /// Additive part (input contribution, N)
    pub b: Array1<f64>,
}

impl MultiDimScanElement {
    /// Create a new multi-dimensional scan element.
    pub fn new(a: Array2<f64>, b: Array1<f64>) -> Self {
        Self { a, b }
    }

    /// Binary associative composition.
    ///
    /// `self` is the *later* element, `other` is the *earlier*.
    /// Result: h = (A₂·A₁)·h₀ + (A₂·B₁ + B₂).
    pub fn compose(&self, other: &MultiDimScanElement) -> MultiDimScanElement {
        MultiDimScanElement {
            a: self.a.dot(&other.a),
            b: self.a.dot(&other.b) + &self.b,
        }
    }

    /// Identity element: h_t = h_{t-1} (identity transition, no input).
    pub fn identity(n: usize) -> Self {
        MultiDimScanElement {
            a: Array2::eye(n),
            b: Array1::zeros(n),
        }
    }
}

pub struct MultiDimScan;

impl MultiDimScan {
    /// Scan over multi-dimensional states.
    ///
    /// # Arguments
    ///
    /// * `a_matrices` - Sequence of state transition matrices (L, N, N)
    /// * `b_vectors` - Sequence of input vectors (L, N)
    /// * `h0` - Initial state (N,)
    ///
    /// # Returns
    ///
    /// State sequence (L, N)
    pub fn scan(
        a_matrices: &[Array2<f64>],
        b_vectors: &[Array1<f64>],
        h0: &Array1<f64>,
    ) -> Vec<Array1<f64>> {
        let l = a_matrices.len();
        assert_eq!(b_vectors.len(), l);

        let mut states = Vec::with_capacity(l);
        let mut h = h0.clone();

        for t in 0..l {
            h = a_matrices[t].dot(&h) + &b_vectors[t];
            states.push(h.clone());
        }

        states
    }

    /// Parallel (Blelloch) scan over multi-dimensional states.
    ///
    /// Computes inclusive prefixes for the recurrence
    /// `h_t = A_t · h_{t-1} + B_t` using a binary tree reduction.
    ///
    /// Returns `Vec<(A_combined, B_combined)>` where
    /// `h_t = A_combined · h_0 + B_combined`.
    ///
    /// # Arguments
    ///
    /// * `a_matrices` - Sequence of state transition matrices (L, N, N)
    /// * `b_vectors` - Sequence of input vectors (L, N)
    ///
    /// # Returns
    ///
    /// Inclusive prefix pairs `(A_combined, B_combined)` — one per timestep
    pub fn parallel_scan(
        a_matrices: &[Array2<f64>],
        b_vectors: &[Array1<f64>],
    ) -> Vec<(Array2<f64>, Array1<f64>)> {
        let l = a_matrices.len();
        assert_eq!(b_vectors.len(), l);
        if l == 0 {
            return vec![];
        }

        let n = a_matrices[0].shape()[0];
        let next_pow2 = l.next_power_of_two();

        // Build tree with identity padding
        let mut tree: Vec<MultiDimScanElement> = a_matrices
            .iter()
            .zip(b_vectors.iter())
            .map(|(a, b)| MultiDimScanElement::new(a.clone(), b.clone()))
            .collect();
        tree.resize_with(next_pow2, || MultiDimScanElement::identity(n));

        // Phase 1: Up-sweep
        let mut step = 1;
        while step < next_pow2 {
            let stride = step * 2;
            for i in (0..next_pow2).step_by(stride) {
                let composed = tree[i + stride - 1].compose(&tree[i + step - 1]);
                tree[i + stride - 1] = composed;
            }
            step = stride;
        }

        // Phase 2: Down-sweep
        tree[next_pow2 - 1] = MultiDimScanElement::identity(n);
        step = next_pow2 / 2;
        while step > 0 {
            let stride = step * 2;
            for i in (0..next_pow2).step_by(stride) {
                let parent = tree[i + stride - 1].clone();
                let left_comp = tree[i + step - 1].clone();
                tree[i + step - 1] = parent.clone();
                tree[i + stride - 1] = left_comp.compose(&parent);
            }
            step /= 2;
        }

        // Convert exclusive → inclusive: result[i] = elements[i] ∘ exclusive[i]
        let mut result = Vec::with_capacity(l);
        for i in 0..l {
            let incl = MultiDimScanElement::new(a_matrices[i].clone(), b_vectors[i].clone())
                .compose(&tree[i]);
            result.push((incl.a, incl.b));
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_simple() {
        // A_t = 1 for all t → h_t = h_{t-1} + B_t
        let elements = vec![
            ScanElement::new(1.0, 3.0),
            ScanElement::new(1.0, 4.0),
            ScanElement::new(1.0, 5.0),
        ];

        let result = ParallelScan::scan(&elements, 0.0);
        assert_eq!(result, vec![3.0, 7.0, 12.0]);
    }

    #[test]
    fn test_parallel_scan_equivalence() {
        let elements = vec![
            ScanElement::new(0.5, 1.0),
            ScanElement::new(0.8, 2.0),
            ScanElement::new(0.3, 3.0),
            ScanElement::new(0.9, 4.0),
        ];

        let sequential = ParallelScan::scan(&elements, 0.0);
        let parallel = ParallelScan::parallel_scan(&elements);

        // Reconstruct h_t from prefix elements: h_t = prefix[t].a * h0 + prefix[t].b
        let parallel_h: Vec<f64> = parallel.iter().map(|p| p.a * 0.0 + p.b).collect();

        assert_eq!(sequential.len(), parallel_h.len());
        for (s, p) in sequential.iter().zip(parallel_h.iter()) {
            assert!((s - p).abs() < 1e-10);
        }
    }

    #[test]
    fn test_prefix_sum() {
        let input = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = ParallelScan::prefix_sum(&input);
        assert_eq!(result, vec![1.0, 3.0, 6.0, 10.0, 15.0]);
    }

    #[test]
    fn test_selective_scan_1d() {
        let a_seq = vec![0.5, 0.5, 0.5];
        let b_seq = vec![1.0, 2.0, 3.0];

        // h1 = 0.5*0 + 1.0 = 1.0
        // h2 = 0.5*1.0 + 2.0 = 2.5
        // h3 = 0.5*2.5 + 3.0 = 4.25
        let result = ParallelScan::selective_scan_1d(&a_seq, &b_seq, 0.0);
        assert_eq!(result, vec![1.0, 2.5, 4.25]);
    }

    #[test]
    fn test_compose_identity() {
        let e = ScanElement::new(0.7, 3.0);
        let id = ScanElement::identity();

        let composed = e.compose(&id);
        assert!((composed.a - 0.7).abs() < 1e-10);
        assert!((composed.b - 3.0).abs() < 1e-10);
    }

    #[test]
    fn test_empty_scan() {
        let empty: &[ScanElement] = &[];
        let result = ParallelScan::scan(empty, 0.0);
        assert!(result.is_empty());

        let result = ParallelScan::parallel_scan(empty);
        assert!(result.is_empty());
    }

    #[test]
    fn test_multidim_parallel_scan_equivalence() {
        let n = 3;
        let l = 5;
        let mut rng = rand::thread_rng();

        // Generate random A matrices (diagonally dominant for stability)
        let a_matrices: Vec<Array2<f64>> = (0..l)
            .map(|_| {
                let mut a = Array2::zeros((n, n));
                for i in 0..n {
                    for j in 0..n {
                        a[[i, j]] = rand::Rng::gen_range(&mut rng, -0.3..0.3);
                    }
                    a[[i, i]] += 0.5; // diagonal boost
                }
                a
            })
            .collect();

        let b_vectors: Vec<Array1<f64>> = (0..l)
            .map(|_| Array1::from_shape_fn(n, |_| rand::Rng::gen_range(&mut rng, -0.5..0.5)))
            .collect();

        let h0 = Array1::from_shape_fn(n, |_| rand::Rng::gen_range(&mut rng, -0.2..0.2));

        // Sequential
        let seq_states = MultiDimScan::scan(&a_matrices, &b_vectors, &h0);

        // Parallel scan → inclusive prefixes: h_t = A_combined·h0 + B_combined
        let prefixes = MultiDimScan::parallel_scan(&a_matrices, &b_vectors);
        assert_eq!(prefixes.len(), l);

        for t in 0..l {
            let h_par = prefixes[t].0.dot(&h0) + &prefixes[t].1;
            for i in 0..n {
                assert!(
                    (h_par[i] - seq_states[t][i]).abs() < 1e-10,
                    "Mismatch at t={}, dim={}: seq={}, par={}",
                    t,
                    i,
                    seq_states[t][i],
                    h_par[i]
                );
            }
        }
    }

    #[test]
    fn test_multidim_parallel_scan_empty() {
        let empty_a: &[Array2<f64>] = &[];
        let empty_b: &[Array1<f64>] = &[];
        let result = MultiDimScan::parallel_scan(empty_a, empty_b);
        assert!(result.is_empty());
    }
}
