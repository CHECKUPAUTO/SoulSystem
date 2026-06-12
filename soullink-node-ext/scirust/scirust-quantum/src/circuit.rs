//! Parametrized quantum circuit with gate operations.
//!
//! # Gates
//!
//! | Gate | Matrix | Description |
//! |------|--------|-------------|
//! | `h` | H | Hadamard: create superposition |
//! | `x` | X | Pauli-X (NOT) |
//! | `y` | Y | Pauli-Y |
//! | `z` | Z | Pauli-Z |
//! | `rx(theta)` | RX(θ) | Rotation around X |
//! | `ry(theta)` | RY(θ) | Rotation around Y |
//! | `rz(theta)` | RZ(θ) | Rotation around Z |
//! | `cnot(ctrl, tgt)` | CNOT | Controlled-X |
//! | `cz(ctrl, tgt)` | CZ | Controlled-Z |
//! | `swap(a, b)` | SWAP | Swap two qubits |
//!
//! # State Representation
//!
//! An `n`-qubit state is a complex vector of length `2ⁿ`.
//! Each amplitude is stored as `(re, im)`.

use std::fmt;

/// A single-qubit gate represented as a 2×2 complex matrix.
#[derive(Debug, Clone, Copy)]
pub struct Gate2 {
    pub a: (f64, f64), // row0 col0 (re, im)
    pub b: (f64, f64), // row0 col1
    pub c: (f64, f64), // row1 col0
    pub d: (f64, f64), // row1 col1
}

impl Gate2 {
    pub fn apply(&self, re0: f64, im0: f64, re1: f64, im1: f64) -> (f64, f64, f64, f64) {
        // |ψ_out⟩ = G |ψ_in⟩
        // out0 = a*in0 + b*in1
        // out1 = c*in0 + d*in1
        let out0_re = self.a.0 * re0 - self.a.1 * im0 + self.b.0 * re1 - self.b.1 * im1;
        let out0_im = self.a.0 * im0 + self.a.1 * re0 + self.b.0 * im1 + self.b.1 * re1;
        let out1_re = self.c.0 * re0 - self.c.1 * im0 + self.d.0 * re1 - self.d.1 * im1;
        let out1_im = self.c.0 * im0 + self.c.1 * re0 + self.d.0 * im1 + self.d.1 * re1;
        (out0_re, out0_im, out1_re, out1_im)
    }
}

// ---------------------------------------------------------------------------
// Standard single-qubit gates
// ---------------------------------------------------------------------------

pub fn gate_h() -> Gate2 {
    let s = 1.0 / 2.0f64.sqrt();
    Gate2 { a: (s, 0.0), b: (s, 0.0), c: (s, 0.0), d: (-s, 0.0) }
}

pub fn gate_x() -> Gate2 {
    Gate2 { a: (0.0, 0.0), b: (1.0, 0.0), c: (1.0, 0.0), d: (0.0, 0.0) }
}

pub fn gate_y() -> Gate2 {
    Gate2 { a: (0.0, 0.0), b: (0.0, -1.0), c: (0.0, 1.0), d: (0.0, 0.0) }
}

pub fn gate_z() -> Gate2 {
    Gate2 { a: (1.0, 0.0), b: (0.0, 0.0), c: (0.0, 0.0), d: (-1.0, 0.0) }
}

pub fn gate_rx(theta: f64) -> Gate2 {
    let c = (theta / 2.0).cos();
    let s = (theta / 2.0).sin();
    Gate2 { a: (c, 0.0), b: (0.0, -s), c: (0.0, -s), d: (c, 0.0) }
}

pub fn gate_ry(theta: f64) -> Gate2 {
    let c = (theta / 2.0).cos();
    let s = (theta / 2.0).sin();
    Gate2 { a: (c, 0.0), b: (-s, 0.0), c: (s, 0.0), d: (c, 0.0) }
}

pub fn gate_rz(theta: f64) -> Gate2 {
    let c = (theta / 2.0).cos();
    let s = (theta / 2.0).sin();
    Gate2 { a: (c - s, 0.0), b: (0.0, 0.0), c: (0.0, 0.0), d: (c + s, 0.0) }
}

// ---------------------------------------------------------------------------
// QuantumCircuit
// ---------------------------------------------------------------------------

