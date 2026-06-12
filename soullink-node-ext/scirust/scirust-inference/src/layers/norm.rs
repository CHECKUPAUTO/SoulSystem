//! Normalization layers for inference.
//!
//! BatchNorm in inference mode uses running statistics (no gradient computation).

use crate::layers::Layer;
use crate::tensor::Tensor;

/// Batch Normalization (inference mode).
///
/// `y = gamma * (x - running_mean) / sqrt(running_var + epsilon) + beta`
///
/// Shape: `[num_features]` for both gamma/beta and running stats.
pub struct BatchNorm {
    pub gamma: Tensor<f32>,       // scale
    pub beta: Tensor<f32>,        // shift
    pub running_mean: Tensor<f32>,
    pub running_var: Tensor<f32>,
    pub epsilon: f32,
    num_features: usize,
}


impl BatchNorm {
    /// Create a new BatchNorm layer in inference mode.
    /// Gamma initialized to 1, beta to 0, running_mean=0, running_var=1.
    pub fn new(num_features: usize, epsilon: f32) -> Self {
        Self {
            gamma: Tensor::<f32>::ones(&[num_features]),
            beta: Tensor::<f32>::zeros(&[num_features]),
            running_mean: Tensor::<f32>::zeros(&[num_features]),
            running_var: Tensor::<f32>::ones(&[num_features]),
            epsilon,
            num_features,
        }
    }

    /// Create from pre-trained parameters.
    pub fn from_weights(
        gamma: Tensor<f32>,
        beta: Tensor<f32>,
        running_mean: Tensor<f32>,
        running_var: Tensor<f32>,
        epsilon: f32,
    ) -> Result<Self, String> {
        let nf = gamma.shape()[0];
        if beta.shape() != &[nf] || running_mean.shape() != &[nf] || running_var.shape() != &[nf] {
            return Err("BatchNorm: all parameter tensors must have same shape".to_string());
        }
        Ok(Self {
            gamma,
            beta,
            running_mean,
            running_var,
            epsilon,
            num_features: nf,
        })
    }

    /// Compute gradients w.r.t. gamma and beta.
    ///
    /// Given input and upstream gradient dout (same shape as output),
    /// returns (d_gamma, d_beta).
    pub fn backward(&self, input: &Tensor<f32>, dout: &Tensor<f32>) -> Result<(Tensor<f32>, Tensor<f32>), String> {
        if input.shape() != dout.shape() {
            return Err(format!("BatchNorm backward: shape mismatch {:?} vs {:?}", input.shape(), dout.shape()));
        }
        let batch = input.shape()[0];
        let num_features = input.shape()[1];
        let spatial: usize = if input.shape().len() > 2 { input.shape()[2..].iter().product() } else { 1 };
        let _norm_size = (batch * spatial) as f32;

        let mut d_gamma_data = vec![0.0f32; num_features];
        let mut d_beta_data = vec![0.0f32; num_features];

        let data = input.data();
        let dout_data = dout.data();

        for c in 0..num_features {
            let mu = self.running_mean.data()[c];
            let var = self.running_var.data()[c];
            let inv_std = 1.0 / (var + self.epsilon).sqrt();

            let mut d_gamma = 0.0f32;
            let mut d_beta = 0.0f32;

            for n in 0..batch {
                for s in 0..spatial {
                    let idx = n * num_features * spatial + c * spatial + s;
                    let x_hat = (data[idx] - mu) * inv_std;
                    d_gamma += dout_data[idx] * x_hat;
                    d_beta += dout_data[idx];
                }
            }

            d_gamma_data[c] = d_gamma;
            d_beta_data[c] = d_beta;
        }

        let d_gamma = Tensor::<f32>::from_vec(d_gamma_data, &[num_features])
            .map_err(|e| format!("d_gamma: {}", e))?;
        let d_beta = Tensor::<f32>::from_vec(d_beta_data, &[num_features])
            .map_err(|e| format!("d_beta: {}", e))?;

        Ok((d_gamma, d_beta))
    }
}

impl Layer for BatchNorm {
    fn forward(&self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        // Input: [batch, num_features, ...] — C channel is axis 1 for NCHW
        // For 2D input [B, C], it's straightforward.
        // We normalize over the batch dimension for each feature.
        if input.shape().len() < 2 {
            return Err(format!(
                "BatchNorm expects at least 2D input [B, C], got {:?}",
                input.shape()
            ));
        }

        let batch = input.shape()[0];
        let num_features = input.shape()[1];

        if num_features != self.num_features {
            return Err(format!(
                "BatchNorm: expected {} features, got {}",
                self.num_features, num_features
            ));
        }

        // Compute spatial dimensions product for multi-dim input
        let spatial: usize = if input.shape().len() > 2 {
            input.shape()[2..].iter().product()
        } else {
            1
        };

        let data = input.data();
        let mut out = vec![0.0f32; input.len()];

        for c in 0..num_features {
            let mu = self.running_mean.data()[c];
            let var = self.running_var.data()[c];
            let inv_std = 1.0 / (var + self.epsilon).sqrt();
            let g = self.gamma.data()[c];
            let b = self.beta.data()[c];

            for n in 0..batch {
                for s in 0..spatial {
                    let idx = n * num_features * spatial + c * spatial + s;
                    out[idx] = g * (data[idx] - mu) * inv_std + b;
                }
            }
        }

        Tensor::from_vec(out, input.shape())
    }

    fn parameters(&self) -> Vec<&Tensor<f32>> {
        vec![&self.gamma, &self.beta]
    }

    fn parameters_mut(&mut self) -> Vec<&mut Tensor<f32>> {
        vec![&mut self.gamma, &mut self.beta]
    }

    fn name(&self) -> &str {
        "BatchNorm"
    }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batchnorm_forward() {
        let bn = BatchNorm::new(4, 1e-5);
        let input = Tensor::<f32>::ones(&[2, 4]);
        let output = bn.forward(&input).unwrap();
        assert_eq!(output.shape(), &[2, 4]);
    }

    #[test]
    fn test_batchnorm_backward() {
        let bn = BatchNorm::new(2, 1e-5);
        let input = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        let output = bn.forward(&input).unwrap();
        let dout = Tensor::<f32>::ones(output.shape());
        let (d_gamma, d_beta) = bn.backward(&input, &dout).unwrap();
        assert_eq!(d_gamma.shape(), &[2]);
        assert_eq!(d_beta.shape(), &[2]);
        // d_beta = sum of dout = 2 (batch) for each feature
        assert!((d_beta.data()[0] - 2.0).abs() < 1e-5, "d_beta[0]={}", d_beta.data()[0]);
        assert!((d_beta.data()[1] - 2.0).abs() < 1e-5, "d_beta[1]={}", d_beta.data()[1]);
    }

    #[test]
    fn test_batchnorm_identity() {
        // With gamma=1, beta=0, mean=0, var=1, BN(x) = (x - 0)/1 * 1 + 0 = x
        let bn = BatchNorm::new(2, 1e-5);
        let input = Tensor::<f32>::from_vec(vec![1.0, 2.0], &[1, 2]).unwrap();
        let output = bn.forward(&input).unwrap();
        // running_mean=0, running_var=1, gamma=1, beta=0: output[i] = input[i]
        assert!((output.data()[0] - 1.0).abs() < 2e-5, "expected 1.0 got {}", output.data()[0]);
        assert!((output.data()[1] - 2.0).abs() < 2e-5, "expected 2.0 got {}", output.data()[1]);
    }
}
