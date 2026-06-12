//! INT8 quantization utilities for SSM weight compression.
//!
//! Provides symmetric per-row quantization for weight matrices,
//! enabling 4× memory reduction and potential hardware acceleration
//! via INT8 compute.
//!
//! # Scheme
//!
//! Symmetric per-row quantization:
//!   scale_r = max_c(|W[r,c]|) / 127.0
//!   W_q[r,c] = round(W[r,c] / scale_r)
//!   W_f32[r,c] ≈ W_q[r,c] * scale_r
//!
//! For matmul: Y = W · X is approximated as:
//!   Y[r,:] ≈ scale_r · (W_q[r,:] · X[:,:])   (INT8 dot products)

use ndarray::{Array1, Array2, ArrayView1};

/// Quantized weight matrix with per-row scaling factors.
#[derive(Debug, Clone)]
pub struct QuantizedMatrix {
    /// INT8 quantized weights — shape (rows, cols)
    pub data: Array2<i8>,
    /// Per-row scale factors — shape (rows,)
    pub scales: Array1<f64>,
    /// Number of rows
    pub rows: usize,
    /// Number of columns
    pub cols: usize,
}

impl QuantizedMatrix {
    /// Quantize a dense f64 matrix using symmetric per-row quantization.
    ///
    /// # Arguments
    ///
    /// * `weights` — Full-precision weight matrix (rows, cols)
    ///
    /// # Returns
    ///
    /// Quantized matrix with INT8 data and per-row scales
    pub fn quantize(weights: &Array2<f64>) -> Self {
        let shape = weights.shape();
        let rows = shape[0];
        let cols = shape[1];

        let mut data = Array2::<i8>::zeros((rows, cols));
        let mut scales = Array1::zeros(rows);

        for r in 0..rows {
            let row = weights.row(r);
            let max_abs: f64 = row.iter().map(|v| v.abs()).fold(0.0, f64::max);

            if max_abs < 1e-12 {
                scales[r] = 1.0;
                continue;
            }

            scales[r] = max_abs / 127.0;
            for c in 0..cols {
                let q = (row[c] / scales[r]).round().clamp(-128.0, 127.0);
                data[[r, c]] = q as i8;
            }
        }

        Self {
            data,
            scales,
            rows,
            cols,
        }
    }

    /// Dequantize back to full precision f64.
    pub fn dequantize(&self) -> Array2<f64> {
        let mut out = Array2::zeros((self.rows, self.cols));
        for r in 0..self.rows {
            let s = self.scales[r];
            for c in 0..self.cols {
                out[[r, c]] = self.data[[r, c]] as f64 * s;
            }
        }
        out
    }

    /// Quantized matrix-vector multiply: y = W · x
    ///
    /// Computes y[r] = scale[r] · Σ_c W_q[r,c] · x[c]
    /// using INT8 × f64 dot products.
    pub fn dot(&self, x: &ArrayView1<f64>) -> Array1<f64> {
        let mut y = Array1::zeros(self.rows);
        for r in 0..self.rows {
            let mut sum: f64 = 0.0;
            for c in 0..self.cols {
                sum += self.data[[r, c]] as f64 * x[c];
            }
            y[r] = sum * self.scales[r];
        }
        y
    }

    /// Quantized matrix-matrix multiply: Y = W · X
    ///
    /// Each row of W is quantized independently.
    pub fn matmul(&self, x: &Array2<f64>) -> Array2<f64> {
        let x_cols = x.shape()[1];
        let mut y = Array2::zeros((self.rows, x_cols));
        for r in 0..self.rows {
            let s = self.scales[r];
            for c in 0..x_cols {
                let mut sum: f64 = 0.0;
                for k in 0..self.cols {
                    sum += self.data[[r, k]] as f64 * x[[k, c]];
                }
                y[[r, c]] = sum * s;
            }
        }
        y
    }

    /// Estimate memory savings vs f64 (bytes).
    pub fn memory_ratio(&self) -> f64 {
        // INT8: 1 byte per element + 8 bytes per scale
        // f64:  8 bytes per element
        let int8_bytes = self.data.len() + 8 * self.rows;
        let f64_bytes = 8 * self.data.len();
        int8_bytes as f64 / f64_bytes as f64
    }
}

/// INT8 quantized embedding table.
///
/// Each row (vocabulary entry) is quantized independently.
#[derive(Debug, Clone)]
pub struct QuantizedEmbedding {
    /// Quantized embedding data
    pub inner: QuantizedMatrix,
    /// Vocabulary size
    pub vocab_size: usize,
    /// Embedding dimension
    pub d_model: usize,
}

impl QuantizedEmbedding {
    /// Quantize a full-precision embedding matrix.
    pub fn quantize(embedding: &Array2<f64>) -> Self {
        let shape = embedding.shape();
        let inner = QuantizedMatrix::quantize(embedding);
        Self {
            vocab_size: shape[0],
            d_model: shape[1],
            inner,
        }
    }

    /// Look up a token's embedding vector (dequantized on-the-fly).
    pub fn lookup(&self, token_id: usize) -> Array1<f64> {
        let idx = token_id.min(self.vocab_size - 1);
        let mut vec = Array1::zeros(self.d_model);
        let s = self.inner.scales[idx];
        for c in 0..self.d_model {
            vec[c] = self.inner.data[[idx, c]] as f64 * s;
        }
        vec
    }

