//! SIMD backend trait — pluggable compute backends for matrix operations.
//!
//! Implementations dispatch to CPU SIMD (AVX/SSE), CUDA, or WebGPU
//! depending on hardware availability.

/// Shape descriptor for matrix views.
pub trait MatrixShape {
    /// Return (rows, columns).
    fn shape(&self) -> (usize, usize);

    /// Return a reference to row `i`, or `None` if out of bounds.
    fn row_slice(&self, i: usize) -> Option<&[f32]>;
}

/// A read-only 2-D view over row-major storage.
#[derive(Debug, Clone)]
pub struct MatrixView<'a, T> {
    pub data: &'a [T],
    pub rows: usize,
    pub cols: usize,
}

impl<'a, T> MatrixView<'a, T> {
    pub fn new(data: &'a [T], rows: usize, cols: usize) -> Self {
        assert_eq!(data.len(), rows * cols);
        Self { data, rows, cols }
    }
}

impl<T> MatrixShape for MatrixView<'_, T>
where
    T: Copy,
{
    #[inline]
    fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    #[inline]
    fn row_slice(&self, i: usize) -> Option<&[f32]> {
        if i >= self.rows {
            return None;
        }
        // SAFETY: same layout when T = f32
        let start = i * self.cols;
        let len = self.cols;
        let ptr = self.data.as_ptr() as *const f32;
        Some(unsafe { std::slice::from_raw_parts(ptr.add(start), len) })
    }
}

impl<T> std::ops::Index<(usize, usize)> for MatrixView<'_, T>
where
    T: Copy,
{
    type Output = T;
    #[inline]
    fn index(&self, (row, col): (usize, usize)) -> &Self::Output {
        &self.data[row * self.cols + col]
    }
}

/// A mutable 2-D view over row-major storage.
pub struct MatrixViewMut<'a, T> {
    pub data: &'a mut [T],
    pub rows: usize,
    pub cols: usize,
}

impl<'a, T> MatrixViewMut<'a, T> {
    pub fn new(data: &'a mut [T], rows: usize, cols: usize) -> Self {
        assert_eq!(data.len(), rows * cols);
        Self { data, rows, cols }
    }
}

impl<T> MatrixShape for MatrixViewMut<'_, T>
where
    T: Copy,
{
    #[inline]
    fn shape(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    #[inline]
    fn row_slice(&self, i: usize) -> Option<&[f32]> {
        if i >= self.rows {
            return None;
        }
        let start = i * self.cols;
        let len = self.cols;
        let ptr = self.data.as_ptr() as *const f32;
        Some(unsafe { std::slice::from_raw_parts(ptr.add(start), len) })
    }
}

impl<T> std::ops::Index<(usize, usize)> for MatrixViewMut<'_, T>
where
    T: Copy,
{
    type Output = T;
    #[inline]
    fn index(&self, (row, col): (usize, usize)) -> &Self::Output {
        &self.data[row * self.cols + col]
    }
}

impl<T: Copy> std::ops::IndexMut<(usize, usize)> for MatrixViewMut<'_, T> {
    #[inline]
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut Self::Output {
        &mut self.data[row * self.cols + col]
    }
}