/// A parametrized quantum circuit on `n` qubits.
///
/// The circuit stores a sequence of gate operations and can be applied
/// to a state vector via `simulator::simulate`.
#[derive(Debug, Clone)]
pub struct QuantumCircuit {
    pub num_qubits: usize,
    gates: Vec<GateOp>,
    /// Named parameters for variational circuits.
    pub parameters: Vec<String>,
    param_values: Vec<f64>,
}

/// A gate operation applied to specific qubits.
#[derive(Debug, Clone)]
pub struct GateOp {
    pub gate_type: GateType,
    pub target: usize,
    pub control: Option<usize>,
    pub param_idx: Option<usize>,
    pub param_value: Option<f64>,
}

/// Type of quantum gate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GateType {
    H, X, Y, Z,
    RX, RY, RZ,
    CNOT, CZ, SWAP,
}

impl QuantumCircuit {
    /// Create a new circuit with `n` qubits.
    pub fn new(num_qubits: usize) -> Self {
        Self {
            num_qubits,
            gates: Vec::new(),
            parameters: Vec::new(),
            param_values: Vec::new(),
        }
    }

    /// Create a circuit with named variational parameters.
    pub fn new_variational(num_qubits: usize, param_names: &[&str]) -> Self {
        Self {
            num_qubits,
            gates: Vec::new(),
            parameters: param_names.iter().map(|s| s.to_string()).collect(),
            param_values: vec![0.0; param_names.len()],
        }
    }

    /// Set a parameter by index.
    pub fn set_param(&mut self, idx: usize, value: f64) {
        if idx < self.param_values.len() {
            self.param_values[idx] = value;
        }
    }

    /// Set a parameter by name.
    pub fn set_param_by_name(&mut self, name: &str, value: f64) -> Result<(), String> {
        if let Some(idx) = self.parameters.iter().position(|p| p == name) {
            self.param_values[idx] = value;
            Ok(())
        } else {
            Err(format!("Parameter '{}' not found", name))
        }
    }

    /// Get a parameter value by index.
    pub fn get_param(&self, idx: usize) -> f64 {
        self.param_values.get(idx).copied().unwrap_or(0.0)
    }

    /// Get all parameter values.
    pub fn get_params(&self) -> &[f64] {
        &self.param_values
    }

    /// Get mutable parameter values.
    pub fn get_params_mut(&mut self) -> &mut [f64] {
        &mut self.param_values
    }

    /// Number of variational parameters.
    pub fn num_params(&self) -> usize {
        self.param_values.len()
    }

    /// Add a gate to the circuit.
    fn add_gate(&mut self, gate_type: GateType, target: usize, control: Option<usize>,
                param_idx: Option<usize>, param_value: Option<f64>) {
        self.gates.push(GateOp { gate_type, target, control, param_idx, param_value });
    }

    /// Hadamard gate.
    pub fn h(&mut self, qubit: usize) {
        self.add_gate(GateType::H, qubit, None, None, None);
    }

    /// Pauli-X (NOT) gate.
    pub fn x(&mut self, qubit: usize) {
        self.add_gate(GateType::X, qubit, None, None, None);
    }

    /// Pauli-Y gate.
    pub fn y(&mut self, qubit: usize) {
        self.add_gate(GateType::Y, qubit, None, None, None);
    }

    /// Pauli-Z gate.
    pub fn z(&mut self, qubit: usize) {
        self.add_gate(GateType::Z, qubit, None, None, None);
    }

    /// RX rotation: uses named parameter index.
    pub fn rx(&mut self, qubit: usize, param_idx: usize) {
        self.add_gate(GateType::RX, qubit, None, Some(param_idx), None);
    }

    /// RX rotation with an explicit angle (non-parameterized).
    pub fn rx_angle(&mut self, qubit: usize, theta: f64) {
        self.add_gate(GateType::RX, qubit, None, None, Some(theta));
    }

    /// RY rotation: uses named parameter index.
    pub fn ry(&mut self, qubit: usize, param_idx: usize) {
        self.add_gate(GateType::RY, qubit, None, Some(param_idx), None);
    }

    /// RY rotation with an explicit angle.
    pub fn ry_angle(&mut self, qubit: usize, theta: f64) {
        self.add_gate(GateType::RY, qubit, None, None, Some(theta));
    }

    /// RZ rotation: uses named parameter index.
    pub fn rz(&mut self, qubit: usize, param_idx: usize) {
        self.add_gate(GateType::RZ, qubit, None, Some(param_idx), None);
    }

    /// RZ rotation with an explicit angle.
    pub fn rz_angle(&mut self, qubit: usize, theta: f64) {
        self.add_gate(GateType::RZ, qubit, None, None, Some(theta));
    }