    /// Look up multiple tokens.
    pub fn lookup_batch(&self, token_ids: &[usize]) -> Array2<f64> {
        let l = token_ids.len();
        let mut out = Array2::zeros((l, self.d_model));
        for (i, &tid) in token_ids.iter().enumerate() {
            let row = self.lookup(tid);
            out.row_mut(i).assign(&row);
        }
        out
    }
}

/// Symmetric quantization error metrics.
pub struct QuantizationError {
    /// Maximum absolute error per row
    pub max_abs_error: f64,
    /// Mean absolute error per row
    pub mean_abs_error: f64,
    /// Signal-to-noise ratio (dB)
    pub snr_db: f64,
}

/// Compute quantization error metrics between original and dequantized matrix.
pub fn quantization_error(original: &Array2<f64>, dequantized: &Array2<f64>) -> QuantizationError {
    let mut max_err = 0.0_f64;
    let mut sum_abs = 0.0_f64;
    let mut signal_power = 0.0_f64;
    let mut noise_power = 0.0_f64;
    let n = original.len();

    for i in 0..original.shape()[0] {
        for j in 0..original.shape()[1] {
            let err = (original[[i, j]] - dequantized[[i, j]]).abs();
            max_err = max_err.max(err);
            sum_abs += err;
            signal_power += original[[i, j]] * original[[i, j]];
            noise_power += err * err;
        }
    }

    let snr = if noise_power > 0.0 {
        10.0 * (signal_power / noise_power).log10()
    } else {
        f64::INFINITY
    };

    QuantizationError {
        max_abs_error: max_err,
        mean_abs_error: sum_abs / n as f64,
        snr_db: snr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_dequantize_identity() {
        let mut w = Array2::zeros((4, 8));
        for i in 0..4 {
            for j in 0..8 {
                w[[i, j]] = (i * 8 + j) as f64 * 0.1 - 0.4;
            }
        }

        let q = QuantizedMatrix::quantize(&w);
        let dq = q.dequantize();

        // Should be close (int8 quantization error is bounded)
        for i in 0..4 {
            for j in 0..8 {
                let err = (w[[i, j]] - dq[[i, j]]).abs();
                assert!(err < 0.1, "Error too large at [{},{}]: {}", i, j, err);
            }
        }
    }

    #[test]
    fn test_quantized_matmul_shape() {
        let w: Array2<f64> = Array2::zeros((3, 5)).mapv(|_: f64| rand::random::<f64>() * 2.0 - 1.0);
        let x: Array2<f64> = Array2::zeros((5, 2)).mapv(|_: f64| rand::random::<f64>());

        let q = QuantizedMatrix::quantize(&w);
        let y = q.matmul(&x);

        assert_eq!(y.shape(), &[3, 2]);
    }

    #[test]
    fn test_quantized_dot_approximate() {
        let w: Array2<f64> = Array2::zeros((2, 4)).mapv(|_: f64| rand::random::<f64>() * 2.0 - 1.0);
        let x = Array1::from_vec(vec![0.5, -0.3, 0.8, 0.1]);

        let q = QuantizedMatrix::quantize(&w);
        let y_q = q.dot(&x.view());

        let y_ref = w.dot(&x);

        assert_eq!(y_q.len(), 2);
        for i in 0..2 {
            let err = (y_q[i] - y_ref[i]).abs();
            assert!(
                err < 0.15,
                "Error too large at [{}]: {} (ref={}, q={})",
                i,
                err,
                y_ref[i],
                y_q[i]
            );
        }
    }

    #[test]
    fn test_quantized_embedding_lookup() {
        let embed: Array2<f64> =
            Array2::zeros((10, 4)).mapv(|_: f64| rand::random::<f64>() * 2.0 - 1.0);
        let qe = QuantizedEmbedding::quantize(&embed);

        let v0 = qe.lookup(0);
        assert_eq!(v0.len(), 4);
        let err = (v0[0] - embed[[0, 0]]).abs();
        assert!(err < 0.1, "Lookup error too large: {}", err);
    }

    #[test]
    fn test_quantization_error_metrics() {
        let w: Array2<f64> = Array2::zeros((3, 5)).mapv(|_: f64| rand::random::<f64>() * 2.0 - 1.0);
        let q = QuantizedMatrix::quantize(&w);
        let dq = q.dequantize();

        let err = quantization_error(&w, &dq);
        assert!(err.max_abs_error < 0.15);
        assert!(err.mean_abs_error < 0.05);
        assert!(err.snr_db > 20.0);
    }

    #[test]
    fn test_zero_matrix_quantization() {
        let w = Array2::<f64>::zeros((3, 4));
        let q = QuantizedMatrix::quantize(&w);
        let dq = q.dequantize();

        for i in 0..3 {
            for j in 0..4 {
                assert!((dq[[i, j]]).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn test_memory_ratio() {
        let w: Array2<f64> = Array2::zeros((100, 64)).mapv(|_: f64| rand::random::<f64>());
        let q = QuantizedMatrix::quantize(&w);
        let ratio = q.memory_ratio();
        // INT8 + scales: 100*64*1 + 100*8 = 7200 bytes
        // f64: 100*64*8 = 51200 bytes
        // ratio ≈ 7200/51200 ≈ 0.14
        assert!(
            ratio > 0.1 && ratio < 0.2,
            "Unexpected memory ratio: {}",
            ratio
        );
    }
}
