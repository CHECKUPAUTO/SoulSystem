//! Quantum backend abstraction layer.
//!
//! Defines a `QuantumBackend` trait that abstracts over different
//! execution providers: local simulator, IBMQ, AWS Braket, etc.
//!
//! # Usage
//!
//! ```ignore
//! use scirust_quantum::backend::{QuantumBackend, SimulatorBackend};
//!
//! // Local simulation (default, no quantum hardware needed)
//! let backend = SimulatorBackend::new();
//! let counts = backend.run(circuit, 1000)?;
//!
//! // IBMQ (requires credentials and network)
//! let ibmq = IBMQBackend::new("ibm_sherbrooke", "your_token");
//! let counts = ibmq.run(circuit, 1000)?;
//! ```

use crate::circuit::QuantumCircuit;

/// Measurement result from running a circuit.
#[derive(Debug, Clone)]
pub struct QuantumResult {
    /// Bitstring counts: "000" → count, "001" → count, etc.
    pub counts: std::collections::HashMap<String, usize>,
    /// Total number of shots
    pub shots: usize,
    /// Execution time in milliseconds
    pub execution_time_ms: f64,
    /// Backend name
    pub backend_name: String,
}

impl QuantumResult {
    /// Get the probability of a specific bitstring.
    pub fn probability_of(&self, bitstring: &str) -> f64 {
        *self.counts.get(bitstring).unwrap_or(&0) as f64 / self.shots as f64
    }

    /// Get the most frequent bitstring.
    pub fn most_frequent(&self) -> Option<String> {
        self.counts.iter()
            .max_by_key(|(_, &c)| c)
            .map(|(k, _)| k.clone())
    }
}

/// Trait for quantum execution backends.
pub trait QuantumBackend {
    /// Run a circuit and return measurement results.
    fn run(&self, circuit: &QuantumCircuit, shots: usize) -> Result<QuantumResult, String>;

    /// Get the backend name.
    fn name(&self) -> &str;

    /// Maximum number of qubits supported.
    fn max_qubits(&self) -> usize;

    /// Whether this is a simulated backend.
    fn is_simulator(&self) -> bool { true }
}

// ---------------------------------------------------------------------------
// Simulator backend (local, no quantum hardware)
// ---------------------------------------------------------------------------

/// Local simulator backend using the state-vector simulator.
pub struct SimulatorBackend;

impl SimulatorBackend {
    pub fn new() -> Self { Self }
}

impl QuantumBackend for SimulatorBackend {
    fn run(&self, circuit: &QuantumCircuit, shots: usize) -> Result<QuantumResult, String> {
        let start = std::time::Instant::now();
        let mut counts = std::collections::HashMap::new();
        let n = circuit.num_qubits;

        for _ in 0..shots {
            let state = crate::simulator::simulate(circuit)?;
            let mut s = state.clone();
            let bits = crate::simulator::measure_all(&mut s, n, &mut fast_rng);
            let bitstr: String = bits.iter().map(|b| format!("{}", b)).collect();
            *counts.entry(bitstr).or_insert(0) += 1;
        }

        let elapsed = start.elapsed().as_secs_f64() * 1000.0;

        Ok(QuantumResult {
            counts,
            shots,
            execution_time_ms: elapsed,
            backend_name: "simulator".to_string(),
        })
    }

    fn name(&self) -> &str { "simulator" }
    fn max_qubits(&self) -> usize { 20 }
    fn is_simulator(&self) -> bool { true }
}

fn fast_rng() -> f64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(12345);
    let mut s = SEED.fetch_add(1, Ordering::Relaxed);
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    (s as f64) / (u64::MAX as f64)
}

// ---------------------------------------------------------------------------
// IBMQ backend stub (requires qisket or REST API)
// ---------------------------------------------------------------------------

/// IBM Quantum backend stub.
///
/// To use with a real IBMQ backend:
/// 1. Get an API token from https://quantum-computing.ibm.com/
/// 2. Set the `IBMQ_TOKEN` environment variable
/// 3. Replace the stub implementation with actual REST API calls
pub struct IBMQBackend {
    hub: String,
    token: String,
}

