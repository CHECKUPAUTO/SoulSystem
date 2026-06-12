//! Quantization support for inference.

use crate::tensor::Tensor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantMode {
    None,
    Float16,
    Int8,
}

// ---------------------------------------------------------------------------
// FP16 quantization
// ---------------------------------------------------------------------------

pub fn f32_to_f16_bits(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = (bits >> 23) & 0xff;
    let mant = bits & 0x7fffff;

    if exp == 0 {
        return sign;
    }
    if exp == 0xff {
        if mant == 0 {
            return sign | 0x7c00u16;
        } else {
            return sign | 0x7c00u16 | (mant >> 13) as u16;
        }
    }

    let exp_i = exp as i32 - 127 + 15;
    if exp_i >= 31 {
        return sign | 0x7c00u16;
    }
    if exp_i <= 0 {
        if exp_i <= -10 {
            return sign;
        }
        let sub_mant = ((mant | 0x800000) >> (14 - exp_i)) as u16;
        return sign | sub_mant;
    }

    let f16_mant = (mant >> 13) as u16;
    let f16_exp = (exp_i as u16) << 10;
    sign | f16_exp | f16_mant
}

pub fn f16_bits_to_f32(bits: u16) -> f32 {
    let sign = ((bits >> 15) as u32) << 31;
    let exp = (bits >> 10) & 0x1f;
    let mant = bits & 0x3ff;

    if exp == 0 {
        if mant == 0 {
            return f32::from_bits(sign);
        }
        let exp_val = 127 - 15;
        let mant_val = (mant as u32) << 13;
        return f32::from_bits(sign | (exp_val << 23) | mant_val);
    }
    if exp == 31 {
        let exp_val = 0xffu32;
        let mant_val = (mant as u32) << 13;
        if mant == 0 {
            return f32::from_bits(sign | (exp_val << 23));
        } else {
            return f32::from_bits(sign | (exp_val << 23) | mant_val | 0x400000);
        }
    }

    let exp_val = (exp as u32 + (127 - 15)) << 23;
    let mant_val = (mant as u32) << 13;
    f32::from_bits(sign | exp_val | mant_val)
}

// ---------------------------------------------------------------------------
// INT8 quantization (asymmetric, unsigned)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct QuantParams {
    pub scale: f32,
    pub zero_point: i16,
}

impl QuantParams {
    pub fn from_minmax(min_val: f32, max_val: f32) -> Self {
        let scale = (max_val - min_val) / 255.0;
        let zero_point: i16 = if scale > 0.0 {
            (-min_val / scale).round() as i16
        } else {
            0
        };
        Self { scale, zero_point }
    }

    pub fn quantize(&self, x: f32) -> u8 {
        let q = (x / self.scale) + (self.zero_point as f32);
        (q.round() as i16).clamp(0, 255) as u8
    }

    pub fn dequantize(&self, q: u8) -> f32 {
        (q as f32 - self.zero_point as f32) * self.scale
    }
}

/// Per-tensor INT8 quantization.
pub fn quantize_int8(t: &Tensor<f32>, params: &QuantParams) -> Vec<u8> {
    t.data().iter().map(|&x| params.quantize(x)).collect()
}

/// Dequantize INT8 back to f32.
pub fn dequantize_int8(quantized: &[u8], params: &QuantParams) -> Vec<f32> {
    quantized.iter().map(|&q| params.dequantize(q)).collect()
}

/// Per-channel INT8 quantization (useful for weights).
pub fn quantize_int8_per_channel(
    weight: &Tensor<f32>,
    channel_axis: usize,
) -> (Vec<Vec<u8>>, Vec<QuantParams>) {
    let shape = weight.shape();
    let num_channels = shape[channel_axis];
    let inner: usize = shape.iter().product::<usize>() / num_channels;
    let mut quantized: Vec<Vec<u8>> = Vec::with_capacity(num_channels);
    let mut params = Vec::with_capacity(num_channels);

    for c in 0..num_channels {
        let mut min_val = f32::MAX;
        let mut max_val = f32::MIN;
        for i in 0..inner {
            let idx = if channel_axis == 0 {
                c * inner + i
            } else {
                i * num_channels + c
            };
            let v = weight.data()[idx];
            min_val = min_val.min(v);
            max_val = max_val.max(v);
        }
        let p = QuantParams::from_minmax(min_val, max_val);
        let mut q_ch = Vec::with_capacity(inner);
        for i in 0..inner {
            let idx = if channel_axis == 0 {
                c * inner + i
            } else {
                i * num_channels + c
            };
            q_ch.push(p.quantize(weight.data()[idx]));
        }
        quantized.push(q_ch);
        params.push(p);
    }
    (quantized, params)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f16_roundtrip() {
        let values = [0.0f32, 1.0, -1.0, 3.14159, 0.0001, 65504.0, -65504.0];
        for &v in &values {
            let bits = f32_to_f16_bits(v);
            let back = f16_bits_to_f32(bits);
            let expected = if v.abs() < 1e-5 { 0.0 } else { v };
            assert!(
                (back - expected).abs() / (expected.abs().max(1.0)) < 1e-2,
                "f16 roundtrip failed for {}: got {}",
                v,
                back
            );
        }
    }

    #[test]
    fn test_int8_quantize_dequantize() {
        let data = Tensor::<f32>::from_vec(vec![-1.0, -0.5, 0.0, 0.5, 1.0], &[5]).unwrap();
        let params = QuantParams::from_minmax(-1.0, 1.0);
        let q = quantize_int8(&data, &params);
        let dq = dequantize_int8(&q, &params);
        for i in 0..data.len() {
            let err = (dq[i] - data.data()[i]).abs();
            assert!(
                err < 0.05,
                "INT8 error at {}: {} vs {} (err={})",
                i, dq[i], data.data()[i], err
            );
        }
    }

    #[test]
    fn test_per_channel_quant() {
        let w = Tensor::<f32>::rand(&[4, 16]);
        let (_q, _params) = quantize_int8_per_channel(&w, 0);
        assert_eq!(_q.len(), 4);
    }
}