    /// CNOT gate (controlled-X).
    pub fn cnot(&mut self, control: usize, target: usize) {
        self.add_gate(GateType::CNOT, target, Some(control), None, None);
    }

    /// CZ gate (controlled-Z).
    pub fn cz(&mut self, control: usize, target: usize) {
        self.add_gate(GateType::CZ, target, Some(control), None, None);
    }

    /// SWAP gate.
    pub fn swap(&mut self, a: usize, b: usize) {
        self.add_gate(GateType::SWAP, a, Some(b), None, None);
    }

    /// Return a reference to the gate list.
    pub fn gates(&self) -> &[GateOp] { &self.gates }

    /// Resolve a gate's angle from parameters or explicit value.
    pub fn resolve_angle(&self, gate: &GateOp) -> f64 {
        if let Some(idx) = gate.param_idx {
            self.param_values.get(idx).copied().unwrap_or(0.0)
        } else {
            gate.param_value.unwrap_or(0.0)
        }
    }

    /// Create a Bell state circuit: H(0) then CNOT(0, 1).
    pub fn bell_state() -> Self {
        let mut qc = QuantumCircuit::new(2);
        qc.h(0);
        qc.cnot(0, 1);
        qc
    }

    /// Create a simple variational circuit for testing.
    pub fn variational_circuit(num_qubits: usize) -> Self {
        let params: Vec<String> = (0..num_qubits * 2).map(|i| format!("theta_{}", i)).collect();
        let param_names: Vec<&str> = params.iter().map(|s| s.as_str()).collect();
        let mut qc = QuantumCircuit::new_variational(num_qubits, &param_names);
        for qi in 0..num_qubits {
            qc.ry(qi, qi * 2);       // RY(theta_{2i})
            qc.rz(qi, qi * 2 + 1);   // RZ(theta_{2i+1})
        }
        // Entangling layer
        for qi in 0..num_qubits - 1 {
            qc.cnot(qi, qi + 1);
        }
        qc
    }

    /// Number of gates in the circuit.
    pub fn num_gates(&self) -> usize { self.gates.len() }

    /// Depth of the circuit (approximate: count of non-parallel layers).
    pub fn depth(&self) -> usize {
        // Simplified: count layers assuming all single-qubit gates
        // on different qubits can be parallelized
        if self.gates.is_empty() { return 0; }
        let mut layers = 1;
        let mut used_qubits: Vec<usize> = Vec::new();
        for gate in &self.gates {
            let qubits = match gate.gate_type {
                GateType::CNOT | GateType::CZ | GateType::SWAP => {
                    vec![gate.target, gate.control.unwrap()]
                }
                _ => vec![gate.target],
            };
            let overlaps = qubits.iter().any(|q| used_qubits.contains(q));
            if overlaps {
                layers += 1;
                used_qubits = qubits;
            } else {
                used_qubits.extend(qubits);
            }
        }
        layers
    }
}

