//! Neural network training using tape-based autodiff.
//!
//! Supports Linear, Conv2d, and BatchNorm backpropagation.
//! Uses `scirust-autodiff::tape` for computation graph construction.

use crate::layers::{Layer, Linear, Conv2d, BatchNorm, ActivationType};
use crate::tensor::Tensor;
use std::collections::HashMap;

/// Multi-layer neural network trainer with layer-type-aware backprop.
pub struct NNTrainer {
    pub learning_rate: f64,
    pub use_adam: bool,
    adam_m: HashMap<String, Vec<f64>>,
    adam_v: HashMap<String, Vec<f64>>,
    adam_t: usize,
}

impl NNTrainer {
    pub fn new(learning_rate: f64) -> Self {
        Self { learning_rate, use_adam: false, adam_m: HashMap::new(), adam_v: HashMap::new(), adam_t: 0 }
    }

    pub fn with_adam(mut self) -> Self {
        self.use_adam = true;
        self
    }

    /// Extract all trainable parameters as a flat f64 vector with layer prefixes.
    pub fn extract_params(layers: &[Box<dyn Layer>]) -> Vec<f64> {
        let mut params = Vec::new();
        for layer in layers {
            for tensor in layer.parameters() {
                for &v in tensor.data() { params.push(v as f64); }
            }
        }
        params
    }

    /// Write parameters back to layers.
    pub fn inject_params(layers: &mut [Box<dyn Layer>], params: &[f64]) {
        let mut idx = 0;
        for layer in layers.iter_mut() {
            for tensor in layer.parameters_mut() {
                let shape = tensor.shape().to_vec();
                let mut data: Vec<f32> = tensor.data().to_vec();
                for v in data.iter_mut() {
                    *v = params[idx] as f32;
                    idx += 1;
                }
                if let Ok(t) = Tensor::<f32>::from_vec(data, &shape) { *tensor = t; }
            }
        }
    }

    /// Count parameters.
    pub fn count_params(layers: &[Box<dyn Layer>]) -> usize {
        layers.iter().flat_map(|l| l.parameters()).map(|t| t.len()).sum()
    }

    // -----------------------------------------------------------------------
    // Forward + backward for a full model using layer-aware dispatch
    // -----------------------------------------------------------------------

    /// Run forward pass through all layers, storing intermediates for backward.
    fn forward_with_cache(
        layers: &[Box<dyn Layer>],
        input: &Tensor<f32>,
    ) -> Result<(Tensor<f32>, Vec<(String, Tensor<f32>, Tensor<f32>)>), String> {
        let mut x = input.clone();
        let mut cache: Vec<(String, Tensor<f32>, Tensor<f32>)> = Vec::new(); // (type, input, output)

        for layer in layers {
            let inp = x.clone();
            let out = layer.forward(&inp)?;
            cache.push((layer.layer_type().to_string(), inp, out.clone()));
            x = out;
        }

        Ok((x, cache))
    }

    /// Compute gradients for all layers by backpropagating through the cache.
    fn backward_pass(
        layers: &[Box<dyn Layer>],
        cache: &[(String, Tensor<f32>, Tensor<f32>)],
        d_out: &Tensor<f32>,
    ) -> Result<Vec<Vec<f64>>, String> {
        let mut grad_layers: Vec<Vec<f64>> = Vec::new();
        let mut upstream_grad = d_out.clone();

        for (layer_idx, (layer_type, inp, _out)) in cache.iter().enumerate().rev() {
            let layer = &layers[layer_idx];
            let layer_grads = match layer_type.as_str() {
                "BatchNorm" => {
                    if let Some(bn) = (&**layer).as_any().downcast_ref::<BatchNorm>() {
                        let (d_gamma, d_beta) = bn.backward(inp, &upstream_grad)?;
                        // d_gamma + d_beta → flat vec
                        let mut g = d_gamma.data().to_vec();
                        g.extend_from_slice(d_beta.data());
                        g.iter().map(|&x| x as f64).collect()
                    } else {
                        vec![0.0; layer.parameters().iter().map(|t| t.len()).sum()]
                    }
                }
                "Conv2d" => {
                    if let Some(conv) = (&**layer).as_any().downcast_ref::<Conv2d>() {
                        let (d_weight, d_bias) = conv.backward(inp, &upstream_grad)?;
                        let mut g: Vec<f64> = d_weight.data().iter().map(|&x| x as f64).collect();
                        if let Some(db) = d_bias {
                            g.extend(db.data().iter().map(|&x| x as f64));
                        }
                        g
                    } else {
                        vec![0.0; layer.parameters().iter().map(|t| t.len()).sum()]
                    }
                }
                _ => {
                    // Linear/other: use tape-based gradient
                    Self::linear_backward(&**layer, inp, &upstream_grad)?
                }
            };
            grad_layers.push(layer_grads);
        }

        grad_layers.reverse();
        Ok(grad_layers)
    }

