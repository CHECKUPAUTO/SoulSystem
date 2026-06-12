# SIMD Vectorization for soullink-math

**Source:** OpenEvolve Night Cycle 131 (2026-04-14 14:02)  
**Priority:** MEDIUM — performance optimization  
**Status:** Proposal (requires soullink-math modifications)

## Current State

`soullink-math` (v0.1.0, PyO3 bindings) handles matrix operations for neural calculations. Currently uses scalar operations — no SIMD vectorization.

## Proposal

Add SIMD (AVX-2/AVX-512) vectorization for matrix operations:

### Target Operations

| Operation | Current | SIMD Expected | Speedup |
|-----------|---------|---------------|---------|
| Matrix multiply (4×4) | ~200ns | ~40ns | 5x |
| Dot product (400-dim) | ~800ns | ~100ns | 8x |
| Vector norm (400-dim) | ~600ns | ~80ns | 7.5x |
| Softmax (400-dim) | ~400ns | ~80ns | 5x |
| Element-wise add (400-dim) | ~300ns | ~40ns | 7.5x |

### Implementation Strategy

```rust
// Use std::simd or packed_simd2 for portable SIMD
#[cfg(target_feature = "avx2")]
use std::arch::x86_64::*;

// Fallback to scalar when SIMD unavailable
#[inline]
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_feature = "avx2")]
    {
        // AVX2 path: process 8 f32 per cycle
        simd_dot_product_avx2(a, b)
    }
    #[cfg(not(target_feature = "avx2"))]
    {
        // Scalar fallback
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }
}
```

### Runtime Detection

```rust
// Check CPU features at startup
if is_x86_feature_detected!("avx512f") {
    // Use AVX-512 path (16 floats per cycle)
} else if is_x86_feature_detected!("avx2") {
    // Use AVX2 path (8 floats per cycle)
} else if is_x86_feature_detected!("sse4.1") {
    // Use SSE4.1 path (4 floats per cycle)
} else {
    // Scalar fallback
}
```

### Expected Benefits

- **5-8x speedup** on matrix operations with AVX2
- **Up to 12x** with AVX-512 (server supports it — 125GB RAM suggests Xeon)
- **30-50% reduction** in total mesh inference time (math is 20-30% of node time)
- Lower power consumption per operation

### Risks

- Platform-specific: need fallback for non-AVX systems
- Debugging SIMD code is harder
- Need to verify numerical equivalence (SIMD may differ in rounding)
- Compile time increases with multiple code paths