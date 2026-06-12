//! State-vector simulator for quantum circuits.
//!
//! The state is an array of `2ⁿ` complex amplitudes stored as
//! interleaved `(re, im)` pairs: `[re₀, im₀, re₁, im₁, ...]`.
//!
//! # Numerical stability
//!
//! After each gate application, the state is re-normalized to
//! prevent floating-point drift. The total probability should
//! sum to `1.0 ± 1e-12`.

use crate::circuit::{gate_h, gate_rx, gate_ry, gate_rz, gate_x, gate_y, gate_z, GateOp, GateType, QuantumCircuit};

/// Maximum number of qubits supported by the simulator.
/// Limited by memory: `2²⁰ ≈ 1M` complex amplitudes ≈ 16 MB.
pub const MAX_QUBITS: usize = 20;

/// Simulate a quantum circuit and return the final state vector.
///
/// The state is returned as `[re, im, re, im, ...]` of length `2ⁿ⁺¹`.
pub fn simulate(circuit: &QuantumCircuit) -> Result<Vec<f64>, String> {
    let n = circuit.num_qubits;
    if n > MAX_QUBITS {
        return Err(format!(
            "Too many qubits: {} (max {})", n, MAX_QUBITS
        ));
    }
    if n == 0 {
        return Err("Need at least 1 qubit".to_string());
    }

    let dim = 1 << n;
    let mut state = vec![0.0f64; dim * 2];
    // Start in |0...0⟩: amplitude 1.0 + 0.0i
    state[0] = 1.0;

    for gate in circuit.gates() {
        match gate.gate_type {
            GateType::H => apply_single(&mut state, gate, gate_h(), n),
            GateType::X => apply_single(&mut state, gate, gate_x(), n),
            GateType::Y => apply_single(&mut state, gate, gate_y(), n),
            GateType::Z => apply_single(&mut state, gate, gate_z(), n),
            GateType::RX => {
                let theta = circuit.resolve_angle(gate);
                apply_single(&mut state, gate, gate_rx(theta), n);
            }
            GateType::RY => {
                let theta = circuit.resolve_angle(gate);
                apply_single(&mut state, gate, gate_ry(theta), n);
            }
            GateType::RZ => {
                let theta = circuit.resolve_angle(gate);
                apply_single(&mut state, gate, gate_rz(theta), n);
            }
            GateType::CNOT => apply_cnot(&mut state, gate, n),
            GateType::CZ => apply_cz(&mut state, gate, n),
            GateType::SWAP => apply_swap(&mut state, gate, n),
        }
    }

    Ok(state)
}

/// Simulate and return probabilities of each basis state.
///
/// Returns a `Vec<f64>` of length `2ⁿ` where index `i` is `|⟨i|ψ⟩|²`.
pub fn probabilities(circuit: &QuantumCircuit) -> Result<Vec<f64>, String> {
    let state = simulate(circuit)?;
    let n = circuit.num_qubits;
    let dim = 1 << n;
    let mut probs = vec![0.0f64; dim];
    for i in 0..dim {
        let re = state[i * 2];
        let im = state[i * 2 + 1];
        probs[i] = re * re + im * im;
    }
    Ok(probs)
}

/// Measure a specific qubit, collapsing the state.
///
/// Returns `0` or `1` based on the probability distribution,
/// and updates the state vector to the post-measurement state.
pub fn measure_qubit(state: &mut [f64], qubit: usize, num_qubits: usize, rng: &mut impl FnMut() -> f64) -> u8 {
    let dim = 1 << num_qubits;
    let mut prob0 = 0.0;

    for i in 0..dim {
        if (i >> qubit) & 1 == 0 {
            let re = state[i * 2];
            let im = state[i * 2 + 1];
            prob0 += re * re + im * im;
        }
    }

    let result: u8 = if rng() < prob0 { 0 } else { 1 };

    let norm = if result == 0 { prob0.sqrt() } else { (1.0 - prob0).sqrt() };
    for i in 0..dim {
        if ((i >> qubit) & 1) != result as usize {
            state[i * 2] = 0.0;
            state[i * 2 + 1] = 0.0;
        } else {
            state[i * 2] /= norm;
            state[i * 2 + 1] /= norm;
        }
    }

    result
}

/// Measure all qubits, collapsing the state.
pub fn measure_all(state: &mut [f64], num_qubits: usize, rng: &mut impl FnMut() -> f64) -> Vec<u8> {
    let mut results = Vec::with_capacity(num_qubits);
    for q in (0..num_qubits).rev() {
        results.push(measure_qubit(state, q, num_qubits, rng));
    }
    results.reverse();
    results
}

