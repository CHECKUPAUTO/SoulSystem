//! Fully-connected (Linear) layer: `y = x @ W^T + b`

use crate::layers::Layer;
use crate::tensor;
use crate::tensor::Tensor;

/// A fully-connected (dense) layer.
pub struct Linear {
    pub weight: Tensor<f32>,
    pub bias: Option<Tensor<f32>>,
    in_features: usize,
    out_features: usize,
}

impl Linear {
    /// Create a new Linear layer with Kaiming uniform initialization.
    pub fn new(in_features: usize, out_features: usize, bias: bool) -> Self {
        let bound = (6.0 / in_features as f32).sqrt();
        let mut weight = Tensor::<f32>::rand(&[out_features, in_features]);
        // Scale to [-bound, bound]
        for i in 0..weight.len() {
            *weight.get_flat_mut(i) = weight.get_flat(i) * 2.0 * bound - bound;
        }

        let bias_tensor = if bias {
            Some(Tensor::<f32>::zeros(&[out_features]))
        } else {
            None
        };

        Self {
            weight,
            bias: bias_tensor,
            in_features,
            out_features,
        }
    }

    pub fn from_parts(weight: Tensor<f32>, bias: Option<Tensor<f32>>) -> Result<Self, String> {
        if weight.shape().len() != 2 {
            return Err("Linear weight must be 2D".to_string());
        }
        let out_features = weight.shape()[0];
        let in_features = weight.shape()[1];
        if let Some(ref b) = bias {
            if b.shape() != &[out_features] {
                return Err(format!(
                    "Bias shape {:?} doesn't match out_features {}",
                    b.shape(), out_features
                ));
            }
        }
        Ok(Self { weight, bias, in_features, out_features })
    }

    pub fn in_features(&self) -> usize { self.in_features }
    pub fn out_features(&self) -> usize { self.out_features }
}

impl Layer for Linear {
    fn forward(&self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        let input_2d = if input.shape().len() == 1 {
            input.reshape(&[1, input.shape()[0]])?
        } else if input.shape().len() == 2 {
            input.clone()
        } else {
            return Err(format!("Linear expects 1D or 2D input, got {:?}", input.shape()));
        };

        let batch = input_2d.shape()[0];
        let _in_f = input_2d.shape()[1];

        if _in_f != self.in_features {
            return Err(format!(
                "Input features {} don't match layer in_features {}",
                _in_f, self.in_features
            ));
        }

        // y = x @ W^T
        let wt = self.weight.transpose()?;
        let mut y = tensor::matmul_f32(&input_2d, &wt)?;

        if let Some(ref b) = self.bias {
            for i in 0..batch {
                for j in 0..self.out_features {
                    *y.get_mut_2d(i, j).unwrap() += b.data()[j];
                }
            }
        }

        if input.shape().len() == 1 {
            y = y.reshape(&[self.out_features])?;
        }

        Ok(y)
    }

    fn parameters(&self) -> Vec<&Tensor<f32>> {
        let mut params = vec![&self.weight];
        if let Some(ref b) = self.bias {
            params.push(b);
        }
        params
    }

    fn parameters_mut(&mut self) -> Vec<&mut Tensor<f32>> {
        let mut params = vec![&mut self.weight];
        if let Some(ref mut b) = self.bias {
            params.push(b);
        }
        params
    }

    fn name(&self) -> &str { "Linear" }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_forward() {
        let layer = Linear::new(4, 3, true);
        let input = Tensor::<f32>::ones(&[2, 4]);
        let output = layer.forward(&input).unwrap();
        assert_eq!(output.shape(), &[2, 3]);
    }

    #[test]
    fn test_linear_1d_input() {
        let layer = Linear::new(4, 3, false);
        let input = Tensor::<f32>::ones(&[4]);
        let output = layer.forward(&input).unwrap();
        assert_eq!(output.shape(), &[3]);
    }
}
