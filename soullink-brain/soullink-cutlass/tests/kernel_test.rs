use soullink_cutlass::{CutlassError, GpuTensor, CudaStream, KernelConfig};

#[test]
fn kernel_config_default() {
    let c = KernelConfig::default();
    assert_eq!(c.block_size, 256);
}

#[test]
fn stream_without_feature_returns_disabled() {
    let result = CudaStream::new();
    assert!(matches!(result, Err(CutlassError::FeatureDisabled)));
}

#[test]
fn gpu_tensor_f32_without_feature() {
    let result = GpuTensor::<f32>::zeros_f32(&[4]);
    assert!(matches!(result, Err(CutlassError::FeatureDisabled)));
}

#[test]
fn gpu_tensor_u32_without_feature() {
    let result = GpuTensor::<u32>::zeros_u32(&[4]);
    assert!(matches!(result, Err(CutlassError::FeatureDisabled)));
}

#[test]
fn gpu_tensor_from_slice_without_feature() {
    let result = GpuTensor::<f32>::from_slice_f32(&[1.0, 2.0, 3.0], &[2, 2]);
    // Without cutlass feature, returns FeatureDisabled (shape check is feature-gated)
    assert!(matches!(result, Err(CutlassError::FeatureDisabled)));
}