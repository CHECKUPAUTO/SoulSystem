// ---------------------------------------------------------------------------
// SciRust Core — Unified facade for all scientific computing layers
// ---------------------------------------------------------------------------

pub mod error;
pub use error::{SciError, CoreResult};

// Layer 1: Math Core (autodiff + symbolic)
pub use scirust_autodiff as autodiff;
pub use scirust_autodiff::Dual;
pub use scirust_symbolic as symbolic;
pub use scirust_symbolic::{Expr, parse, simplify, diff, eval, to_rust_code};

// Layer 2: Reasoning Engine
pub use scirust_reasoning as reasoning;
pub use scirust_reasoning::{solve_linear, solve_quadratic, Optimizer, apply_trig_identity, prove_equal};

// Layer 3: Learning / Adaptation
pub use scirust_learning as learning;
pub use scirust_learning::{PatternMemory, polynomial_fit, linear_regression, discover_patterns};

// Layer 4: Execution (SIMD + GPU dispatch)
pub use scirust_simd as simd;
pub use scirust_gpu as gpu;
pub use scirust_gpu::dispatch;
pub use scirust_simd::simd_add_one;

// Layer 5: IA Bridge (NLP → math pipeline)
pub use scirust_bridge as bridge;
pub use scirust_bridge::{Pipeline, PipelineOutput, parse_natural, NaturalCommand};

// Macros
pub use scirust_macros::autodiff;
pub use scirust_simd_macros::simd;
pub use scirust_gpu_macros::gpu;

// ---------------------------------------------------------------------------
// Convenience: unified SciRust prelude
// ---------------------------------------------------------------------------

pub mod prelude {
    pub use scirust_autodiff::Dual;
    pub use scirust_autodiff::{derivative_1d, gradient_2d, gradient_3d};
    pub use scirust_symbolic::{Expr, parse, simplify, diff, eval, to_rust_code};
    pub use scirust_reasoning::{solve_linear, solve_quadratic, Optimizer, apply_trig_identity, prove_equal};
    pub use scirust_learning::{PatternMemory, polynomial_fit, linear_regression, discover_patterns};
    pub use scirust_bridge::{Pipeline, parse_natural, NaturalCommand};
    pub use scirust_simd::ops::{add_f32, mul_f32, add_f64, mul_f64};
    pub use scirust_gpu::dispatch::gpu_or_cpu;
}

// ---------------------------------------------------------------------------
// Tensor helpers (ndarray integration)
// ---------------------------------------------------------------------------

pub use scirust_learning::tensor;

// ---------------------------------------------------------------------------
// High-level unified pipeline (one-shot API)
// ---------------------------------------------------------------------------

use std::collections::HashMap;

/// Result of a unified SciRust computation.
#[derive(Debug, Clone)]
pub struct SciResult {
    pub parsed: String,
    pub simplified: String,
    pub derivative: String,
    pub value: f64,
    pub rust_code: String,
}

/// Run the full SciRust pipeline on a mathematical expression string.
///
/// Example:
/// ```
/// use std::collections::HashMap;
/// let mut vars = HashMap::new();
/// vars.insert("x".to_string(), 2.0);
/// let result = scirust_core::compute("x^2 + 2*x + 1", &vars);
/// ```
pub fn compute(input: &str, vars: &HashMap<String, f64>) -> Result<SciResult, String> {
    let mut pipeline = Pipeline::new().with_vars(vars.clone());
    let out = pipeline.run(input)?;
    Ok(SciResult {
        parsed: out.parsed,
        simplified: out.simplified,
        derivative: out.derivative,
        value: out.value,
        rust_code: out.rust_code,
    })
}

/// Compute the exact derivative of a 1D function at a point using Dual numbers.
pub fn deriv<F>(f: F, x: f64) -> f64
where
    F: Fn(Dual) -> Dual,
{
    scirust_autodiff::derivative_1d(f, x)
}

/// Compute the gradient of a 2D function at a point.
pub fn grad2<F>(f: F, x: f64, y: f64) -> (f64, f64)
where
    F: Fn(Dual, Dual) -> Dual,
{
    scirust_autodiff::gradient_2d(f, x, y)
}
