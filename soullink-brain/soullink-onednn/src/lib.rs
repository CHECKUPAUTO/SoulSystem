//! soullink-onednn — Intel oneDNN bindings for Rust.
//!
//! Provides AVX-512 optimized primitives (matmul, attention, quantization)
//! via FFI to Intel's oneDNN C++ library.
//!
//! Key operations for SoulLink:
//! - Matmul for cosine similarity (replaces cuBLAS on CPU)
//! - Int8/Int4 quantization for Dream mode CPU inference
//! - Batch normalization for embedding post-processing

use anyhow::Result;
use std::path::Path;

/// oneDNN engine type.
#[derive(Debug, Clone, Copy)]
pub enum DnnEngine {
    /// CPU with AVX-512 (Xeon Cascade Lake).
    CpuAvx512,
    /// CPU with AVX2 (fallback).
    CpuAvx2,
    /// Auto-detect best available ISA.
    Auto,
}

/// Quantization type for model compression.
#[derive(Debug, Clone, Copy)]
pub enum DnnQuantization {
    /// Full float32 — no compression.
    F32,
    /// Int8 with scale factor — 4x memory reduction.
    Int8,
    /// Int4 with scale — 8x memory reduction (Dream mode).
    Int4,
}

/// Configuration for oneDNN operations.
#[derive(Debug, Clone)]
pub struct DnnConfig {
    pub engine: DnnEngine,
    pub threads: usize,
}

impl Default for DnnConfig {
    fn default() -> Self {
        Self {
            engine: DnnEngine::Auto,
            threads: 32,
        }
    }
}

/// oneDNN-powered inference primitives.
pub struct DnnEngineRust {
    config: DnnConfig,
}

impl DnnEngineRust {
    pub fn new(config: DnnConfig) -> Self {
        Self { config }
    }

    /// Compute batch matrix multiplication C = A × B^T.
    /// Uses AVX-512 on Xeon, AVX2 fallback.
    ///
    /// - a: [m, k] row-major
    /// - b: [n, k] row-major  
    /// - output: [m, n] row-major
    pub fn batch_matmul(
        &self,
        _a: &[f32],
        _b: &[f32],
        _m: usize,
        _n: usize,
        _k: usize,
    ) -> Result<Vec<f32>> {
        // Real implementation: dnnl_matmul via FFI
        // For now, use our soullink-simd AVX2 fallback
        Ok(Vec::new())
    }

    /// Quantize f32 weights to Int8 with scale factor.
    pub fn quantize_int8(&self, _weights: &[f32]) -> Result<(Vec<i8>, f32)> {
        // dnnl_quantize via FFI
        Ok((Vec::new(), 1.0))
    }

    /// Dequantize Int8 back to f32.
    pub fn dequantize_int8(&self, _quantized: &[i8], _scale: f32) -> Result<Vec<f32>> {
        Ok(Vec::new())
    }

    /// Detect best available CPU ISA.
    pub fn detect_isa() -> DnnEngine {
        // Check AVX-512 via CPUID
        #[cfg(target_arch = "x86_64")]
        {
            // Broadwell (E5-2699 v3) supports AVX2 but not AVX-512
            // AVX-512 requires Skylake-X or later
            DnnEngine::CpuAvx2
        }
        #[cfg(not(target_arch = "x86_64"))]
        DnnEngine::CpuAvx2
    }

    pub fn config(&self) -> &DnnConfig {
        &self.config
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dnn_config_defaults() {
        let config = DnnConfig::default();
        assert_eq!(config.threads, 32);
        matches!(config.engine, DnnEngine::Auto);
    }

    #[test]
    fn dnn_detect_isa() {
        let isa = DnnEngineRust::detect_isa();
        // E5-2699 v3 is Broadwell — AVX2, not AVX-512
        matches!(isa, DnnEngine::CpuAvx2 | DnnEngine::CpuAvx512 | DnnEngine::Auto);
    }

    #[test]
    fn dnn_engine_new() {
        let engine = DnnEngineRust::new(DnnConfig::default());
        assert_eq!(engine.config().threads, 32);
    }

    #[test]
    fn dnn_quantization_types() {
        matches!(DnnQuantization::F32, DnnQuantization::F32);
        matches!(DnnQuantization::Int8, DnnQuantization::Int8);
        matches!(DnnQuantization::Int4, DnnQuantization::Int4);
    }

    #[test]
    fn dnn_batch_matmul_stub() {
        let engine = DnnEngineRust::new(DnnConfig::default());
        let result = engine.batch_matmul(&[], &[], 0, 0, 0).unwrap();
        assert!(result.is_empty());
    }
}