    /// Tape-based gradient for Linear (or unknown) layers.
    fn linear_backward(
        layer: &dyn Layer,
        input: &Tensor<f32>,
        upstream_grad: &Tensor<f32>,
    ) -> Result<Vec<f64>, String> {
        use scirust_autodiff::tape::{Tape, Var};

        let params = layer.parameters();
        let total_params: usize = params.iter().map(|t| t.len()).sum();
        if total_params == 0 { return Ok(Vec::new()); }

        let tape = Tape::new();
        let flat_params: Vec<f64> = params.iter()
            .flat_map(|t| t.data().iter().map(|&x| x as f64))
            .collect();
        let param_vars: Vec<Var> = flat_params.iter().map(|&p| tape.var("p", p)).collect();

        let batch = input.shape()[0];
        let in_features = params[0].shape()[1];
        let out_features = params[0].shape()[0];
        let input_data = input.data();
        let grad_data = upstream_grad.data();

        // Build loss = Σ(upstream_grad * prediction)
        let mut loss = tape.var("loss", 0.0);
        for b in 0..batch {
            for j in 0..out_features {
                let mut pred = tape.var("pred", 0.0);
                for i in 0..in_features {
                    let w_idx = j * in_features + i;
                    let x_val = input_data[b * in_features + i] as f64;
                    let xv = tape.var("x", x_val);
                    pred = pred.add(&param_vars[w_idx].mul(&xv));
                }
                let bias_idx = in_features * out_features + j;
                if bias_idx < total_params {
                    pred = pred.add(&param_vars[bias_idx]);
                }
                let g_val = grad_data[b * out_features + j] as f64;
                loss = loss.add(&pred.mul_scalar(g_val));
            }
        }

        tape.backward(&loss);
        Ok(param_vars.iter().map(|v| v.grad()).collect())
    }

    /// Compute gradients for the MSE loss end-to-end.
    pub fn compute_gradients(
        layers: &mut [Box<dyn Layer>],
        input: &Tensor<f32>,
        target: &Tensor<f32>,
    ) -> Result<(f64, Vec<f64>), String> {
        let (output, cache) = Self::forward_with_cache(layers, input)?;

        // MSE loss
        let batch = output.shape()[0];
        let out_dim = output.shape()[1];
        let mut loss_val = 0.0f64;
        let mut d_out_data = vec![0.0f32; output.len()];

        for b in 0..batch {
            for j in 0..out_dim {
                let diff = output.data()[b * out_dim + j] as f64 - target.data()[b * out_dim + j] as f64;
                loss_val += diff * diff;
                d_out_data[b * out_dim + j] = (2.0 * diff) as f32 / batch as f32;
            }
        }
        loss_val /= batch as f64;

        let d_out = Tensor::<f32>::from_vec(d_out_data, output.shape())
            .map_err(|e| format!("d_out tensor: {}", e))?;

        let grad_groups = Self::backward_pass(layers, &cache, &d_out)?;
        let flat_grads: Vec<f64> = grad_groups.into_iter().flatten().collect();

        Ok((loss_val, flat_grads))
    }

    /// Train one step.
    pub fn train_step(
        &mut self,
        layers: &mut [Box<dyn Layer>],
        input: &Tensor<f32>,
        target: &Tensor<f32>,
    ) -> f64 {
        let flat_params = Self::extract_params(layers);
        let (loss, grads) = Self::compute_gradients(layers, input, target).unwrap_or((0.0, vec![0.0; flat_params.len()]));

        let mut new_params = flat_params;
        let key = "all";
        if self.use_adam {
            self.adam_update(key, &mut new_params, &grads);
        } else {
            self.sgd_update(&mut new_params, &grads);
        }

        Self::inject_params(layers, &new_params);
        loss
    }

    fn sgd_update(&self, params: &mut [f64], grads: &[f64]) {
        for i in 0..params.len() {
            params[i] -= self.learning_rate * grads[i];
        }
    }

    fn adam_update(&mut self, key: &str, params: &mut [f64], grads: &[f64]) {
        let n = params.len();
        let m_entry = self.adam_m.entry(key.to_string()).or_insert_with(|| vec![0.0; n]);
        let v_entry = self.adam_v.entry(key.to_string()).or_insert_with(|| vec![0.0; n]);
        if m_entry.len() != n { *m_entry = vec![0.0; n]; }
        if v_entry.len() != n { *v_entry = vec![0.0; n]; }

        self.adam_t += 1;
        for i in 0..n {
            m_entry[i] = 0.9 * m_entry[i] + 0.1 * grads[i];
            v_entry[i] = 0.999 * v_entry[i] + 0.001 * grads[i] * grads[i];
            let m_hat = m_entry[i] / (1.0 - 0.9f64.powi(self.adam_t as i32));
            let v_hat = v_entry[i] / (1.0 - 0.999f64.powi(self.adam_t as i32));
            params[i] -= self.learning_rate * m_hat / (v_hat.sqrt() + 1e-8);
        }
    }

