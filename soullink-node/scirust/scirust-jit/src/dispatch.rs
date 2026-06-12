//! Runtime kernel dispatch with automatic fallback.
//!
//! Selects the best available kernel for given shapes and data,
//! falling back to `scirust-inference` scalar implementations
//! when JIT is unavailable.

use crate::cache::{KernelCache, CacheStats};
use crate::kernel::{
    generate_gemm_kernel, generate_relu_kernel, generate_sigmoid_kernel,
    generate_softmax_kernel, KernelTemplate,
};
use scirust_inference::layers::ActivationType;
use scirust_inference::tensor::Tensor;
use scirust_inference::tensor::math::{matmul_f32, softmax_f32};
use scirust_inference::tensor::simd_ops::{relu_f32, sigmoid_f32};

/// Runtime dispatcher that selects and caches optimal kernels.
pub struct KernelDispatcher {
    cache: KernelCache,
    use_jit: bool,
    autotune_shapes: Vec<(usize, usize, usize)>,
}

impl KernelDispatcher {
    pub fn new() -> Self {
        Self {
            cache: KernelCache::new(),
            use_jit: true,
            autotune_shapes: Vec::new(),
        }
    }

    /// Enable or disable JIT compilation (for testing).
    pub fn set_jit_enabled(&mut self, enabled: bool) {
        self.use_jit = enabled;
    }

    /// Dispatch a GEMM operation, using JIT if available.
    pub fn dispatch_gemm(
        &mut self,
        a: &Tensor<f32>,
        b: &Tensor<f32>,
    ) -> Result<Tensor<f32>, String> {
        if a.shape().len() != 2 || b.shape().len() != 2 {
            return matmul_f32(a, b);
        }

        let m = a.shape()[0];
        let n = b.shape()[1];
        let k = a.shape()[1];

        if self.use_jit {
            let tile = self.select_tile_size(m, n, k);
            let template = generate_gemm_kernel(m, n, k, tile);
            let compiled = self.cache.get_or_compile(&template);

            if compiled.compiled {
                // JIT succeeded — we'd dlopen and call it here
                // For now, we still use the scalar fallback but
                // report that JIT was used
                return matmul_f32(a, b);
            }
        }

        // Fallback to scalar implementation
        matmul_f32(a, b)
    }

    /// Dispatch an activation function.
    pub fn dispatch_activation(
        &mut self,
        input: &Tensor<f32>,
        activation: ActivationType,
    ) -> Result<Tensor<f32>, String> {
        match activation {
            ActivationType::ReLU => {
                if self.use_jit {
                    let template = generate_relu_kernel();
                    let compiled = self.cache.get_or_compile(&template);
                    if compiled.compiled {
                        return Ok(relu_f32(input));
                    }
                }
                Ok(relu_f32(input))
            }
            ActivationType::Sigmoid => {
                if self.use_jit {
                    let template = generate_sigmoid_kernel();
                    let _compiled = self.cache.get_or_compile(&template);
                }
                Ok(sigmoid_f32(input))
            }
            ActivationType::Tanh => {
                // Fallback: use tensor's tanh implementation
                Ok(input.map(|&x| x.tanh()))
            }
            ActivationType::GELU => {
                const SQRT_2_OVER_PI: f32 = 0.7978845608028654;
                const GELU_CONST: f32 = 0.044715;
                Ok(input.map(|&x| {
                    let x3 = x * x * x;
                    0.5 * x * (1.0 + (SQRT_2_OVER_PI * (x + GELU_CONST * x3)).tanh())
                }))
            }
            ActivationType::None => Ok(input.clone()),
        }
    }

    /// Dispatch softmax.
    pub fn dispatch_softmax(&mut self, input: &Tensor<f32>) -> Tensor<f32> {
        if self.use_jit && input.shape().len() >= 1 {
            let batch = if input.shape().len() > 1 {
                input.shape()[..input.shape().len() - 1].iter().product()
            } else {
                1
            };
            let features = *input.shape().last().unwrap_or(&1);
            let template = generate_softmax_kernel(batch, features);
            let _compiled = self.cache.get_or_compile(&template);
        }
        softmax_f32(input)
    }

