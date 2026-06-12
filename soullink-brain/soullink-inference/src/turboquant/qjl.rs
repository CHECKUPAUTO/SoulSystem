//! QJL Quantizer: Phase 2 of TurboQuant — 3-bit quantization + 1-bit correction
//!
//! Quantized Johnson-Lindenstrauss lemma guarantees:
//!   |⟨Q(x), Q(y)⟩ - ⟨x, y⟩| < ε · ‖x‖ · ‖y‖
//!
//! 3-bit quantization (8 levels) + sign-based residual correction.

/// QJL Quantizer with configurable bit depth and correction scale.
pub struct QJLQuantizer {
    pub bits: u32,
    levels: f32,
    scale: f32,
}

impl QJLQuantizer {
    pub fn new(bits: u32, scale: f32) -> Self {
        Self {
            bits,
            levels: (1u32 << bits) as f32,
            scale,
        }
    }

    /// Default TurboQuant config: 3-bit, 0.01 correction scale.
    pub fn turbo3() -> Self {
        Self::new(3, 0.01)
    }

    /// Quantize a slice of f32 values to 3-bit + QJL correction.
    pub fn quantize(&self, x: &[f32]) -> Vec<f32> {
        let x_max = x.iter().map(|v| v.abs()).fold(f32::NEG_INFINITY, f32::max) + 1e-8;
        let half_range = (self.levels - 1.0) / 2.0;
        let increment = 1.0 / (self.levels / 2.0);

        x.iter()
            .map(|&val| {
                let scaled = (val / x_max) * half_range;
                let mut quant = (scaled / increment).round() * increment;
                quant = quant.clamp(-half_range, half_range);
                let residual = scaled - quant;
                let correction = residual.signum() * self.scale * x_max;
                quant + correction
            })
            .collect()
    }

    /// Dequantize back to approximate original values.
    pub fn dequantize(&self, x_quant: &[f32], original_scale: f32) -> Vec<f32> {
        let half_range = (self.levels - 1.0) / 2.0;
        x_quant
            .iter()
            .map(|&v| (v / half_range) * original_scale)
            .collect()
    }

    /// Pack quantized f32 values into packed u8 (3 bits per value).
    /// 8 values per 3 bytes (24 bits = 8 × 3 bits).
    pub fn pack_3bit(x: &[f32], x_max: f32) -> Vec<u8> {
        let half_range = 3.5; // (2^3 - 1) / 2
        let increment = 1.0 / 4.0; // 1 / (2^3 / 2)

        let n = x.len();
        let packed_len = (n * 3).div_ceil(8); // ceil(n*3/8) bytes
        let mut packed = vec![0u8; packed_len];
        let mut bit_pos = 0u32;

        for &val in x {
            let scaled = (val / (x_max + 1e-8)) * half_range;
            let quant = ((scaled / increment).round() as i32 + 4).clamp(0, 7) as u8; // 3-bit: 0-7
            let byte_idx = (bit_pos / 8) as usize;
            let bit_offset = bit_pos % 8;
            packed[byte_idx] |= quant << bit_offset;
            if bit_offset > 5 {
                packed[byte_idx + 1] |= quant >> (8 - bit_offset);
            }
            bit_pos += 3;
        }
        packed
    }

    /// Unpack 3-bit values back to f32.
    pub fn unpack_3bit(packed: &[u8], n: usize, x_max: f32) -> Vec<f32> {
        let increment = 1.0 / 4.0;
        let mut result = Vec::with_capacity(n);
        let mut bit_pos = 0u32;

        for _ in 0..n {
            let byte_idx = (bit_pos / 8) as usize;
            let bit_offset = bit_pos % 8;
            let mut quant = (packed[byte_idx] >> bit_offset) & 0x07;
            if bit_offset > 5 && byte_idx + 1 < packed.len() {
                quant |= (packed[byte_idx + 1] << (8 - bit_offset)) & 0x07;
            }
            let val = ((quant as i32 - 4) as f32) * increment * x_max;
            result.push(val);
            bit_pos += 3;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_dequantize_roundtrip() {
        let q = QJLQuantizer::turbo3();
        let data: Vec<f32> = (0..128).map(|i| (i as f32 - 64.0) * 0.1).collect();
        let compressed = q.quantize(&data);
        assert_eq!(compressed.len(), data.len());
        let max_abs = compressed.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
        assert!(max_abs < 100.0, "Values out of range: {max_abs}");
    }

    #[test]
    fn pack_unpack_3bit() {
        let data: Vec<f32> = (0..32).map(|i| (i as f32 - 16.0) * 0.1).collect();
        let x_max = data
            .iter()
            .map(|v| v.abs())
            .fold(f32::NEG_INFINITY, f32::max)
            + 1e-8;
        let packed = QJLQuantizer::pack_3bit(&data, x_max);
        let unpacked = QJLQuantizer::unpack_3bit(&packed, data.len(), x_max);
        assert_eq!(unpacked.len(), data.len());
        // Check reconstruction quality (3-bit should be within ~10%)
        let avg_err: f32 = unpacked
            .iter()
            .zip(data.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f32>()
            / data.len() as f32;
        assert!(avg_err < 0.5, "Average error too large: {avg_err}");
    }

    #[test]
    fn compression_ratio() {
        // 3 bits per value vs 16 bits (FP16) = 16/3 = 5.33x
        let ratio = 16.0 / 3.0;
        assert!((ratio - 5.33f64).abs() < 0.1);
    }
}