impl IBMQBackend {
    /// Create a new IBMQ backend.
    ///
    /// # Arguments
    /// * `hub` — IBMQ hub/group/project or device name (e.g. "ibm_sherbrooke")
    /// * `token` — IBMQ API token
    pub fn new(hub: &str, token: &str) -> Self {
        Self { hub: hub.to_string(), token: token.to_string() }
    }

    /// Create from environment variables.
    pub fn from_env() -> Result<Self, String> {
        let token = std::env::var("IBMQ_TOKEN")
            .map_err(|_| "IBMQ_TOKEN not set".to_string())?;
        let hub = std::env::var("IBMQ_HUB")
            .unwrap_or_else(|_| "ibm_sherbrooke".to_string());
        Ok(Self::new(&hub, &token))
    }
}

impl QuantumBackend for IBMQBackend {
    fn run(&self, _circuit: &QuantumCircuit, _shots: usize) -> Result<QuantumResult, String> {
        Err("IBMQ backend requires network access and qiskit bindings. \
             Set IBMQ_TOKEN and IBMQ_HUB environment variables, or use \
             SimulatorBackend for local execution."
            .to_string())
    }

    fn name(&self) -> &str { &self.hub }
    fn max_qubits(&self) -> usize { 127 } // IBM Heron processor
    fn is_simulator(&self) -> bool { false }
}

// ---------------------------------------------------------------------------
// AWS Braket backend stub
// ---------------------------------------------------------------------------

/// AWS Braket backend stub.
///
/// To use with real Braket hardware:
/// 1. Configure AWS credentials
/// 2. Replace the stub with actual Braket API calls
pub struct BraketBackend {
    device_arn: String,
    region: String,
}

impl BraketBackend {
    pub fn new(device_arn: &str, region: &str) -> Self {
        Self { device_arn: device_arn.to_string(), region: region.to_string() }
    }
}

impl QuantumBackend for BraketBackend {
    fn run(&self, _circuit: &QuantumCircuit, _shots: usize) -> Result<QuantumResult, String> {
        Err("AWS Braket backend requires network access and AWS SDK bindings. \
             Use SimulatorBackend for local execution."
            .to_string())
    }

    fn name(&self) -> &str { "aws_braket" }
    fn max_qubits(&self) -> usize { 100 }
    fn is_simulator(&self) -> bool { false }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::QuantumCircuit;

    #[test]
    fn test_simulator_backend() {
        let backend = SimulatorBackend::new();
        let qc = QuantumCircuit::bell_state();
        let result = backend.run(&qc, 100).unwrap();
        assert_eq!(result.shots, 100);
        assert!(!result.counts.is_empty());
        assert!(result.execution_time_ms > 0.0);
        assert_eq!(result.backend_name, "simulator");
    }

    #[test]
    fn test_most_frequent() {
        let mut counts = std::collections::HashMap::new();
        counts.insert("00".to_string(), 100);
        counts.insert("11".to_string(), 50);
        let result = QuantumResult {
            counts,
            shots: 150,
            execution_time_ms: 1.0,
            backend_name: "test".to_string(),
        };
        assert_eq!(result.most_frequent(), Some("00".to_string()));
        assert!((result.probability_of("00") - 100.0 / 150.0).abs() < 1e-10);
    }

    #[test]
    fn test_ibmq_stub() {
        let backend = IBMQBackend::new("ibm_test", "dummy_token");
        assert!(!backend.is_simulator());
        let qc = QuantumCircuit::new(1);
        let result = backend.run(&qc, 100);
        assert!(result.is_err()); // No real IBMQ available
    }

    #[test]
    fn test_braket_stub() {
        let backend = BraketBackend::new("arn:aws:braket:device", "us-east-1");
        assert!(!backend.is_simulator());
    }

    #[test]
    fn test_result_probability() {
        let mut counts = std::collections::HashMap::new();
        counts.insert("0".to_string(), 75);
        counts.insert("1".to_string(), 25);
        let result = QuantumResult {
            counts, shots: 100,
            execution_time_ms: 0.5, backend_name: "test".to_string(),
        };
        assert!((result.probability_of("0") - 0.75).abs() < 1e-10);
        assert!((result.probability_of("1") - 0.25).abs() < 1e-10);
        assert!((result.probability_of("2") - 0.0).abs() < 1e-10);
    }
}
