//! High-level inference pipeline.

use crate::layers::Layer;
use crate::tensor::softmax_f32;
use crate::tensor::Tensor;
use std::time::Instant;

/// A sequential inference model.
pub struct SequentialModel {
    layers: Vec<Box<dyn Layer>>,
}

impl SequentialModel {
    pub fn new() -> Self { Self { layers: Vec::new() } }

    pub fn add<L: Layer + 'static>(&mut self, layer: L) {
        self.layers.push(Box::new(layer));
    }

    pub fn forward(&self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        let mut x = input.clone();
        for layer in &self.layers {
            x = layer.forward(&x)?;
        }
        Ok(x)
    }

    pub fn num_parameters(&self) -> usize {
        self.layers.iter().map(|l| l.parameters().iter().map(|t| t.len()).sum::<usize>()).sum()
    }

    pub fn layer_names(&self) -> Vec<String> {
        self.layers.iter().map(|l| l.name().to_string()).collect()
    }

    pub fn num_layers(&self) -> usize { self.layers.len() }
}

// ---------------------------------------------------------------------------
// Inference session with timing
// ---------------------------------------------------------------------------

pub struct InferenceSession {
    model: SequentialModel,
    timing: Vec<(String, std::time::Duration)>,
}

impl InferenceSession {
    pub fn new(model: SequentialModel) -> Self {
        Self { model, timing: Vec::new() }
    }

    pub fn predict(&mut self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        self.timing.clear();
        let mut x = input.clone();
        for layer in &self.model.layers {
            let start = Instant::now();
            x = layer.forward(&x)?;
            self.timing.push((layer.name().to_string(), start.elapsed()));
        }
        Ok(x)
    }

    pub fn timing_breakdown(&self) -> &[(String, std::time::Duration)] { &self.timing }

    pub fn total_time(&self) -> std::time::Duration {
        self.timing.iter().map(|(_, d)| *d).sum()
    }

    pub fn predict_proba(&mut self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        let logits = self.predict(input)?;
        Ok(softmax_f32(&logits))
    }

    pub fn model(&self) -> &SequentialModel { &self.model }
    pub fn model_mut(&mut self) -> &mut SequentialModel { &mut self.model }
}

// ---------------------------------------------------------------------------
// Benchmarking
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub batch_size: usize,
    pub latency: f64,
    pub throughput: f64,
    pub total_samples: usize,
    pub total_time_ms: f64,
}

/// Benchmark a model with various batch sizes.
pub fn benchmark_model(
    model: &SequentialModel,
    input_shape: &[usize],
    batch_sizes: &[usize],
    num_warmup: usize,
    num_iterations: usize,
) -> Vec<BenchmarkResult> {
    let mut results = Vec::new();

    for &batch in batch_sizes {
        let mut full_shape = input_shape.to_vec();
        full_shape.insert(0, batch);
        let input = Tensor::<f32>::rand(&full_shape);

        // Warmup
        for _ in 0..num_warmup {
            let _ = model.forward(&input);
        }

        let start = Instant::now();
        for _ in 0..num_iterations {
            let _ = model.forward(&input);
        }
        let elapsed = start.elapsed();

        let total_time_ms = elapsed.as_secs_f64() * 1000.0;
        let total_samples = batch * num_iterations;
        let latency = total_time_ms / num_iterations as f64;
        let throughput = total_samples as f64 / elapsed.as_secs_f64();

        results.push(BenchmarkResult {
            batch_size: batch,
            latency,
            throughput,
            total_samples,
            total_time_ms,
        });
    }

    results
}

/// Format benchmark results as a table.
pub fn format_benchmark(results: &[BenchmarkResult]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    writeln!(&mut s, "{:>6} | {:>12} | {:>14} | {:>8}", "Batch", "Latency (ms)", "Throughput/s", "Total").unwrap();
    writeln!(&mut s, "{:-<6}-+-{:-<12}-+-{:-<14}-+-{:-<8}", "", "", "", "").unwrap();
    for r in results {
        writeln!(
            &mut s, "{:>6} | {:>9.3} ms | {:>12.1} | {:>8}",
            r.batch_size, r.latency, r.throughput, r.total_samples
        ).unwrap();
    }
    s
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{Linear, ReLU, Sigmoid, Tanh};

    #[test]
    fn test_sequential_model() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(4, 3, true));
        model.add(ReLU::new());
        model.add(Linear::new(3, 2, false));
        model.add(Sigmoid::new());

        let input = Tensor::<f32>::ones(&[2, 4]);
        let output = model.forward(&input).unwrap();
        assert_eq!(output.shape(), &[2, 2]);
        assert!(model.num_parameters() > 0);
        assert_eq!(model.num_layers(), 4);
    }

    #[test]
    fn test_inference_session() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(10, 5, true));
        model.add(Tanh::new());
        model.add(Linear::new(5, 2, false));

        let mut session = InferenceSession::new(model);
        let input = Tensor::<f32>::rand(&[3, 10]);
        let output = session.predict(&input).unwrap();
        assert_eq!(output.shape(), &[3, 2]);
        assert_eq!(session.timing_breakdown().len(), 3);
    }

    #[test]
    fn test_predict_proba() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(4, 3, false));

        let mut session = InferenceSession::new(model);
        let input = Tensor::<f32>::rand(&[2, 4]);
        let probs = session.predict_proba(&input).unwrap();
        assert_eq!(probs.shape(), &[2, 3]);

        for i in 0..2 {
            let row_sum: f32 = (0..3).map(|j| *probs.get_2d(i, j).unwrap()).sum();
            assert!((row_sum - 1.0).abs() < 1e-5);
        }
    }
}
