//! Hybrid quantum-classical inference.
//!
//! Connects quantum circuit simulation with the existing SciRust
//! inference engine (`scirust-inference`) and probability engine
//! (`scirust-probability`).
//!
//! # Architecture
//!
//! ```text
//! Classical data → QuantumCircuit (parametrized) → Measurement → Tensor → NN layers
//!                                                                         ↓
//! Bayesian network ← Posterior ← Quantum observations ←─────────────── Predictions
//! ```
//!
//! The quantum circuit acts as a **feature map** that transforms classical
//! data into a higher-dimensional Hilbert space before classification.

use crate::circuit::QuantumCircuit;
use crate::encoding::{angle_encoding, EncodingType, qubits_needed};
use crate::simulator::{probabilities, simulate};

/// A hybrid quantum-classical inference pipeline.
///
/// Combines a quantum feature map with classical neural network layers
/// for classification or regression.
pub struct QuantumHybridPipeline {
    /// The quantum feature map circuit (without parameters set).
    pub feature_map: QuantumCircuit,
    /// Number of measurement results fed into the classical layers.
    pub num_measurements: usize,
    /// Encoding type used.
    pub encoding: EncodingType,
    /// Classical model (from scirust-inference)
    pub model: Option<ClassicalModel>,
}

/// Placeholder for a classical model (scirust-inference dependency handled at runtime).
#[derive(Debug, Clone)]
pub struct ClassicalModel {
    pub layer_configs: Vec<LayerConfig>,
}

#[derive(Debug, Clone)]
pub struct LayerConfig {
    pub input_size: usize,
    pub output_size: usize,
    pub activation: String,
}

impl QuantumHybridPipeline {
    /// Create a new hybrid pipeline.
    ///
    /// # Arguments
    /// * `num_features` — Number of classical features
    /// * `encoding` — Encoding type to use
    /// * `classical_layers` — Configuration of classical layers
    pub fn new(num_features: usize, encoding: EncodingType, classical_layers: Vec<LayerConfig>) -> Self {
        let num_qubits = qubits_needed(num_features, encoding);
        let feature_map = QuantumCircuit::new(num_qubits);

        let num_measurements = match encoding {
            EncodingType::Amplitude => 1 << num_qubits, // all basis states
            _ => num_qubits,
        };

        Self {
            feature_map,
            num_measurements,
            encoding,
            model: if classical_layers.is_empty() { None } else { Some(ClassicalModel { layer_configs: classical_layers }) },
        }
    }

    /// Encode classical data into a quantum circuit and measure.
    ///
    /// Returns a vector of measurement probabilities used as features
    /// for the classical model.
    pub fn encode_and_measure(&self, features: &[f64]) -> Result<Vec<f64>, String> {
        // Step 1: Encode features into quantum circuit
        let qc = match self.encoding {
            EncodingType::Angle => angle_encoding(features),
            EncodingType::Basis => {
                let bits: Vec<u8> = features.iter().map(|&x| if x > 0.5 { 1 } else { 0 }).collect();
                crate::encoding::basis_encoding(&bits)
            }
            EncodingType::Amplitude => {
                let (_qc, _amplitudes) = crate::encoding::amplitude_encoding(features);
                // For simplicity, use angle encoding as fallback
                angle_encoding(features)
            }
            EncodingType::DenseAngle => crate::encoding::dense_angle_encoding(features),
        };

        // Step 2: Run variational circuit if feature_map has gates
        // (In a full implementation, the feature_map would be prepended)

        // Step 3: Measure probabilities
        let probs = probabilities(&qc)?;

        // Step 4: Return measurement results as feature vector
        Ok(probs)
    }

    /// Run the full hybrid pipeline: encode → measure → classify.
    pub fn run(&self, features: &[f64]) -> Result<HybridOutput, String> {
        let measurements = self.encode_and_measure(features)?;

        // Here a real implementation would pass measurements through
        // the scirust-inference SequentialModel. For now, we return
        // the measurement results and layer info.
        Ok(HybridOutput {
            measurements: measurements.clone(),
            num_qubits: self.feature_map.num_qubits,
            encoding: self.encoding,
            prediction: None,
        })
    }
}

/// Output from a hybrid quantum-classical inference.
#[derive(Debug, Clone)]
pub struct HybridOutput {
    pub measurements: Vec<f64>,
    pub num_qubits: usize,
    pub encoding: EncodingType,
    pub prediction: Option<f64>,
}

// ---------------------------------------------------------------------------
// Quantum Bayesian Node (integration with scirust-probability)
// ---------------------------------------------------------------------------

/// Configuration for a quantum-generated Bayesian CPT.
///
/// A quantum circuit can generate the conditional probability table
/// for a discrete Bayesian node. The circuit's measurement outcomes
/// define the probabilities of each category given parent states.
pub struct QuantumBayesianCPT {
    /// Quantum circuit template (will be parameterized by parent states).
    pub circuit: QuantumCircuit,
    /// Mapping from parent state indices to circuit parameter indices.
    pub parent_to_param: Vec<(usize, usize)>, // (parent_idx, param_idx)
    /// Number of output categories (measurement outcomes).
    pub num_categories: usize,
}

