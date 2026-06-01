//! Tensor: N-dimensional array with SIMD-accelerated linear algebra.

pub mod simd_ops;
pub mod math;

pub use simd_ops::*;
pub use math::*;

use std::fmt;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Tensor
// ---------------------------------------------------------------------------

/// N-dimensional array stored in row-major (C-style) contiguous memory.
#[derive(Debug, Clone)]
pub struct Tensor<T: Clone + Default> {
    data: Arc<Vec<T>>,
    shape: Vec<usize>,
    strides: Vec<usize>,
    offset: usize,
    len: usize,
}

impl<T: Clone + Default> Tensor<T> {
    pub fn new(shape: &[usize]) -> Self {
        let len: usize = shape.iter().product();
        let strides = compute_strides(shape);
        Self {
            data: Arc::new(vec![T::default(); len]),
            shape: shape.to_vec(),
            strides,
            offset: 0,
            len,
        }
    }

    pub fn from_vec(data: Vec<T>, shape: &[usize]) -> Result<Self, String> {
        let expected: usize = shape.iter().product();
        if data.len() != expected {
            return Err(format!(
                "Shape {:?} expects {} elements but got {}",
                shape, expected, data.len()
            ));
        }
        let strides = compute_strides(shape);
        let len = data.len();
        Ok(Self {
            data: Arc::new(data),
            shape: shape.to_vec(),
            strides,
            offset: 0,
            len,
        })
    }

    pub fn scalar(value: T) -> Self {
        Self {
            data: Arc::new(vec![value]),
            shape: vec![],
            strides: vec![],
            offset: 0,
            len: 1,
        }
    }

    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn shape(&self) -> &[usize] { &self.shape }
    pub fn ndim(&self) -> usize { self.shape.len() }

    pub fn as_slice(&self) -> Option<&[T]> {
        if self.is_contiguous() {
            Some(&self.data[self.offset..self.offset + self.len])
        } else {
            None
        }
    }

    pub fn to_contiguous(&self) -> Vec<T> {
        let mut result = Vec::with_capacity(self.len);
        for i in 0..self.len {
            result.push(self.get_flat(i).clone());
        }
        result
    }

    pub fn is_contiguous(&self) -> bool {
        self.offset == 0
    }

    pub fn get_flat(&self, index: usize) -> &T {
        &self.data[self.offset + index]
    }

    pub fn get_flat_mut(&mut self, index: usize) -> &mut T {
        &mut Arc::make_mut(&mut self.data)[self.offset + index]
    }

    pub fn data_mut(&mut self) -> &mut Vec<T> {
        Arc::make_mut(&mut self.data)
    }

    pub fn data(&self) -> &[T] {
        &self.data[self.offset..self.offset + self.len]
    }

    pub fn fill(&mut self, value: T) {
        let data = Arc::make_mut(&mut self.data);
        for i in self.offset..self.offset + self.len {
            data[i] = value.clone();
        }
    }

    pub fn map_in_place<F: Fn(&T) -> T>(&mut self, f: F) {
        let data = Arc::make_mut(&mut self.data);
        for i in self.offset..self.offset + self.len {
            data[i] = f(&data[i]);
        }
    }

    pub fn map<F: Fn(&T) -> T>(&self, f: F) -> Self {
        let mut result = Vec::with_capacity(self.len);
        for i in 0..self.len {
            result.push(f(self.get_flat(i)));
        }
        Tensor::from_vec(result, &self.shape).unwrap()
    }

    pub fn reshape(&self, new_shape: &[usize]) -> Result<Self, String> {
        let new_len: usize = new_shape.iter().product();
        if new_len != self.len {
            return Err(format!(
                "Cannot reshape {} elements into shape {:?}",
                self.len, new_shape
            ));
        }
        let strides = compute_strides(new_shape);
        Ok(Self {
            data: self.data.clone(),
            shape: new_shape.to_vec(),
            strides,
            offset: self.offset,
            len: self.len,
        })
    }

    pub fn transpose(&self) -> Result<Self, String> {
        if self.shape.len() != 2 {
            return Err("Transpose requires 2D tensor".to_string());
        }
        let rows = self.shape[0];
        let cols = self.shape[1];
        let mut result = Tensor::<T>::new(&[cols, rows]);
        for i in 0..rows {
            for j in 0..cols {
                let src = self.get(&[i, j]).unwrap().clone();
                *result.get_mut(&[j, i]).unwrap() = src;
            }
        }
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Indexing
    // -----------------------------------------------------------------------

    pub fn get(&self, indices: &[usize]) -> Option<&T> {
        if indices.len() != self.shape.len() {
            return None;
        }
        let flat_idx = self.flat_index(indices);
        self.data.get(self.offset + flat_idx)
    }

    pub fn get_2d(&self, i: usize, j: usize) -> Option<&T> {
        self.get(&[i, j])
    }

    pub fn get_mut(&mut self, indices: &[usize]) -> Option<&mut T> {
        if indices.len() != self.shape.len() {
            return None;
        }
        let flat_idx = self.flat_index(indices);
        let global = self.offset + flat_idx;
        if global >= self.data.len() {
            return None;
        }
        Some(&mut Arc::make_mut(&mut self.data)[global])
    }

    pub fn get_mut_2d(&mut self, i: usize, j: usize) -> Option<&mut T> {
        self.get_mut(&[i, j])
    }

    pub fn unravel_index(&self, mut idx: usize) -> Vec<usize> {
        let mut indices = vec![0usize; self.shape.len()];
        for d in (0..self.shape.len()).rev() {
            indices[d] = idx % self.shape[d];
            idx /= self.shape[d];
        }
        indices
    }

    fn flat_index(&self, indices: &[usize]) -> usize {
        let mut idx = 0usize;
        for (i, &dim) in indices.iter().enumerate() {
            idx += dim * self.strides[i];
        }
        idx
    }
}

// ---------------------------------------------------------------------------
// Convenience constructors for f32
// ---------------------------------------------------------------------------

impl Tensor<f32> {
    pub fn from_slice_1d(data: &[f32]) -> Self {
        Self::from_vec(data.to_vec(), &[data.len()]).unwrap()
    }

