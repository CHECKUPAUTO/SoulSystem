//! 2D Convolution layer for inference.

use crate::layers::Layer;
use crate::tensor::Tensor;

/// 2D Convolution layer.
pub struct Conv2d {
    pub weight: Tensor<f32>,
    pub bias: Option<Tensor<f32>>,
    in_channels: usize,
    out_channels: usize,
    pub kernel_size: (usize, usize),
    pub stride: (usize, usize),
    pub padding: (usize, usize),
}

impl Conv2d {
    pub fn new(
        in_channels: usize, out_channels: usize,
        kernel_size: usize, stride: usize, padding: usize, bias: bool,
    ) -> Self {
        let fan_in = in_channels * kernel_size * kernel_size;
        let bound = (6.0 / fan_in as f32).sqrt();
        let mut weight = Tensor::<f32>::rand(&[out_channels, in_channels, kernel_size, kernel_size]);
        for i in 0..weight.len() {
            *weight.get_flat_mut(i) = weight.get_flat(i) * 2.0 * bound - bound;
        }

        let bias_tensor = if bias { Some(Tensor::<f32>::zeros(&[out_channels])) } else { None };

        Self {
            weight, bias: bias_tensor,
            in_channels, out_channels,
            kernel_size: (kernel_size, kernel_size),
            stride: (stride, stride),
            padding: (padding, padding),
        }
    }

    fn output_size(&self, h_in: usize, w_in: usize) -> (usize, usize) {
        let h_out = ((h_in + 2 * self.padding.0 - self.kernel_size.0) / self.stride.0) + 1;
        let w_out = ((w_in + 2 * self.padding.1 - self.kernel_size.1) / self.stride.1) + 1;
        (h_out, w_out)
    }
}

impl Conv2d {
    /// Forward pass with fused activation (avoid temp tensor allocation).
    /// Compute gradients for backpropagation.
    ///
    /// Given the upstream gradient `dout` (same shape as output),
    /// computes gradients w.r.t. weight and bias.
    /// Returns (d_weight, d_bias) where d_bias may be None if bias is None.
    pub fn backward(&self, input: &Tensor<f32>, dout: &Tensor<f32>) -> Result<(Tensor<f32>, Option<Tensor<f32>>), String> {
        if input.shape().len() != 4 || dout.shape().len() != 4 {
            return Err("Conv2d backward expects 4D tensors".to_string());
        }
        let batch = input.shape()[0];
        let in_c = input.shape()[1];
        let h_in = input.shape()[2];
        let w_in = input.shape()[3];
        let out_c = self.out_channels;
        let k_h = self.kernel_size.0;
        let k_w = self.kernel_size.1;
        let p_h = self.padding.0;
        let p_w = self.padding.1;
        let s_h = self.stride.0;
        let s_w = self.stride.1;
        let h_out = dout.shape()[2];
        let w_out = dout.shape()[3];

        // Gradient w.r.t. weight: im2col style
        let mut d_weight_data = vec![0.0f32; out_c * in_c * k_h * k_w];

        for oc in 0..out_c {
            for ic in 0..in_c {
                for kh in 0..k_h {
                    for kw in 0..k_w {
                        let mut grad = 0.0f32;
                        for b in 0..batch {
                            for oh in 0..h_out {
                                for ow in 0..w_out {
                                    let ih = (oh * s_h + kh).wrapping_sub(p_h);
                                    let iw = (ow * s_w + kw).wrapping_sub(p_w);
                                    if ih < h_in && iw < w_in {
                                        grad += *input.get(&[b, ic, ih, iw]).unwrap()
                                              * dout.get(&[b, oc, oh, ow]).unwrap();
                                    }
                                }
                            }
                        }
                        let idx = ((oc * in_c + ic) * k_h + kh) * k_w + kw;
                        d_weight_data[idx] = grad;
                    }
                }
            }
        }

        let d_weight = Tensor::<f32>::from_vec(d_weight_data, &[out_c, in_c, k_h, k_w])
            .map_err(|e| format!("d_weight: {}", e))?;

        // Gradient w.r.t. bias
        let d_bias = if self.bias.is_some() {
            let mut d_bias_data = vec![0.0f32; out_c];
            for oc in 0..out_c {
                let mut grad = 0.0f32;
                for b in 0..batch {
                    for oh in 0..h_out {
                        for ow in 0..w_out {
                            grad += dout.get(&[b, oc, oh, ow]).unwrap();
                        }
                    }
                }
                d_bias_data[oc] = grad;
            }
            Some(Tensor::<f32>::from_vec(d_bias_data, &[out_c])
                .map_err(|e| format!("d_bias: {}", e))?)
        } else {
            None
        };

        Ok((d_weight, d_bias))
    }