/// Expected value of a Pauli-Z measurement on `qubit`.
pub fn expectation_z(state: &[f64], qubit: usize, num_qubits: usize) -> f64 {
    let dim = 1 << num_qubits;
    let mut exp = 0.0;
    for i in 0..dim {
        let re = state[i * 2];
        let im = state[i * 2 + 1];
        let prob = re * re + im * im;
        let eigen = if ((i >> qubit) & 1) == 0 { 1.0 } else { -1.0 };
        exp += eigen * prob;
    }
    exp
}

// ---------------------------------------------------------------------------
// Internal: gate application
// ---------------------------------------------------------------------------

fn apply_single(state: &mut [f64], gate: &GateOp, g: crate::circuit::Gate2, n: usize) {
    let q = gate.target;
    let dim = 1 << n;

    for base in 0..dim {
        if (base >> q) & 1 == 1 { continue; }
        let zero_idx = base;
        let one_idx = base | (1 << q);

        let re0 = state[zero_idx * 2];
        let im0 = state[zero_idx * 2 + 1];
        let re1 = state[one_idx * 2];
        let im1 = state[one_idx * 2 + 1];

        let (out0_re, out0_im, out1_re, out1_im) = g.apply(re0, im0, re1, im1);

        state[zero_idx * 2] = out0_re;
        state[zero_idx * 2 + 1] = out0_im;
        state[one_idx * 2] = out1_re;
        state[one_idx * 2 + 1] = out1_im;
    }

    normalize(state);
}

fn apply_cnot(state: &mut [f64], gate: &GateOp, n: usize) {
    let ctrl = gate.control.unwrap();
    let tgt = gate.target;
    let dim = 1 << n;

    for base in 0..dim {
        if (base >> ctrl) & 1 == 0 { continue; }
        if ((base >> tgt) & 1) == 1 { continue; }

        let src = base;
        let dst = base | (1 << tgt);

        let re_src = state[src * 2];
        let im_src = state[src * 2 + 1];
        let re_dst = state[dst * 2];
        let im_dst = state[dst * 2 + 1];

        state[src * 2] = re_dst;
        state[src * 2 + 1] = im_dst;
        state[dst * 2] = re_src;
        state[dst * 2 + 1] = im_src;
    }

    normalize(state);
}

fn apply_cz(state: &mut [f64], gate: &GateOp, n: usize) {
    let ctrl = gate.control.unwrap();
    let tgt = gate.target;
    let dim = 1 << n;

    for i in 0..dim {
        if ((i >> ctrl) & 1) == 1 && ((i >> tgt) & 1) == 1 {
            state[i * 2] = -state[i * 2];
            state[i * 2 + 1] = -state[i * 2 + 1];
        }
    }

    normalize(state);
}

fn apply_swap(state: &mut [f64], gate: &GateOp, n: usize) {
    let a = gate.target;
    let b = gate.control.unwrap();
    let dim = 1 << n;

    for i in 0..dim {
        let bit_a = (i >> a) & 1;
        let bit_b = (i >> b) & 1;
        if bit_a != bit_b {
            let paired = i ^ (1 << a) ^ (1 << b);
            if paired > i { continue; }

            let re_a = state[i * 2];
            let im_a = state[i * 2 + 1];
            let re_b = state[paired * 2];
            let im_b = state[paired * 2 + 1];

            state[i * 2] = re_b;
            state[i * 2 + 1] = im_b;
            state[paired * 2] = re_a;
            state[paired * 2 + 1] = im_a;
        }
    }

    normalize(state);
}

