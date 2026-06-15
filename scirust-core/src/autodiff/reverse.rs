//! Reverse-mode automatic differentiation.
//!
//! `Tensor` wraps a `Vec<f32>` with shape (rows × cols) and is the primary
//! data container for autodiff computations. `Tape` records operations for
//! gradient backpropagation.

/// A dense 2-D tensor with row-major storage.
#[derive(Debug, Clone)]
pub struct Tensor {
    /// Flattened data in row-major order: `data[row * cols + col]`.
    pub data: Vec<f32>,
    /// Number of rows.
    pub rows: usize,
    /// Number of columns.
    pub cols: usize,
}

impl Tensor {
    /// Create a tensor from a flat vector and explicit shape.
    pub fn from_vec(data: Vec<f32>, rows: usize, cols: usize) -> Self {
        assert_eq!(data.len(), rows * cols, "data length mismatch");
        Self { data, rows, cols }
    }

    /// Create a tensor filled with zeros.
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self {
            data: vec![0.0; rows * cols],
            rows,
            cols,
        }
    }

    /// Create a tensor filled with a constant value.
    pub fn full(rows: usize, cols: usize, value: f32) -> Self {
        Self {
            data: vec![value; rows * cols],
            rows,
            cols,
        }
    }

    /// Create a tensor from a 1-D row vector.
    pub fn from_row(v: &[f32]) -> Self {
        Self {
            data: v.to_vec(),
            rows: 1,
            cols: v.len(),
        }
    }

    /// Total number of elements.
    #[inline]
    pub fn len(&self) -> usize {
        self.rows * self.cols
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read element at (row, col).
    #[inline]
    pub fn get(&self, row: usize, col: usize) -> f32 {
        self.data[row * self.cols + col]
    }

    /// Write element at (row, col).
    #[inline]
    pub fn set(&mut self, row: usize, col: usize, value: f32) {
        self.data[row * self.cols + col] = value;
    }

    /// Row slice as &[f32].
    #[inline]
    pub fn row(&self, r: usize) -> &[f32] {
        let start = r * self.cols;
        &self.data[start..start + self.cols]
    }

    /// Mutable row slice.
    #[inline]
    pub fn row_mut(&mut self, r: usize) -> &mut [f32] {
        let start = r * self.cols;
        &mut self.data[start..start + self.cols]
    }

    /// Element-wise addition (consuming).
    pub fn add(mut self, other: &Tensor) -> Self {
        assert_eq!(self.len(), other.len());
        for (a, b) in self.data.iter_mut().zip(&other.data) {
            *a += b;
        }
        self
    }

    /// Element-wise multiplication.
    pub fn mul(mut self, other: &Tensor) -> Self {
        assert_eq!(self.len(), other.len());
        for (a, b) in self.data.iter_mut().zip(&other.data) {
            *a *= b;
        }
        self
    }

    /// Scale all elements.
    pub fn scale(mut self, s: f32) -> Self {
        for v in &mut self.data {
            *v *= s;
        }
        self
    }

    /// L2 norm.
    pub fn norm(&self) -> f32 {
        self.data.iter().map(|v| v * v).sum::<f32>().sqrt()
    }

    /// Dot product with another tensor (both flattened).
    pub fn dot(&self, other: &Tensor) -> f32 {
        assert_eq!(self.len(), other.len());
        self.data.iter().zip(&other.data).map(|(a, b)| a * b).sum()
    }

    /// Cosine similarity.
    pub fn cosine_similarity(&self, other: &Tensor) -> f32 {
        let dot = self.dot(other);
        let n1 = self.norm();
        let n2 = other.norm();
        if n1 == 0.0 || n2 == 0.0 {
            0.0
        } else {
            dot / (n1 * n2)
        }
    }
}

// ── Tape (autodiff execution graph) ──────────────────────────────────────────

/// A tape recording operations for reverse-mode gradient computation.
///
/// In production autodiff frameworks this tracks the computation DAG.
/// For the current codebase, it serves as an opaque handle.
#[derive(Debug, Default)]
pub struct Tape {
    ops: usize,
}

impl Tape {
    pub fn new() -> Self {
        Self { ops: 0 }
    }

    /// Record an operation on the tape.
    pub fn record(&mut self) {
        self.ops += 1;
    }

    /// Number of operations recorded.
    pub fn op_count(&self) -> usize {
        self.ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tensor_creation() {
        let t = Tensor::from_vec(vec![1.0, 2.0, 3.0, 4.0], 2, 2);
        assert_eq!(t.rows, 2);
        assert_eq!(t.cols, 2);
        assert_eq!(t.get(0, 1), 2.0);
    }

    #[test]
    fn tensor_zeros() {
        let t = Tensor::zeros(3, 2);
        assert_eq!(t.len(), 6);
        assert!(t.data.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn tensor_dot() {
        let a = Tensor::from_row(&[1.0, 2.0, 3.0]);
        let b = Tensor::from_row(&[4.0, 5.0, 6.0]);
        assert_eq!(a.dot(&b), 32.0);
    }

    #[test]
    fn tensor_cosine() {
        let a = Tensor::from_row(&[1.0, 0.0]);
        let b = Tensor::from_row(&[1.0, 0.0]);
        assert!((a.cosine_similarity(&b) - 1.0).abs() < 1e-6);
        let c = Tensor::from_row(&[0.0, 1.0]);
        assert!((a.cosine_similarity(&c) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn tape_records_ops() {
        let mut t = Tape::new();
        assert_eq!(t.op_count(), 0);
        t.record();
        t.record();
        assert_eq!(t.op_count(), 2);
    }
}
