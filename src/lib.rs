use pyo3::prelude::*;

pub mod ffi_api;
pub mod extractor;
pub mod surgeon;

/// Verifies the FFI bridge by attempting a small write/read cycle on a raw pointer.
///
/// # Arguments
/// * `ptr` - Raw pointer to the f32 data.
/// * `rows` - Number of rows in the matrix.
/// * `cols` - Number of columns in the matrix.
#[pyfunction]
fn check_ffi_bridge(ptr: *mut f32, rows: usize, cols: usize) -> PyResult<bool> {
    unsafe {
        let view = ffi_api::wrap_raw_mut(ptr, rows, cols);
        // Simple sanity check: write a value and read it back
        if rows > 0 && cols > 0 {
            let old_val = view[(0, 0)];
            view[(0, 0)] = old_val + 1.0;
            let new_val = view[(0, 0)];
            view[(0, 0)] = old_val; // restore
            Ok(new_val == old_val + 1.0)
        } else {
            Ok(false)
        }
    }
}

/// The `repe_core_lib` Python module.
#[pymodule]
fn repe_core_lib(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_function!(check_ffi_bridge, "check_ffi_bridge"))?;
    Ok(())
}

/*
COMPILATION INSTRUCTIONS:
1. Install maturin: `pip install maturin`
2. Build and develop the library with native CPU optimizations:
   `maturin develop --release -C target-cpu=native`
3. If using a custom path for scirust, ensure it's correctly set in Cargo.toml.
*/