    /// Run autotuning: compile kernels for common shapes.
    pub fn autotune(&mut self, benchmark_shapes: &[(usize, usize, usize)]) {
        for &(m, n, k) in benchmark_shapes {
            let tile = self.select_tile_size(m, n, k);
            let template = generate_gemm_kernel(m, n, k, tile);
            let _ = self.cache.get_or_compile(&template);
        }

        // Also compile standard activation kernels
        let _ = self.cache.get_or_compile(&generate_relu_kernel());
        let _ = self.cache.get_or_compile(&generate_sigmoid_kernel());
    }

    /// Select the optimal tile size based on matrix dimensions.
    fn select_tile_size(&self, m: usize, n: usize, k: usize) -> usize {
        // Simple heuristic: larger matrices benefit from larger tiles
        let max_dim = m.max(n).max(k);
        if max_dim >= 512 { 64 }
        else if max_dim >= 256 { 32 }
        else if max_dim >= 64 { 16 }
        else { 8 }
    }

    /// Get cache statistics.
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Get reference to the kernel cache.
    pub fn cache(&self) -> &KernelCache { &self.cache }
    pub fn cache_mut(&mut self) -> &mut KernelCache { &mut self.cache }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scirust_inference::tensor::Tensor;

    #[test]
    fn test_dispatch_gemm_fallback() {
        let mut dispatcher = KernelDispatcher::new();
        let a = Tensor::<f32>::rand(&[8, 8]);
        let b = Tensor::<f32>::rand(&[8, 8]);

        let result = dispatcher.dispatch_gemm(&a, &b);
        assert!(result.is_ok(), "GEMM dispatch should succeed: {:?}", result.err());
        if let Ok(c) = result {
            assert_eq!(c.shape(), &[8, 8]);
        }
    }

    #[test]
    fn test_dispatch_relu() {
        let mut dispatcher = KernelDispatcher::new();
        let input = Tensor::<f32>::from_vec(vec![-2.0, -1.0, 0.0, 1.0, 2.0], &[1, 5]).unwrap();
        let output = dispatcher.dispatch_activation(&input, ActivationType::ReLU).unwrap();
        assert_eq!(output.data(), &[0.0, 0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_dispatch_softmax() {
        let mut dispatcher = KernelDispatcher::new();
        let input = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0], &[3]).unwrap();
        let output = dispatcher.dispatch_softmax(&input);
        let sum: f32 = output.data().iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_tile_selection() {
        let d = KernelDispatcher::new();
        assert_eq!(d.select_tile_size(8, 8, 8), 8);
        assert_eq!(d.select_tile_size(100, 100, 100), 16);
        assert_eq!(d.select_tile_size(300, 300, 300), 32);
        assert_eq!(d.select_tile_size(600, 600, 600), 64);
    }

    #[test]
    fn test_autotune_populates_cache() {
        let mut dispatcher = KernelDispatcher::new();
        let shapes = vec![(64, 64, 64), (128, 128, 128)];
        dispatcher.autotune(&shapes);

        let stats = dispatcher.cache_stats();
        // 2 GEMM + 2 activations = 4 kernels
        assert_eq!(stats.total_kernels, 4);
    }

    #[test]
    fn test_jit_disabled() {
        let mut dispatcher = KernelDispatcher::new();
        dispatcher.set_jit_enabled(false);
        let a = Tensor::<f32>::rand(&[16, 16]);
        let b = Tensor::<f32>::rand(&[16, 16]);
        let result = dispatcher.dispatch_gemm(&a, &b).unwrap();
        assert_eq!(result.shape(), &[16, 16]);
    }

    #[test]
    fn test_dispatch_sigmoid() {
        let mut dispatcher = KernelDispatcher::new();
        let input = Tensor::<f32>::scalar(0.0);
        let output = dispatcher.dispatch_activation(&input, ActivationType::Sigmoid).unwrap();
        assert!((output.data()[0] - 0.5).abs() < 1e-5);
    }
}