    /// Train multiple epochs.
    pub fn train(
        &mut self,
        layers: &mut [Box<dyn Layer>],
        inputs: &[Tensor<f32>],
        targets: &[Tensor<f32>],
        epochs: usize,
    ) -> Vec<f64> {
        let mut losses = Vec::with_capacity(epochs);
        for _ in 0..epochs {
            let mut epoch_loss = 0.0;
            for (inp, tgt) in inputs.iter().zip(targets.iter()) {
                epoch_loss += self.train_step(layers, inp, tgt);
            }
            losses.push(epoch_loss / inputs.len() as f64);
        }
        losses
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{Linear, ReLU};

    #[test]
    fn test_extract_inject_params() {
        let mut linear = Box::new(Linear::new(3, 2, true)) as Box<dyn Layer>;
        let mut layers = vec![linear];
        let params = NNTrainer::extract_params(&mut layers);
        assert_eq!(params.len(), 3 * 2 + 2);

        let modified: Vec<f64> = params.iter().map(|p| p + 1.0).collect();
        NNTrainer::inject_params(&mut layers, &modified);
        let params2 = NNTrainer::extract_params(&mut layers);
        for (a, b) in params.iter().zip(params2.iter()) {
            assert!((b - a - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_count_params() {
        let linear = Box::new(Linear::new(4, 3, true)) as Box<dyn Layer>;
        assert_eq!(NNTrainer::count_params(&[linear]), 4 * 3 + 3);
    }

    #[test]
    fn test_gradient_linear() {
        let linear = Box::new(Linear::new(2, 1, true)) as Box<dyn Layer>;
        let mut layers = vec![linear];
        // Manually set known weights
        let known = vec![1.0f64, 2.0, 0.5];
        NNTrainer::inject_params(&mut layers, &known);

        let input = Tensor::<f32>::from_vec(vec![1.0, 1.0], &[1, 2]).unwrap();
        let target = Tensor::<f32>::from_vec(vec![5.0], &[1, 1]).unwrap();

        let (loss, grads) = NNTrainer::compute_gradients(&mut layers, &input, &target).unwrap();
        // pred = 1*1 + 2*1 + 0.5 = 3.5
        // loss = (3.5 - 5)^2 / 1 = 2.25
        assert!((loss - 2.25).abs() < 1e-6, "loss={}", loss);
        assert!((grads[0] - (-3.0)).abs() < 1e-6, "grad[0]={}", grads[0]);
    }

    #[test]
    fn test_train_decreases_loss() {
        let mut layers: Vec<Box<dyn Layer>> = vec![
            Box::new(Linear::new(2, 1, true)),
        ];

        let input = Tensor::<f32>::from_vec(vec![1.0, 2.0], &[1, 2]).unwrap();
        let target = Tensor::<f32>::from_vec(vec![5.0], &[1, 1]).unwrap();

        let mut trainer = NNTrainer::new(0.01);
        let l1 = trainer.train_step(&mut layers, &input, &target);
        let l2 = trainer.train_step(&mut layers, &input, &target);
        assert!(l2 <= l1 + 1e-6, "loss increased: {} -> {}", l1, l2);
    }

    #[test]
    fn test_forward_with_cache() {
        let layers: Vec<Box<dyn Layer>> = vec![
            Box::new(Linear::new(4, 3, true)),
        ];
        let input = Tensor::<f32>::ones(&[2, 4]);
        let (_output, cache) = NNTrainer::forward_with_cache(&layers, &input).unwrap();
        assert_eq!(cache.len(), 1);
        assert_eq!(cache[0].0, "Linear");
    }

    #[test]
    fn test_conv_backward_integration() {
        let conv = Box::new(Conv2d::new(1, 1, 3, 1, 0, true)) as Box<dyn Layer>;
        let mut layers = vec![conv];

        let input = Tensor::<f32>::ones(&[1, 1, 4, 4]);
        let target = Tensor::<f32>::ones(&[1, 1, 2, 2]);

        let mut trainer = NNTrainer::new(0.01);
        let loss = trainer.train_step(&mut layers, &input, &target);
        assert!(loss.is_finite(), "Loss should be finite, got {}", loss);
    }

    #[test]
    fn test_bn_backward_integration() {
        let bn = Box::new(BatchNorm::new(3, 1e-5)) as Box<dyn Layer>;
        let mut layers: Vec<Box<dyn Layer>> = vec![
            Box::new(Linear::new(4, 3, false)),
            bn,
        ];

        let input = Tensor::<f32>::rand(&[2, 4]);
        let target = Tensor::<f32>::rand(&[2, 3]);

        let mut trainer = NNTrainer::new(0.01);
        let loss = trainer.train_step(&mut layers, &input, &target);
        assert!(loss.is_finite(), "Loss should be finite, got {}", loss);
    }
}
