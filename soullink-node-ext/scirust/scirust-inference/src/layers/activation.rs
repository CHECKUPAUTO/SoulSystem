//! Activation function layers.

use crate::layers::Layer;
use crate::tensor;

pub struct ReLU;
impl ReLU { pub fn new() -> Self { Self } }

impl Layer for ReLU {
    fn forward(&self, input: &tensor::Tensor<f32>) -> Result<tensor::Tensor<f32>, String> {
        Ok(tensor::relu_f32(input))
    }
    fn parameters(&self) -> Vec<&tensor::Tensor<f32>> { vec![] }
    fn parameters_mut(&mut self) -> Vec<&mut tensor::Tensor<f32>> { vec![] }
    fn name(&self) -> &str { "ReLU" }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

pub struct Sigmoid;
impl Sigmoid { pub fn new() -> Self { Self } }

impl Layer for Sigmoid {
    fn forward(&self, input: &tensor::Tensor<f32>) -> Result<tensor::Tensor<f32>, String> {
        Ok(tensor::sigmoid_f32(input))
    }
    fn parameters(&self) -> Vec<&tensor::Tensor<f32>> { vec![] }
    fn parameters_mut(&mut self) -> Vec<&mut tensor::Tensor<f32>> { vec![] }
    fn name(&self) -> &str { "Sigmoid" }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

pub struct Tanh;
impl Tanh { pub fn new() -> Self { Self } }

impl Layer for Tanh {
    fn forward(&self, input: &tensor::Tensor<f32>) -> Result<tensor::Tensor<f32>, String> {
        Ok(tensor::tanh_f32(input))
    }
    fn parameters(&self) -> Vec<&tensor::Tensor<f32>> { vec![] }
    fn parameters_mut(&mut self) -> Vec<&mut tensor::Tensor<f32>> { vec![] }
    fn name(&self) -> &str { "Tanh" }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

pub struct GELU;
impl GELU { pub fn new() -> Self { Self } }

impl Layer for GELU {
    fn forward(&self, input: &tensor::Tensor<f32>) -> Result<tensor::Tensor<f32>, String> {
        const SQRT_2_OVER_PI: f32 = 0.7978845608028654;
        const GELU_CONST: f32 = 0.044715;
        Ok(input.map(|&x| {
            let x3 = x * x * x;
            0.5 * x * (1.0 + (SQRT_2_OVER_PI * (x + GELU_CONST * x3)).tanh())
        }))
    }
    fn parameters(&self) -> Vec<&tensor::Tensor<f32>> { vec![] }
    fn parameters_mut(&mut self) -> Vec<&mut tensor::Tensor<f32>> { vec![] }
    fn name(&self) -> &str { "GELU" }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tensor::Tensor;

    #[test]
    fn test_relu() {
        let input = Tensor::<f32>::from_vec(vec![-2.0, -1.0, 0.0, 1.0, 2.0], &[1, 5]).unwrap();
        let output = ReLU::new().forward(&input).unwrap();
        assert_eq!(output.data(), &[0.0, 0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn test_sigmoid() {
        let input = Tensor::<f32>::scalar(0.0);
        let output = Sigmoid::new().forward(&input).unwrap();
        assert!((output.data()[0] - 0.5).abs() < 1e-5);
    }
}