    pub fn from_slice_2d(data: &[&[f32]]) -> Self {
        let rows = data.len();
        let cols = if rows > 0 { data[0].len() } else { 0 };
        let flat: Vec<f32> = data.iter().flat_map(|r| r.iter()).copied().collect();
        Self::from_vec(flat, &[rows, cols]).unwrap()
    }

    pub fn ones(shape: &[usize]) -> Self {
        let mut t = Self::new(shape);
        t.fill(1.0);
        t
    }

    pub fn zeros(shape: &[usize]) -> Self {
        Self::new(shape)
    }

    pub fn rand(shape: &[usize]) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let mut rng = fast_rand(seed);
        let len: usize = shape.iter().product();
        let data: Vec<f32> = (0..len).map(|_| rng()).collect();
        Self::from_vec(data, shape).unwrap()
    }

    pub fn randn(shape: &[usize]) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let mut rng = fast_rand(seed);
        let len: usize = shape.iter().product();
        let data: Vec<f32> = (0..len)
            .map(|_| {
                let u1 = rng();
                let u2 = rng();
                (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
            })
            .collect();
        Self::from_vec(data, shape).unwrap()
    }
}

impl Tensor<f64> {
    pub fn from_slice_1d(data: &[f64]) -> Self {
        Self::from_vec(data.to_vec(), &[data.len()]).unwrap()
    }

    pub fn from_slice_2d(data: &[&[f64]]) -> Self {
        let rows = data.len();
        let cols = if rows > 0 { data[0].len() } else { 0 };
        let flat: Vec<f64> = data.iter().flat_map(|r| r.iter()).copied().collect();
        Self::from_vec(flat, &[rows, cols]).unwrap()
    }

    pub fn ones(shape: &[usize]) -> Self {
        let mut t = Self::new(shape);
        t.fill(1.0);
        t
    }

    pub fn zeros(shape: &[usize]) -> Self {
        Self::new(shape)
    }

    pub fn rand(shape: &[usize]) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let mut rng = fast_rand(seed);
        let len: usize = shape.iter().product();
        let data: Vec<f64> = (0..len).map(|_| rng() as f64).collect();
        Self::from_vec(data, shape).unwrap()
    }

    pub fn randn(shape: &[usize]) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as u64;
        let mut rng = fast_rand(seed);
        let len: usize = shape.iter().product();
        let data: Vec<f64> = (0..len)
            .map(|_| {
                let u1 = rng() as f64;
                let u2 = rng() as f64;
                (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
            })
            .collect();
        Self::from_vec(data, shape).unwrap()
    }
}

// ---------------------------------------------------------------------------
// Display
// ---------------------------------------------------------------------------

impl<T: Clone + Default + fmt::Display + std::fmt::Debug> fmt::Display for Tensor<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.shape.is_empty() {
            return write!(f, "Tensor(scalar, {:?})", self.data[self.offset]);
        }
        write!(f, "Tensor<{:?}, {}>", self.shape, self.len)?;
        if self.len <= 20 {
            write!(f, " [")?;
            for i in 0..self.len {
                if i > 0 { write!(f, ", ")?; }
                write!(f, "{}", self.get_flat(i))?;
            }
            write!(f, "]")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn compute_strides(shape: &[usize]) -> Vec<usize> {
    let mut strides = vec![1usize; shape.len()];
    for i in (1..shape.len()).rev() {
        strides[i - 1] = strides[i] * shape[i];
    }
    strides
}

fn fast_rand(seed: u64) -> impl FnMut() -> f32 {
    let mut state = seed;
    move || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let u32_val = (state >> 33) as u32;
        u32_val as f32 / 4294967296.0f32
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tensor_create() {
        let t = Tensor::<f32>::new(&[2, 3]);
        assert_eq!(t.shape(), &[2, 3]);
        assert_eq!(t.len(), 6);
    }

    #[test]
    fn test_tensor_from_vec() {
        let t = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        assert_eq!(*t.get_2d(0, 0).unwrap(), 1.0);
        assert_eq!(*t.get_2d(1, 1).unwrap(), 4.0);
    }

    #[test]
    fn test_tensor_reshape() {
        let t = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
        let r = t.reshape(&[3, 2]).unwrap();
        assert_eq!(r.shape(), &[3, 2]);
    }

    #[test]
    fn test_tensor_transpose() {
        let t = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        let rt = t.transpose().unwrap();
        assert_eq!(*rt.get_2d(0, 1).unwrap(), 3.0);  // row0,col1 = original[1,0] = 3
        assert_eq!(*rt.get_2d(1, 0).unwrap(), 2.0);  // row1,col0 = original[0,1] = 2
    }

    #[test]
    fn test_tensor_scalar() {
        let s = Tensor::<f32>::scalar(42.0);
        assert_eq!(s.ndim(), 0);
        assert_eq!(s.len(), 1);
    }
}