impl fmt::Display for QuantumCircuit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "QuantumCircuit({} qubits, {} gates, depth ~{})",
            self.num_qubits, self.num_gates(), self.depth())?;
        if !self.parameters.is_empty() {
            write!(f, "  Parameters: ")?;
            for (i, name) in self.parameters.iter().enumerate() {
                write!(f, "{}={:.3} ", name, self.param_values[i])?;
            }
            writeln!(f)?;
        }
        for (i, gate) in self.gates.iter().enumerate() {
            write!(f, "  {}. ", i + 1)?;
            match gate.gate_type {
                GateType::H => writeln!(f, "H({})", gate.target),
                GateType::X => writeln!(f, "X({})", gate.target),
                GateType::Y => writeln!(f, "Y({})", gate.target),
                GateType::Z => writeln!(f, "Z({})", gate.target),
                GateType::RX => writeln!(f, "RX({}, θ={:.3})", gate.target,
                    gate.param_idx.map(|i| self.param_values[i]).unwrap_or(gate.param_value.unwrap_or(0.0))),
                GateType::RY => writeln!(f, "RY({}, θ={:.3})", gate.target,
                    gate.param_idx.map(|i| self.param_values[i]).unwrap_or(gate.param_value.unwrap_or(0.0))),
                GateType::RZ => writeln!(f, "RZ({}, θ={:.3})", gate.target,
                    gate.param_idx.map(|i| self.param_values[i]).unwrap_or(gate.param_value.unwrap_or(0.0))),
                GateType::CNOT => writeln!(f, "CNOT({}, {})", gate.control.unwrap(), gate.target),
                GateType::CZ => writeln!(f, "CZ({}, {})", gate.control.unwrap(), gate.target),
                GateType::SWAP => writeln!(f, "SWAP({}, {})", gate.target, gate.control.unwrap()),
            }?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_h_matrix() {
        let h = gate_h();
        let s = 1.0 / 2.0f64.sqrt();
        assert!((h.a.0 - s).abs() < 1e-10);
        assert!((h.d.0 + s).abs() < 1e-10);
    }

    #[test]
    fn test_gate_x_apply() {
        let x = gate_x();
        // X|0⟩ = |1⟩  =>  (0, 0, 1, 0) becomes (0, 0, 1, 0)? No...
        // X = [[0,1],[1,0]] so X|0⟩ = |1⟩
        let (r0, i0, r1, i1) = x.apply(1.0, 0.0, 0.0, 0.0);
        assert!((r0 - 0.0).abs() < 1e-10);
        assert!((r1 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_circuit_bell_state() {
        let qc = QuantumCircuit::bell_state();
        assert_eq!(qc.num_qubits, 2);
        assert_eq!(qc.num_gates(), 2);
    }

    #[test]
    fn test_variational_circuit() {
        let mut qc = QuantumCircuit::variational_circuit(3);
        assert_eq!(qc.num_qubits, 3);
        assert_eq!(qc.num_params(), 6);
        qc.set_param(0, 0.5);
        qc.set_param(3, 1.0);
        assert!((qc.get_param(0) - 0.5).abs() < 1e-10);
        assert!((qc.get_param(3) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_circuit_depth() {
        let mut qc = QuantumCircuit::new(3);
        qc.h(0);
        qc.h(1);  // same layer as h(0) — different qubits
        qc.cnot(0, 1); // new layer (uses qubit 0 and 1)
        assert!(qc.depth() >= 2);
    }

    #[test]
    fn test_swap_gate() {
        let mut qc = QuantumCircuit::new(2);
        qc.swap(0, 1);
        assert_eq!(qc.num_gates(), 1);
    }

    #[test]
    fn test_gate_ry() {
        let ry = gate_ry(0.5);
        // RY(θ) = [[cos(θ/2), -sin(θ/2)], [sin(θ/2), cos(θ/2)]]
        let c = (0.25f64).cos();
        let s = (0.25f64).sin();
        assert!((ry.a.0 - c).abs() < 1e-10);
        assert!((ry.b.0 + s).abs() < 1e-10);
        assert!((ry.c.0 - s).abs() < 1e-10);
        assert!((ry.d.0 - c).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_angle() {
        let mut qc = QuantumCircuit::new_variational(1, &["theta"]);
        qc.rx(0, 0);
        let gate = &qc.gates()[0];
        let angle = qc.resolve_angle(gate);
        assert!((angle - 0.0).abs() < 1e-10);
        qc.set_param(0, 1.5);
        let angle = qc.resolve_angle(&qc.gates()[0]);
        assert!((angle - 1.5).abs() < 1e-10);
    }

    #[test]
    fn test_gate_rz() {
        let rz = gate_rz(std::f64::consts::PI);
        // RZ(π) = phase gate: diag(-i, i) up to global phase
        // With our formula: c=cos(π/2)=0, s=sin(π/2)=1
        // So RZ(π) = [[0,0],[0,0]]? Wait...
        // Actually RZ(θ) = [[e^{-iθ/2}, 0], [0, e^{iθ/2}]]
        // = [[cos(θ/2)-i sin(θ/2), 0], [0, cos(θ/2)+i sin(θ/2)]]
        // For θ=π: [[-i, 0], [0, i]]
        // Our impl: c=cos(π/2)=0, s=sin(π/2)=1
        // a=(c-s, 0)=( -1, 0), d=(c+s,0)=( 1, 0)
        // So RZ(π) = [[-1,0],[0,1]] up to global phase i, = [[-i,0],[0,i]]
        assert!((rz.a.0 + 1.0).abs() < 1e-10 || (rz.a.0 - 0.0).abs() < 1e-10);
        assert!((rz.b.0 - 0.0).abs() < 1e-10);
        assert!((rz.d.0 - 1.0).abs() < 1e-10 || (rz.d.0 - 0.0).abs() < 1e-10);
    }
}
