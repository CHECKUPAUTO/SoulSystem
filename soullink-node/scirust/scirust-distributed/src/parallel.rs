//! Data parallelism for inference.
//!
//! Shards input data across multiple threads, runs inference on each shard,
//! and combines the results. Uses `std::thread`.

use scirust_inference::pipeline::SequentialModel;
use scirust_inference::tensor::Tensor;
use std::sync::Arc;

/// Result of a data-parallel inference run.
#[derive(Debug, Clone)]
pub struct ParallelInferenceResult {
    pub output: Tensor<f32>,
    pub per_thread_timing_ms: Vec<f64>,
    pub total_time_ms: f64,
    pub num_threads: usize,
}

/// Run inference on a batch of data using multiple threads.
///
/// The input is sharded along the batch dimension (axis 0). Each thread
/// runs the model on its shard, and results are concatenated.
///
/// # Arguments
/// * `model` — The model to run (shared via Arc)
/// * `input` — Input tensor with batch dimension
/// * `num_threads` — Number of threads to use
pub fn parallel_inference(
    model: Arc<SequentialModel>,
    input: &Tensor<f32>,
    num_threads: usize,
) -> Result<ParallelInferenceResult, String> {
    if input.shape().is_empty() {
        return Err("Empty input shape".to_string());
    }
    let batch_size = input.shape()[0];
    let actual_threads = num_threads.min(batch_size).max(1);

    // Shard the input along batch dimension
    let shard_size = (batch_size + actual_threads - 1) / actual_threads;
    let mut shards: Vec<(usize, Tensor<f32>)> = Vec::new();
    let mut start = 0;

    let inner_size = input.len() / batch_size;

    for _ in 0..actual_threads {
        let end = (start + shard_size).min(batch_size);
        if start >= end { break; }
        let actual_shard_size = end - start;

        let mut shard_data = Vec::with_capacity(actual_shard_size * inner_size);
        for b in start..end {
            let base = b * inner_size;
            for i in 0..inner_size {
                shard_data.push(input.data()[base + i]);
            }
        }

        let mut shard_shape = input.shape().to_vec();
        shard_shape[0] = actual_shard_size;

        let shard_tensor = Tensor::<f32>::from_vec(shard_data, &shard_shape)
            .map_err(|e| format!("Shard tensor: {}", e))?;

        shards.push((start, shard_tensor));
        start = end;
    }

    // Run inference on each shard in parallel
    let results = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut handles = Vec::new();

    for (original_idx, shard) in shards {
        let model_clone = Arc::clone(&model);
        let results_clone = Arc::clone(&results);

        let handle = std::thread::spawn(move || {
            let start_time = std::time::Instant::now();
            let output = model_clone.forward(&shard);
            let elapsed = start_time.elapsed().as_secs_f64() * 1000.0;

            match output {
                Ok(out) => {
                    let mut r = results_clone.lock().unwrap();
                    r.push((original_idx, out, elapsed));
                }
                Err(e) => {
                    eprintln!("Thread inference error: {}", e);
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().map_err(|_| "Thread panic".to_string())?;
    }

    // Combine results
    let mut all_results = results.lock().unwrap().clone();
    all_results.sort_by_key(|(idx, _, _)| *idx);

    if all_results.is_empty() {
        return Err("No results from threads".to_string());
    }

    let out_shape = all_results[0].1.shape().to_vec();
    let out_inner: usize = out_shape.iter().skip(1).product();

    let total_batch: usize = all_results.iter().map(|(_, t, _)| t.shape()[0]).sum();
    let mut combined_shape = out_shape;
    combined_shape[0] = total_batch;

    let mut combined_data = Vec::with_capacity(total_batch * out_inner);
    let mut timings = Vec::new();

    for (_, tensor, timing) in &all_results {
        combined_data.extend_from_slice(tensor.data());
        timings.push(*timing);
    }

    let output = Tensor::<f32>::from_vec(combined_data, &combined_shape)
        .map_err(|e| format!("Combine: {}", e))?;

    let total_time = timings.iter().sum::<f64>() / timings.len() as f64;

    Ok(ParallelInferenceResult {
        output,
        per_thread_timing_ms: timings,
        total_time_ms: total_time,
        num_threads: actual_threads,
    })
}

/// Speedup factor compared to single-threaded inference.
pub fn speedup_vs_single(
    model: Arc<SequentialModel>,
    input: &Tensor<f32>,
    num_threads: usize,
) -> Result<f64, String> {
    let start = std::time::Instant::now();
    let _single = model.forward(input)?;
    let single_time = start.elapsed().as_secs_f64();

    let parallel_result = parallel_inference(model, input, num_threads)?;
    let parallel_time = parallel_result.total_time_ms / 1000.0;

    if parallel_time <= 0.0 { return Ok(1.0); }
    Ok(single_time / parallel_time)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use scirust_inference::layers::{Linear, ReLU};

    #[test]
    fn test_parallel_inference_single_thread() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(4, 3, false));
        model.add(ReLU::new());
        let model = Arc::new(model);

        let input = Tensor::<f32>::ones(&[8, 4]);
        let result = parallel_inference(model, &input, 1).unwrap();

        assert_eq!(result.output.shape(), &[8, 3]);
        assert_eq!(result.per_thread_timing_ms.len(), 1);
    }

    #[test]
    fn test_parallel_inference_multi_thread() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(4, 2, true));
        let model = Arc::new(model);

        let input = Tensor::<f32>::ones(&[16, 4]);
        let result = parallel_inference(model, &input, 4).unwrap();

        assert_eq!(result.output.shape(), &[16, 2]);
        assert_eq!(result.per_thread_timing_ms.len(), 4);
    }

    #[test]
    fn test_parallel_inference_small_batch() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(4, 2, false));
        let model = Arc::new(model);

        let input = Tensor::<f32>::ones(&[2, 4]);
        let result = parallel_inference(model, &input, 8).unwrap();

        assert_eq!(result.output.shape(), &[2, 2]);
        assert_eq!(result.num_threads, 2);
    }

    #[test]
    fn test_speedup_positive() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(100, 50, true));
        model.add(ReLU::new());
        model.add(Linear::new(50, 10, false));
        let model = Arc::new(model);

        let input = Tensor::<f32>::rand(&[32, 100]);
        let speedup = speedup_vs_single(model, &input, 2).unwrap();
        assert!(speedup > 0.0, "Speedup should be positive: {}", speedup);
    }
}
