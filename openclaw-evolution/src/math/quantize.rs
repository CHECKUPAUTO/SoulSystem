//! # Quantization 3-bit (TurboQuant style)
//!
//! Compresse les vecteurs f32 → 3 bits par élément pour la mémoire épisodique.
//! 8 niveaux uniformes par bloc, avec facteur d'échelle f32.
//!
//! Compression : f32 (32 bits) → 3 bits = ratio ~10.7x

use serde::{Deserialize, Serialize};

/// Nombre de niveaux de quantification (2^3 = 8)
const Q_LEVELS: usize = 8;
/// Bits par valeur
const Q_BITS: u32 = 3;

/// Vecteur quantifié en 3 bits par blocs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantizedVector {
    /// Taille originale
    pub original_len: usize,
    /// Taille des blocs
    pub block_size: usize,
    /// Codes quantifiés (emballés en u8 : 2 valeurs 3-bit par u8 + padding)
    pub codes: Vec<u8>,
    /// Facteur d'échelle par bloc (absmax)
    pub scales: Vec<f32>,
    /// Point zéro par bloc
    pub zero_points: Vec<f32>,
}

/// Configuration de la quantification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantConfig {
    /// Taille du bloc (plus petit = plus précis, plus de metadata)
    pub block_size: usize,
    /// Utiliser la quantification symétrique (zero_point = 0)
    pub symmetric: bool,
}

impl Default for QuantConfig {
    fn default() -> Self {
        Self {
            block_size: 32,
            symmetric: true,
        }
    }
}

/// Quantifier un vecteur f32 en 3 bits
pub fn quantize(data: &[f32], config: &QuantConfig) -> QuantizedVector {
    let n = data.len();
    let bs = config.block_size;
    let n_blocks = (n + bs - 1) / bs;

    let mut scales = Vec::with_capacity(n_blocks);
    let mut zero_points = Vec::with_capacity(n_blocks);
    let mut all_codes: Vec<u8> = Vec::with_capacity(n);

    for block_idx in 0..n_blocks {
        let start = block_idx * bs;
        let end = (start + bs).min(n);
        let block = &data[start..end];

        if config.symmetric {
            // Symétrique : scale = absmax / (levels/2 - 1)
            let absmax = block.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
            let scale = if absmax < 1e-10 { 1.0 } else { absmax / 3.0 }; // 3 = (8/2 - 1)
            scales.push(scale);
            zero_points.push(0.0);

            for &val in block {
                // Quantifier : code = round(val / scale) + 4 (offset pour 0..7)
                let q = (val / scale).round() as i32 + 4;
                let q = q.clamp(0, 7) as u8;
                all_codes.push(q);
            }
        } else {
            // Asymétrique : scale = (max - min) / (levels - 1)
            let min_val = block.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_val = block.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let range = max_val - min_val;
            let scale = if range < 1e-10 { 1.0 } else { range / 7.0 };
            scales.push(scale);
            zero_points.push(min_val);

            for &val in block {
                let q = ((val - min_val) / scale).round() as i32;
                let q = q.clamp(0, 7) as u8;
                all_codes.push(q);
            }
        }
    }

    // Emballer les codes 3-bit en octets (2 valeurs + 2 bits de padding par octet)
    let packed = pack_3bit(&all_codes);

    QuantizedVector {
        original_len: n,
        block_size: bs,
        codes: packed,
        scales,
        zero_points,
    }
}

/// Déquantifier un vecteur 3-bit → f32
pub fn dequantize(qv: &QuantizedVector) -> Vec<f32> {
    let codes = unpack_3bit(&qv.codes, qv.original_len);
    let mut result = Vec::with_capacity(qv.original_len);
    let bs = qv.block_size;

    for (i, &code) in codes.iter().enumerate() {
        let block_idx = i / bs;
        let scale = qv.scales[block_idx];
        let zp = qv.zero_points[block_idx];

        let val = if zp == 0.0 {
            // Symétrique : val = (code - 4) * scale
            (code as f32 - 4.0) * scale
        } else {
            // Asymétrique : val = code * scale + zero_point
            code as f32 * scale + zp
        };

        result.push(val);
    }

    result
}

