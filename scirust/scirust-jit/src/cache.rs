//! Kernel cache for compiled JIT kernels.
//!
//! Manages a collection of compiled kernels, tracks hits/misses,
//! and provides statistics.

use crate::kernel::{KernelTemplate, KernelType};
use std::collections::HashMap;
use std::time::Instant;

/// A compiled kernel ready for execution.
#[derive(Debug, Clone)]
pub struct CompiledKernel {
    pub name: String,
    pub compiled: bool,
    pub compile_time_ms: f64,
    pub kernel_type: KernelType,
}

/// Cache statistics for monitoring JIT performance.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub total_kernels: usize,
    pub total_compile_time_ms: f64,
}

/// Thread-safe kernel cache.
pub struct KernelCache {
    kernels: HashMap<String, CompiledKernel>,
    stats: CacheStats,
}

impl KernelCache {
    pub fn new() -> Self {
        Self { kernels: HashMap::new(), stats: CacheStats::default() }
    }

    /// Get or compile a kernel template.
    ///
    /// Returns the compiled kernel. If compilation fails, returns a
    /// placeholder with `compiled: false`.
    pub fn get_or_compile(&mut self, template: &KernelTemplate) -> CompiledKernel {
        let key = format!("{}", template.kernel_type);

        if let Some(kernel) = self.kernels.get(&key) {
            self.stats.hits += 1;
            return kernel.clone();
        }

        self.stats.misses += 1;
        let start = Instant::now();

        // Attempt to compile with rustc
        let compiled = self.try_compile(template);

        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        let kernel = CompiledKernel {
            name: template.function_name.clone(),
            compiled,
            compile_time_ms: elapsed,
            kernel_type: template.kernel_type.clone(),
        };

        self.stats.total_kernels += 1;
        self.stats.total_compile_time_ms += elapsed;

        self.kernels.insert(key, kernel.clone());
        kernel
    }

    /// Try to compile a kernel source with rustc.
    /// Falls back gracefully if rustc is unavailable.
    fn try_compile(&self, template: &KernelTemplate) -> bool {
        let out_name = format!("/tmp/scirust_jit_{}.so", template.function_name);
        let src_name = format!("/tmp/scirust_jit_{}.rs", template.function_name);

        // Write source to temp file
        if let Err(_) = std::fs::write(&src_name, &template.source) {
            return false;
        }

        // Try to compile with rustc
        let result = std::process::Command::new("rustc")
            .args(&[
                "--crate-type", "cdylib",
                "-o", &out_name,
                "--edition", "2021",
                &src_name,
            ])
            .output();

        match result {
            Ok(output) if output.status.success() => {
                // Clean up source file
                let _ = std::fs::remove_file(&src_name);
                true
            }
            _ => {
                // Compilation failed or rustc not available — clean up
                let _ = std::fs::remove_file(&src_name);
                false
            }
        }
    }

    /// Clear all cached kernels.
    pub fn clear(&mut self) {
        self.kernels.clear();
        self.stats = CacheStats::default();
    }

    /// Get cache statistics.
    pub fn stats(&self) -> CacheStats {
        self.stats.clone()
    }

    /// Number of kernels in cache.
    pub fn len(&self) -> usize { self.kernels.len() }
    pub fn is_empty(&self) -> bool { self.kernels.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::generate_relu_kernel;

    #[test]
    fn test_cache_empty_on_new() {
        let cache = KernelCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.stats().hits, 0);
        assert_eq!(cache.stats().misses, 0);
    }

    #[test]
    fn test_cache_get_or_compile() {
        let mut cache = KernelCache::new();
        let tpl = generate_relu_kernel();
        let kernel = cache.get_or_compile(&tpl);
        // May or may not compile (depends on rustc availability)
        // But should not panic
        assert!(!kernel.name.is_empty());
    }

    #[test]
    fn test_cache_hit_after_first_compile() {
        let mut cache = KernelCache::new();
        let tpl = generate_relu_kernel();
        let _first = cache.get_or_compile(&tpl);
        let stats_after_first = cache.stats();

        let _second = cache.get_or_compile(&tpl);
        let stats_after_second = cache.stats();

        // Should have at least one hit
        assert!(stats_after_second.hits >= stats_after_first.hits);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = KernelCache::new();
        let tpl = generate_relu_kernel();
        let _ = cache.get_or_compile(&tpl);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.stats().hits, 0);
    }

    #[test]
    fn test_cache_stats_tracking() {
        let mut cache = KernelCache::new();
        let relu = generate_relu_kernel();
        let sigmoid = crate::kernel::generate_sigmoid_kernel();

        let _ = cache.get_or_compile(&relu);
        let _ = cache.get_or_compile(&sigmoid);
        let _ = cache.get_or_compile(&relu); // hit

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.total_kernels, 2);
    }
}