impl QuantumBayesianCPT {
    pub fn new(circuit: QuantumCircuit, num_categories: usize) -> Self {
        Self {
            circuit,
            parent_to_param: Vec::new(),
            num_categories,
        }
    }

    /// Add a connection: parent_state → circuit parameter.
    pub fn connect_parent(&mut self, parent_idx: usize, param_idx: usize) {
        self.parent_to_param.push((parent_idx, param_idx));
    }

    /// Compute the CPT row for given parent values.
    pub fn compute_probabilities(&self, parent_values: &[f64]) -> Result<Vec<f64>, String> {
        let mut qc = self.circuit.clone();
        for &(parent_idx, param_idx) in &self.parent_to_param {
            if parent_idx < parent_values.len() {
                qc.set_param(param_idx, parent_values[parent_idx]);
            }
        }
        let probs = probabilities(&qc)?;
        let categories = probs.iter().take(self.num_categories).copied().collect();
        Ok(categories)
    }
}

// ---------------------------------------------------------------------------
// Quantum Variational Optimizer
// ---------------------------------------------------------------------------

/// Optimize a variational quantum circuit using parameter-shift gradients
/// combined with classical optimizers from scirust-autodiff.
pub struct QuantumVariationalOptimizer {
    pub circuit: QuantumCircuit,
    pub learning_rate: f64,
    pub max_iterations: usize,
}

impl QuantumVariationalOptimizer {
    pub fn new(circuit: QuantumCircuit, learning_rate: f64) -> Self {
        Self { circuit, learning_rate, max_iterations: 100 }
    }

    pub fn with_max_iterations(mut self, n: usize) -> Self {
        self.max_iterations = n;
        self
    }

    /// Optimize to maximize ⟨Z₀⟩.
    pub fn optimize(&mut self) -> Result<Vec<f64>, String> {
        let n = self.circuit.num_params();
        let mut params = self.circuit.get_params().to_vec();
        // Copy initial params back
        for i in 0..n {
            self.circuit.set_param(i, params[i]);
        }

        for iteration in 0..self.max_iterations {
            let grads = crate::gradient::full_gradient(&self.circuit, &params)?;

            // Gradient ascent (maximize expectation)
            for i in 0..n {
                params[i] += self.learning_rate * grads[i];
                self.circuit.set_param(i, params[i]);
            }

            // Check convergence
            let grad_norm: f64 = grads.iter().map(|g| g * g).sum::<f64>().sqrt();
            if grad_norm < 1e-6 {
                break;
            }
        }

        Ok(params)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_pipeline_creation() {
        let layers = vec![
            LayerConfig { input_size: 3, output_size: 5, activation: "relu".to_string() },
            LayerConfig { input_size: 5, output_size: 2, activation: "sigmoid".to_string() },
        ];
        let pipeline = QuantumHybridPipeline::new(3, EncodingType::Angle, layers);
        assert_eq!(pipeline.num_measurements, 3);
        assert!(pipeline.model.is_some());
    }

    #[test]
    fn test_encode_and_measure() {
        let pipeline = QuantumHybridPipeline::new(2, EncodingType::Angle, vec![]);
        let features = vec![0.3, 0.7];
        let measurements = pipeline.encode_and_measure(&features).unwrap();
        // 2 qubits → 4 basis states
        assert_eq!(measurements.len(), 4);
        let sum: f64 = measurements.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10, "probs sum={}", sum);
    }

    #[test]
    fn test_quantum_bayesian_cpt() {
        let mut circuit = QuantumCircuit::new_variational(1, &["theta"]);
        circuit.ry(0, 0);
        let mut cpt = QuantumBayesianCPT::new(circuit, 2);
        cpt.connect_parent(0, 0);

        let probs = cpt.compute_probabilities(&[1.0]).unwrap();
        assert_eq!(probs.len(), 2);
        let sum: f64 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_hybrid_pipeline_run() {
        let pipeline = QuantumHybridPipeline::new(2, EncodingType::Angle, vec![]);
        let output = pipeline.run(&[0.5, 0.8]).unwrap();
        assert_eq!(output.num_qubits, 2);
        assert_eq!(output.measurements.len(), 4);
    }

    #[test]
    fn test_variational_optimizer_convergence() {
        // Simple circuit: RY(θ)|0⟩ → maximize ⟨Z₀⟩ = cos(θ)
        // Maximum at θ=0 where ⟨Z₀⟩=1
        let mut circuit = QuantumCircuit::new_variational(1, &["theta"]);
        circuit.ry(0, 0);
        circuit.set_param(0, 2.0); // start far from optimum

        let mut opt = QuantumVariationalOptimizer::new(circuit, 0.1);
        opt.max_iterations = 50;
        let final_params = opt.optimize().unwrap();

        // Should converge toward 0 (mod 2π)
        let final_theta = final_params[0] % (2.0 * std::f64::consts::PI);
        assert!(final_theta.abs() < 0.5 || (final_theta - 2.0 * std::f64::consts::PI).abs() < 0.5,
            "theta should converge to 0, got {}", final_theta);
    }
}