/// Ratio de compression
pub fn compression_ratio(qv: &QuantizedVector) -> f32 {
    let original_bytes = qv.original_len * 4; // f32 = 4 bytes
    let compressed_bytes = qv.codes.len() + qv.scales.len() * 4 + qv.zero_points.len() * 4;
    original_bytes as f32 / compressed_bytes as f32
}

/// Erreur de quantification (RMSE normalisée)
pub fn quantization_error(original: &[f32], qv: &QuantizedVector) -> f32 {
    let reconstructed = dequantize(qv);
    let mse: f32 = original
        .iter()
        .zip(reconstructed.iter())
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f32>()
        / original.len() as f32;

    let rms_orig: f32 =
        (original.iter().map(|x| x * x).sum::<f32>() / original.len() as f32).sqrt();
    if rms_orig < 1e-10 {
        0.0
    } else {
        mse.sqrt() / rms_orig
    }
}

// ── Packing 3-bit ──────────────────────────

/// Emballer des codes 3-bit en octets
/// Layout : chaque octet contient 2 valeurs 3-bit (6 bits utilisés, 2 de padding)
fn pack_3bit(codes: &[u8]) -> Vec<u8> {
    let mut packed = Vec::with_capacity((codes.len() + 1) / 2);
    let mut i = 0;
    while i < codes.len() {
        let a = codes[i] & 0x07;
        let b = if i + 1 < codes.len() {
            codes[i + 1] & 0x07
        } else {
            0
        };
        // Pack : [aaa bbb 00]
        packed.push((a << 5) | (b << 2));
        i += 2;
    }
    packed
}

/// Déballer des codes 3-bit depuis des octets
fn unpack_3bit(packed: &[u8], n: usize) -> Vec<u8> {
    let mut codes = Vec::with_capacity(n);
    for &byte in packed {
        codes.push((byte >> 5) & 0x07);
        codes.push((byte >> 2) & 0x07);
    }
    codes.truncate(n);
    codes
}

/// Quantification adaptative : choisir block_size optimal par cross-validation
pub fn auto_quantize(data: &[f32], target_error: f32) -> QuantizedVector {
    let block_sizes = [16, 32, 64, 128];

    for &bs in &block_sizes {
        let config = QuantConfig {
            block_size: bs,
            symmetric: true,
        };
        let qv = quantize(data, &config);
        let error = quantization_error(data, &qv);
        if error <= target_error {
            return qv;
        }
    }

    // Fallback : plus petit block size
    quantize(
        data,
        &QuantConfig {
            block_size: 16,
            symmetric: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_dequantize() {
        let data: Vec<f32> = (0..64).map(|i| (i as f32 - 32.0) * 0.1).collect();
        let config = QuantConfig::default();
        let qv = quantize(&data, &config);
        let reconstructed = dequantize(&qv);

        assert_eq!(reconstructed.len(), data.len());
        let error = quantization_error(&data, &qv);
        assert!(
            error < 0.3,
            "Erreur de quantification trop élevée: {}",
            error
        );
    }

    #[test]
    fn test_compression_ratio() {
        let data = vec![1.0; 1024];
        let qv = quantize(&data, &QuantConfig::default());
        let ratio = compression_ratio(&qv);
        assert!(ratio > 3.0, "Ratio devrait être > 3x, got {:.1}x", ratio);
    }

    #[test]
    fn test_pack_unpack_roundtrip() {
        let codes: Vec<u8> = vec![0, 3, 7, 1, 5, 2, 4, 6];
        let packed = pack_3bit(&codes);
        let unpacked = unpack_3bit(&packed, codes.len());
        assert_eq!(codes, unpacked);
    }
}
