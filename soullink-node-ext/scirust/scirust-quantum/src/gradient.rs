//! Parameter-shift rule for quantum circuit gradients.
//!
//! Since quantum gates are unitary exponentials of Pauli operators,
//! their gradients can be computed without backpropagation through
//! the circuit using the parameter-shift rule:
//!
//! ```text
//! ∂⟨O⟩/∂θ = (⟨O⟩(θ + π/2) - ⟨O⟩(θ - π/2)) / 2
//! ```
//!
//! This works for any gate of the form `e^{-iθP/2}` where `P` is
//! a Pauli operator (RX, RY, RZ).

use crate::circuit::QuantumCircuit;
use crate::simulator::{expectation_z, simulate};

/// Compute the gradient of `<Z₀>` with respect to the `param_idx`-th parameter
/// using the parameter-shift rule.
///
/// # Arguments
/// * `circuit` — The parametrized circuit (must have the parameter set)
/// * `param_idx` — Index of the parameter to differentiate
/// * `original_params` — The parameter values at which to evaluate the gradient
///
/// # Returns
/// The gradient `∂⟨Z₀⟩/∂θ` at the given parameter point.
pub fn parameter_shift(
    circuit: &QuantumCircuit,
    param_idx: usize,
    original_params: &[f64],
) -> Result<f64, String> {
    if param_idx >= original_params.len() {
        return Err(format!("Parameter index {} out of range ({})", param_idx, original_params.len()));
    }

    let shift = std::f64::consts::FRAC_PI_2; // π/2

    // Forward shift: θ + π/2
    let mut qc_fwd = circuit.clone();
    qc_fwd.set_param(param_idx, original_params[param_idx] + shift);
    let state_fwd = simulate(&qc_fwd)?;
    let fwd = expectation_z(&state_fwd, 0, circuit.num_qubits);

    // Backward shift: θ - π/2
    let mut qc_bwd = circuit.clone();
    qc_bwd.set_param(param_idx, original_params[param_idx] - shift);
    let state_bwd = simulate(&qc_bwd)?;
    let bwd = expectation_z(&state_bwd, 0, circuit.num_qubits);

    // Parameter-shift rule
    let grad = (fwd - bwd) / 2.0;

    Ok(grad)
}

/// Compute the full gradient vector of `<Z₀>` w.r.t. all parameters.
pub fn full_gradient(
    circuit: &QuantumCircuit,
    params: &[f64],
) -> Result<Vec<f64>, String> {
    let n = params.len();
    let mut grads = Vec::with_capacity(n);
    for i in 0..n {
        let g = parameter_shift(circuit, i, params)?;
        grads.push(g);
    }
    Ok(grads)
}

/// Sample-based parameter-shift gradient (with measurement shots).
///
/// Uses `num_shots` measurements to estimate the expectation value,
/// which introduces statistical noise but simulates real quantum hardware.
pub fn parameter_shift_with_shots(
    circuit: &QuantumCircuit,
    param_idx: usize,
    original_params: &[f64],
    num_shots: usize,
    rng: &mut impl FnMut() -> f64,
) -> Result<f64, String> {
    if param_idx >= original_params.len() {
        return Err(format!("Parameter index {} out of range", param_idx));
    }

    let shift = std::f64::consts::FRAC_PI_2;

    let mut estimate_expectation = |circ: &QuantumCircuit| -> Result<f64, String> {
        let mut sum = 0.0;
        for _ in 0..num_shots {
            let state = simulate(circ)?;
            let mut s = state.clone();
            let bits = crate::simulator::measure_all(&mut s, circ.num_qubits, rng);
            let z0 = if bits[0] == 0 { 1.0 } else { -1.0 };
            sum += z0;
        }
        Ok(sum / num_shots as f64)
    };

    let mut qc_fwd = circuit.clone();
    qc_fwd.set_param(param_idx, original_params[param_idx] + shift);
    let fwd = estimate_expectation(&qc_fwd)?;

    let mut qc_bwd = circuit.clone();
    qc_bwd.set_param(param_idx, original_params[param_idx] - shift);
    let bwd = estimate_expectation(&qc_bwd)?;

    Ok((fwd - bwd) / 2.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::QuantumCircuit;

    #[test]
    fn test_parameter_shift_single_qubit() {
        // Circuit: RY(θ)|0⟩ → ⟨Z₀⟩ = cos(θ)
        let mut qc = QuantumCircuit::new_variational(1, &["theta"]);
        qc.ry(0, 0);

        let params = vec![0.5];
        let grad = parameter_shift(&qc, 0, &params).unwrap();

        // d⟨Z⟩/dθ = -sin(θ) = -sin(0.5) ≈ -0.479
        let expected = -(0.5f64).sin();
        assert!((grad - expected).abs() < 1e-6,
            "grad={}, expected={}", grad, expected);
    }

    #[test]
    fn test_parameter_shift_zero_gradient() {
        // At θ=0, ⟨Z₀⟩ = cos(0) = 1, derivative = -sin(0) = 0
        let mut qc = QuantumCircuit::new_variational(1, &["theta"]);
        qc.ry(0, 0);

        let params = vec![0.0];
        let grad = parameter_shift(&qc, 0, &params).unwrap();

        assert!(grad.abs() < 1e-10, "grad should be 0 at θ=0, got {}", grad);
    }

    #[test]
    fn test_full_gradient() {
        let mut qc = QuantumCircuit::new_variational(2, &["t0", "t1", "t2"]);
        qc.ry(0, 0);
        qc.ry(1, 1);
        qc.cnot(0, 1);
        qc.rz(0, 2);

        let params = vec![0.2, 0.8, 0.5];
        let grads = full_gradient(&qc, &params).unwrap();
        assert_eq!(grads.len(), 3);
        for g in &grads {
            assert!(g.is_finite(), "Found non-finite gradient");
        }
    }

    #[test]
    fn test_gradient_param_out_of_range() {
        let mut qc = QuantumCircuit::new_variational(1, &["theta"]);
        qc.rx(0, 0);
        let params = vec![0.5];
        let result = parameter_shift(&qc, 5, &params);
        assert!(result.is_err());
    }
}
