//! Neural network layer definitions for inference.

use crate::tensor::Tensor;

pub mod linear;
pub mod conv;
pub mod norm;
pub mod activation;

pub use linear::Linear;
pub use conv::Conv2d;
pub use norm::BatchNorm;
pub use activation::{ReLU, Sigmoid, Tanh, GELU};

/// Trait that all neural network layers must implement.
pub trait Layer: Send + Sync {
    fn forward(&self, input: &Tensor<f32>) -> Result<Tensor<f32>, String>;
    fn parameters(&self) -> Vec<&Tensor<f32>>;
    fn parameters_mut(&mut self) -> Vec<&mut Tensor<f32>>;
    fn name(&self) -> &str;

    /// Layer type identifier for runtime dispatch (e.g. "Linear", "Conv2d").
    fn layer_type(&self) -> &str { self.name() }

    /// Downcast to `std::any::Any` for accessing concrete layer methods.
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Activation function enum for serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serialization", derive(serde::Serialize, serde::Deserialize))]
pub enum ActivationType {
    ReLU,
    Sigmoid,
    Tanh,
    GELU,
    None,
}
