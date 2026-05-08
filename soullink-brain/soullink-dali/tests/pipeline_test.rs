use soullink_dali::{DaliError, DaliPipeline, Device};

#[test]
fn pipeline_builder_default() {
    let b = DaliPipeline::builder();
    // builder compiles, that's the test
}

#[test]
fn build_without_feature_returns_disabled() {
    let result = DaliPipeline::builder()
        .batch_size(4)
        .build();
    assert!(matches!(result, Err(DaliError::FeatureDisabled)));
}

#[test]
fn device_serde_roundtrip() {
    let d = Device::Gpu(0);
    let json = serde_json::to_string(&d).unwrap();
    let d2: Device = serde_json::from_str(&json).unwrap();
    assert!(matches!(d2, Device::Gpu(0)));
}