use pyo3::prelude::*;
use scirust_core::matrix::view::{MatrixView, MatrixViewMut};

/// Internal helper to create a mutable matrix view from raw parts provided by Python.
/// This is the core of the Zero-Copy mechanism.
pub unsafe fn wrap_raw_mut(ptr: *mut f32, rows: usize, cols: usize) -> MatrixViewMut<'static, f32> {
    // Row-major assumption as per standard Numpy/Torch layouts used in these contexts.
    // row_stride = cols, col_stride = 1
    MatrixViewMut::from_raw_parts(ptr, rows, cols, cols, 1)
}

/// Internal helper to create an immutable matrix view from raw parts.
pub unsafe fn wrap_raw(ptr: *const f32, rows: usize, cols: usize) -> MatrixView<'static, f32> {
    MatrixView::from_raw_parts(ptr, rows, cols, cols, 1)
}

// The actual FFI functions will be implemented here or in lib.rs
// For Step 2, we provide the infrastructure for zero-copy mapping.