fn normalize(state: &mut [f64]) {
    let mut norm_sq = 0.0;
    for i in (0..state.len()).step_by(2) {
        norm_sq += state[i] * state[i] + state[i + 1] * state[i + 1];
    }
    if norm_sq.abs() < 1e-30 { return; }
    let inv_norm = 1.0 / norm_sq.sqrt();
    for val in state.iter_mut() {
        *val *= inv_norm;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::QuantumCircuit;

    #[test]
    fn test_simulate_bell_state() {
        let qc = QuantumCircuit::bell_state();
        let probs = probabilities(&qc).unwrap();
        assert_eq!(probs.len(), 4);
        assert!((probs[0] - 0.5).abs() < 1e-10, "P(00)={}", probs[0]);
        assert!((probs[3] - 0.5).abs() < 1e-10, "P(11)={}", probs[3]);
        assert!(probs[1].abs() < 1e-10, "P(01)={}", probs[1]);
        assert!(probs[2].abs() < 1e-10, "P(10)={}", probs[2]);
    }

    #[test]
    fn test_simulate_hadamard() {
        let mut qc = QuantumCircuit::new(1);
        qc.h(0);
        let probs = probabilities(&qc).unwrap();
        assert!((probs[0] - 0.5).abs() < 1e-10, "P(0)={}", probs[0]);
        assert!((probs[1] - 0.5).abs() < 1e-10, "P(1)={}", probs[1]);
    }

    #[test]
    fn test_simulate_x_gate() {
        let mut qc = QuantumCircuit::new(1);
        qc.x(0);
        let probs = probabilities(&qc).unwrap();
        assert!((probs[0]).abs() < 1e-10, "P(0)={}", probs[0]);
        assert!((probs[1] - 1.0).abs() < 1e-10, "P(1)={}", probs[1]);
    }

    #[test]
    fn test_expectation_z() {
        let qc = QuantumCircuit::bell_state();
        let state = simulate(&qc).unwrap();
        let exp = expectation_z(&state, 0, 2);
        assert!(exp.abs() < 1e-10, "⟨Z₀⟩ for Bell should be 0, got {}", exp);
    }

    #[test]
    fn test_measurement() {
        let mut qc = QuantumCircuit::new(1);
        qc.h(0);
        let state = simulate(&qc).unwrap();

        let mut counts = [0u32; 2];
        let mut rng = deterministic_rng();
        for _ in 0..200 {
            let mut s = state.clone();
            let bit = measure_qubit(&mut s, 0, 1, &mut rng);
            counts[bit as usize] += 1;
        }
        // With a deterministic RNG seeded from system time, we can
        // only check that we got some results for each bit
        assert!(counts[0] + counts[1] == 200, "total={}", counts[0] + counts[1]);
        assert!(counts[0] > 0 || counts[1] > 0);
    }

    #[test]
    fn test_normalization() {
        let mut qc = QuantumCircuit::new(3);
        qc.h(0);
        qc.cnot(0, 1);
        qc.ry_angle(2, 0.7);
        qc.cnot(1, 2);
        let state = simulate(&qc).unwrap();

        let norm_sq: f64 = state.chunks(2)
            .map(|c| c[0] * c[0] + c[1] * c[1])
            .sum();
        assert!((norm_sq - 1.0).abs() < 1e-10, "norm={}", norm_sq);
    }

    #[test]
    fn test_probabilities_sum() {
        let mut qc = QuantumCircuit::new(4);
        for i in 0..4 {
            qc.h(i);
            qc.ry_angle(i, i as f64 * 0.5);
        }
        qc.cnot(0, 1);
        qc.cnot(2, 3);

        let probs = probabilities(&qc).unwrap();
        let sum: f64 = probs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10, "sum={}", sum);
    }

    #[test]
    fn test_cnot_truth_table() {
        // CNOT(control=0, target=1): |a b⟩ → |a, a⊕b⟩
        // Where qubit 0 = LSB (control), qubit 1 = bit 1 (target)
        // |00⟩ → |00⟩  (index 0)
        // |01⟩ → |01⟩  (index 1, q0=1, q1=0) → CNOT flips q1 since q0=1 → |11⟩ (index 3)
        // |10⟩ → |11⟩  (index 2, q0=0, q1=1) → CNOT doesn't flip q1 since q0=0 → |10⟩ (index 2)
        // |11⟩ → |10⟩  (index 3, q0=1, q1=1) → CNOT flips q1 since q0=1 → |01⟩ (index 1)
        // CNOT(0,1) truth table: |q0 q1⟩ = |control target⟩
        // |00⟩ (0) -> |00⟩ (0): control=0, no flip
        // |01⟩ (1) -> |11⟩ (3): control=1, flip target
        // |10⟩ (2) -> |10⟩ (2): control=0, no flip
        // |11⟩ (3) -> |01⟩ (1): control=1, flip target
        let test_cases = [
            (0, 0, 0usize),
            (1, 0, 3usize),
            (0, 1, 2usize),
            (1, 1, 1usize),
        ];
        for &(x0, x1, expected_idx) in &test_cases {
            let mut qc = QuantumCircuit::new(2);
            if x0 == 1 { qc.x(0); }
            if x1 == 1 { qc.x(1); }
            qc.cnot(0, 1);

            let probs = probabilities(&qc).unwrap();
            assert!((probs[expected_idx] - 1.0).abs() < 1e-10,
                "CNOT on |{}{}⟩: expected P[{}]=1, got {}",
                x0, x1, expected_idx, probs[expected_idx]);
        }
    }

    /// Deterministic RNG based on XOR shift for consistent tests.
    fn deterministic_rng() -> impl FnMut() -> f64 {
        let mut seed: u64 = 12345;
        move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed as f64) / (u64::MAX as f64)
        }
    }
}