    pub fn forward_fused(&self, input: &Tensor<f32>, activation: crate::layers::ActivationType) -> Result<Tensor<f32>, String> {
        let mut output = self.forward(input)?;
        match activation {
            crate::layers::ActivationType::ReLU => {
                for i in 0..output.len() {
                    let v = *output.get_flat(i);
                    if v < 0.0 { *output.get_flat_mut(i) = 0.0; }
                }
            }
            crate::layers::ActivationType::GELU => {
                const SQRT_2_OVER_PI: f32 = 0.7978845608028654;
                const GELU_CONST: f32 = 0.044715;
                for i in 0..output.len() {
                    let x = *output.get_flat(i);
                    let x3 = x * x * x;
                    *output.get_flat_mut(i) = 0.5 * x * (1.0 + (SQRT_2_OVER_PI * (x + GELU_CONST * x3)).tanh());
                }
            }
            crate::layers::ActivationType::Sigmoid => {
                for i in 0..output.len() {
                    let x = *output.get_flat(i);
                    *output.get_flat_mut(i) = 1.0 / (1.0 + (-x).exp());
                }
            }
            crate::layers::ActivationType::Tanh => {
                for i in 0..output.len() {
                    *output.get_flat_mut(i) = output.get_flat(i).tanh();
                }
            }
            crate::layers::ActivationType::None => {}
        }
        Ok(output)
    }
}

impl Layer for Conv2d {
    fn forward(&self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        if input.shape().len() != 4 {
            return Err(format!("Conv2d expects 4D input [B,C,H,W], got {:?}", input.shape()));
        }

        let batch = input.shape()[0];
        let in_c = input.shape()[1];
        let h_in = input.shape()[2];
        let w_in = input.shape()[3];

        if in_c != self.in_channels {
            return Err(format!("Input channels {} don't match layer in_channels {}", in_c, self.in_channels));
        }

        let (h_out, w_out) = self.output_size(h_in, w_in);
        let out_c = self.out_channels;
        let k_h = self.kernel_size.0;
        let k_w = self.kernel_size.1;
        let p_h = self.padding.0;
        let p_w = self.padding.1;
        let s_h = self.stride.0;
        let s_w = self.stride.1;

        let mut output = Tensor::<f32>::zeros(&[batch, out_c, h_out, w_out]);

        for b in 0..batch {
            for oc in 0..out_c {
                for oh in 0..h_out {
                    for ow in 0..w_out {
                        let mut sum = 0.0f32;
                        for ic in 0..in_c {
                            for kh in 0..k_h {
                                for kw in 0..k_w {
                                    let ih = (oh * s_h + kh).wrapping_sub(p_h);
                                    let iw = (ow * s_w + kw).wrapping_sub(p_w);
                                    if ih < h_in && iw < w_in {
                                        sum += *input.get(&[b, ic, ih, iw]).unwrap()
                                             * self.weight.get(&[oc, ic, kh, kw]).unwrap();
                                    }
                                }
                            }
                        }
                        if let Some(ref bias) = self.bias {
                            sum += bias.data()[oc];
                        }
                        *output.get_mut(&[b, oc, oh, ow]).unwrap() = sum;
                    }
                }
            }
        }
        Ok(output)
    }

    fn parameters(&self) -> Vec<&Tensor<f32>> {
        let mut params = vec![&self.weight];
        if let Some(ref b) = self.bias { params.push(b); }
        params
    }

    fn parameters_mut(&mut self) -> Vec<&mut Tensor<f32>> {
        let mut params = vec![&mut self.weight];
        if let Some(ref mut b) = self.bias { params.push(b); }
        params
    }

    fn name(&self) -> &str { "Conv2d" }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conv2d_basic() {
        let conv = Conv2d::new(1, 2, 3, 1, 0, false);
        let input = Tensor::<f32>::ones(&[1, 1, 5, 5]);
        let output = conv.forward(&input).unwrap();
        assert_eq!(output.shape(), &[1, 2, 3, 3]);
    }

    #[test]
    fn test_conv2d_backward() {
        let conv = Conv2d::new(1, 1, 3, 1, 0, true);
        let input = Tensor::<f32>::ones(&[1, 1, 4, 4]);
        let output = conv.forward(&input).unwrap();
        // Upstream gradient: all ones
        let dout = Tensor::<f32>::ones(output.shape());
        let (d_weight, d_bias) = conv.backward(&input, &dout).unwrap();
        assert_eq!(d_weight.shape(), conv.weight.shape());
        assert!(d_bias.is_some());
        // Bias gradient should be spatial_size * batch = 2*2*1 = 4 for each output channel (1)
        if let Some(ref db) = d_bias {
            assert!((db.data()[0] - 4.0).abs() < 1e-5, "d_bias[0]={}", db.data()[0]);
        }
    }

    #[test]
    fn test_conv2d_with_padding() {
        let conv = Conv2d::new(1, 1, 3, 1, 1, false);
        let input = Tensor::<f32>::ones(&[1, 1, 5, 5]);
        let output = conv.forward(&input).unwrap();
        assert_eq!(output.shape(), &[1, 1, 5, 5]);
    }
}
