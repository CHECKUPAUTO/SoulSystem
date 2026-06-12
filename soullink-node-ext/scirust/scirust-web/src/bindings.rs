//! WASM bindings for browser-based inference.
//!
//! These functions are exported to JavaScript when compiled with `wasm-pack`.
//! They provide a thin wrapper over `scirust-inference` operations.

use scirust_inference::layers::{Linear, Layer, ReLU};
use scirust_inference::pipeline::SequentialModel;
use scirust_inference::tensor::Tensor;

/// A model handle for JavaScript (opaque pointer).
pub struct JSModel {
    model: SequentialModel,
}

/// Load a model from a JSON-serialized state.
///
/// # Safety
/// Called from JavaScript. The JSON format matches `ModelState::to_json`.
#[no_mangle]
pub extern "C" fn scirust_load_model(json_ptr: *const u8, json_len: usize) -> *mut JSModel {
    if json_ptr.is_null() || json_len == 0 { return std::ptr::null_mut(); }
    let json_slice = unsafe { std::slice::from_raw_parts(json_ptr, json_len) };
    let json_str = match std::str::from_utf8(json_slice) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    // Parse JSON and build model
    let model = match build_model_from_json(json_str) {
        Ok(m) => m,
        Err(_) => return std::ptr::null_mut(),
    };

    Box::into_raw(Box::new(JSModel { model }))
}

/// Run inference on a flat f32 array.
///
/// Returns a pointer to the output data. The caller must call
/// `scirust_free_output` to release the memory.
#[no_mangle]
pub extern "C" fn scirust_predict(
    model_ptr: *mut JSModel,
    input_ptr: *const f32,
    input_len: usize,
    input_shape_ptr: *const usize,
    input_shape_len: usize,
    out_len: *mut usize,
) -> *mut f32 {
    if model_ptr.is_null() || input_ptr.is_null() || input_shape_ptr.is_null() || out_len.is_null() {
        return std::ptr::null_mut();
    }
    let model = unsafe { &*model_ptr };

    let input_shape = unsafe { std::slice::from_raw_parts(input_shape_ptr, input_shape_len) };
    let input_data = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };

    let input = match Tensor::<f32>::from_vec(input_data.to_vec(), input_shape) {
        Ok(t) => t,
        Err(_) => return std::ptr::null_mut(),
    };

    let output = match model.model.forward(&input) {
        Ok(o) => o,
        Err(_) => return std::ptr::null_mut(),
    };

    let out_data = output.data().to_vec();
    unsafe { *out_len = out_data.len(); }

    let out_ptr = out_data.as_ptr() as *mut f32;
    std::mem::forget(out_data); // Leak to JS — caller must free
    out_ptr
}

/// Free a model handle.
#[no_mangle]
pub extern "C" fn scirust_free_model(ptr: *mut JSModel) {
    if !ptr.is_null() {
        unsafe { let _ = Box::from_raw(ptr); }
    }
}

/// Free output data allocated by `scirust_predict`.
#[no_mangle]
pub extern "C" fn scirust_free_output(ptr: *mut f32, len: usize) {
    if !ptr.is_null() {
        unsafe { let _ = Vec::from_raw_parts(ptr, len, len); }
    }
}

/// Build a simple MLP model from JSON config.
fn build_model_from_json(json: &str) -> Result<SequentialModel, String> {
    // Simplified: build a standard 784→256→128→10 MLP
    let mut model = SequentialModel::new();
    model.add(Linear::new(784, 256, true));
    model.add(ReLU::new());
    model.add(Linear::new(256, 128, true));
    model.add(ReLU::new());
    model.add(Linear::new(128, 10, false));
    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_load_model_null_safety() {
        let ptr = scirust_load_model(std::ptr::null(), 0);
        assert!(ptr.is_null());
    }

    #[test]
    fn test_load_model_null_with_len() {
        let ptr = scirust_load_model(std::ptr::null(), 10);
        assert!(ptr.is_null());
    }

    #[test]
    fn test_predict_null_safety() {
        let mut out_len = 0usize;
        let result = scirust_predict(
            std::ptr::null_mut(),
            std::ptr::null(), 0,
            std::ptr::null(), 0,
            &mut out_len,
        );
        assert!(result.is_null());
    }

    #[test]
    fn test_free_null_safety() {
        // Should not crash
        scirust_free_model(std::ptr::null_mut());
        scirust_free_output(std::ptr::null_mut(), 0);
    }
}
