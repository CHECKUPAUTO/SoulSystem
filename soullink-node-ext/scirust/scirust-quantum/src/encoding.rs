//! Classical-to-quantum data encoding strategies.
//!
//! # Encoding Methods
//!
//! * **Angle encoding** — Map each feature to a rotation angle.
//!   `x ↦ RY(π·x)`. Requires 1 qubit per feature.
//!
//! * **Amplitude encoding** — Encode `N` features into the amplitudes of
//!   `⌈log₂(N)⌉` qubits. More efficient but requires normalization.
//!
//! * **Basis encoding** — Binary encoding of integer values.

use crate::circuit::QuantumCircuit;

/// Encode a feature vector into a quantum circuit using angle encoding.
///
/// Each feature `x_i` is encoded as an `RY(π * x_i)` rotation on qubit `i`.
///
/// # Arguments
/// * `features` — Slice of feature values (should be normalized or in range [0, 1])
///
/// # Returns
/// A circuit with the features encoded as RY rotations.
pub fn angle_encoding(features: &[f64]) -> QuantumCircuit {
    let n = features.len();
    let mut qc = QuantumCircuit::new(n);
    for (i, &x) in features.iter().enumerate() {
        let theta = std::f64::consts::PI * x.clamp(-1.0, 1.0);
        qc.ry_angle(i, theta);
    }
    qc
}

/// Encode a feature vector using dense angle encoding (RX + RZ).
///
/// Each feature is encoded twice: once as RX and once as RZ,
/// providing richer expressivity per qubit.
///
/// # Arguments
/// * `features` — Feature values (should be in range [0, 1])
///
/// # Returns
/// A circuit with 2× features.length qubits.
pub fn dense_angle_encoding(features: &[f64]) -> QuantumCircuit {
    let n = features.len();
    let mut qc = QuantumCircuit::new(n);
    for (i, &x) in features.iter().enumerate() {
        let theta_x = std::f64::consts::PI * x.clamp(-1.0, 1.0);
        let theta_z = std::f64::consts::PI * (1.0 - x).clamp(-1.0, 1.0);
        qc.rx_angle(i, theta_x);
        qc.rz_angle(i, theta_z);
    }
    qc
}

/// Encode data using amplitude encoding.
///
/// Maps a normalized feature vector to the amplitudes of `⌈log₂(N)⌉` qubits.
/// The input vector will be padded with zeros to the next power of 2.
///
/// # Arguments
/// * `data` — Feature vector (will be normalized to unit length)
///
/// # Returns
/// A circuit initialized by the amplitude encoding, plus the number of qubits.
pub fn amplitude_encoding(data: &[f64]) -> (QuantumCircuit, Vec<f64>) {
    let n = data.len();
    let num_qubits = (n as f64).log2().ceil() as usize;
    let dim = 1 << num_qubits;

    // Pad and normalize
    let mut amplitudes = vec![0.0f64; dim];
    for (i, &v) in data.iter().enumerate().take(dim.min(n)) {
        amplitudes[i] = v;
    }
    let norm: f64 = amplitudes.iter().map(|x| x * x).sum();
    if norm > 0.0 {
        let inv_norm = 1.0 / norm.sqrt();
        for a in &mut amplitudes {
            *a *= inv_norm;
        }
    }

    let qc = QuantumCircuit::new(num_qubits);
    // Note: full state preparation would require complex decomposition;
    // for now we return the circuit + amplitudes for custom simulation.
    (qc, amplitudes)
}

/// Encode binary data as basis states.
///
/// # Arguments
/// * `bits` — Binary values (0 or 1)
///
/// # Returns
/// A circuit with X gates applied to qubits where the bit is 1.
pub fn basis_encoding(bits: &[u8]) -> QuantumCircuit {
    let n = bits.len();
    let mut qc = QuantumCircuit::new(n);
    for (i, &b) in bits.iter().enumerate() {
        if b == 1 {
            qc.x(i);
        }
    }
    qc
}

/// Compute the number of qubits needed for a given encoding type.
pub fn qubits_needed(data_len: usize, encoding: EncodingType) -> usize {
    match encoding {
        EncodingType::Angle => data_len,
        EncodingType::DenseAngle => data_len,
        EncodingType::Amplitude => (data_len as f64).log2().ceil() as usize,
        EncodingType::Basis => data_len,
    }
}

/// Supported encoding types.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EncodingType {
    Angle,
    DenseAngle,
    Amplitude,
    Basis,
}

impl std::fmt::Display for EncodingType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodingType::Angle => write!(f, "AngleEncoding"),
            EncodingType::DenseAngle => write!(f, "DenseAngleEncoding"),
            EncodingType::Amplitude => write!(f, "AmplitudeEncoding"),
            EncodingType::Basis => write!(f, "BasisEncoding"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_angle_encoding() {
        let features = vec![0.3, 0.7, 0.5];
        let qc = angle_encoding(&features);
        assert_eq!(qc.num_qubits, 3);
        assert_eq!(qc.num_gates(), 3);
        // All gates should be RY
        for gate in qc.gates() {
            assert_eq!(gate.gate_type, crate::circuit::GateType::RY);
        }
    }

    #[test]
    fn test_basis_encoding() {
        let bits = vec![1, 0, 1, 1];
        let qc = basis_encoding(&bits);
        assert_eq!(qc.num_qubits, 4);
        assert_eq!(qc.num_gates(), 3); // three X gates
    }

    #[test]
    fn test_amplitude_encoding() {
        let data = vec![1.0, 2.0, 3.0];
        let (qc, amplitudes) = amplitude_encoding(&data);
        // ceil(log₂(3)) = 2 qubits, 4 amplitudes
        assert_eq!(qc.num_qubits, 2);
        assert_eq!(amplitudes.len(), 4);
        // Check normalization
        let norm: f64 = amplitudes.iter().map(|x| x * x).sum();
        assert!((norm - 1.0).abs() < 1e-10, "norm={}", norm);
    }

    #[test]
    fn test_dense_angle_encoding() {
        let features = vec![0.2, 0.8];
        let qc = dense_angle_encoding(&features);
        assert_eq!(qc.num_qubits, 2);
        assert_eq!(qc.num_gates(), 4); // RX + RZ per qubit
    }

    #[test]
    fn test_qubits_needed() {
        assert_eq!(qubits_needed(4, EncodingType::Angle), 4);
        assert_eq!(qubits_needed(4, EncodingType::Amplitude), 2);
        assert_eq!(qubits_needed(8, EncodingType::Amplitude), 3);
        assert_eq!(qubits_needed(5, EncodingType::Amplitude), 3);
    }
}
