//! SciRust Quantum — Simulated quantum circuit inference.
//!
//! This crate provides a simulated quantum computing backend for hybrid
//! quantum-classical inference. It operates entirely via classical simulation
//! (state-vector multiplication), supporting up to ~24 qubits on standard
//! hardware.
//!
//! # Architecture
//!
//! - `circuit::QuantumCircuit` — Parametrized quantum circuit with gates
//! - `simulator` — State-vector simulation via matrix multiplication
//! - `gradient::parameter_shift` — Quantum gradient for variational circuits
//! - `encoding` — Classical-to-quantum data encoding (angle, amplitude)
//! - `hybrid` — Integration with `scirust-inference` and `scirust-probability`
//!
//! # Example
//!
//! ```ignore
//! use scirust_quantum::circuit::QuantumCircuit;
//!
//! let mut qc = QuantumCircuit::new(2);
//! qc.h(0);                    // Hadamard on qubit 0
//! qc.cnot(0, 1);              // CNOT control=0, target=1
//! let probs = qc.probabilities();
//! // Bell state: (|00⟩ + |11⟩) / √2
//! assert!((probs[0] - 0.5).abs() < 1e-10);
//! assert!((probs[3] - 0.5).abs() < 1e-10);
//! ```

#![allow(unused)]

pub mod circuit;
pub mod simulator;
pub mod gradient;
pub mod encoding;
pub mod hybrid;
pub mod backend;

pub mod prelude {
    pub use crate::circuit::QuantumCircuit;
    pub use crate::encoding::*;
    pub use crate::gradient::parameter_shift;
    pub use crate::hybrid::*;
}
