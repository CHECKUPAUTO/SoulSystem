//! SciRust Inference Engine — Industrial-grade neural network inference runtime.
//!
//! # Architecture
//!
//! ```text
//! scirust-inference/
//! ├── tensor/       # Generic N-dimensional array with SIMD-accelerated ops
//! ├── layers/       # Neural network layer definitions (Linear, Conv2d, BatchNorm, etc.)
//! ├── graph/        # Computation graph with fusion and constant folding
//! ├── quant/        # FP16/INT8 quantization
//! ├── serial/       # Model serialization (SafeTensors, JSON)
//! └── pipeline/     # High-level inference pipeline
//! ```

pub mod tensor;
pub mod layers;
pub mod graph;
pub mod quant;
pub mod serial;
pub mod pipeline;
pub mod train;

pub mod prelude {
    pub use crate::tensor::*;
    pub use crate::layers::*;
    pub use crate::graph::*;
    pub use crate::pipeline::*;
}
