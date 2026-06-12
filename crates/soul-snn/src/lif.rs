/// Leaky Integrate-and-Fire (LIF) neuron model.
///
/// This model implements the discrete update equation:
/// V_m(t+1) = V_m(t) * (1 - 1/tau) + X(t)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LIFNeuron {
    /// Membrane potential.
    pub v_m: f64,
    /// Firing threshold.
    pub v_thresh: f64,
    /// Time constant for membrane decay.
    pub tau: f64,
    /// Reset potential after a spike.
    pub v_reset: f64,
}

impl LIFNeuron {
    /// Creates a new LIF neuron with specified parameters.
    pub fn new(v_thresh: f64, tau: f64, v_reset: f64) -> Self {
        Self {
            v_m: v_reset,
            v_thresh,
            tau,
            v_reset,
        }
    }

    /// Updates the neuron state with an input current X(t).
    /// Returns true if the neuron fires a spike, false otherwise.
    pub fn update(&mut self, x: f64) -> bool {
        // Equation: V_m(t+1) = V_m(t) * (1 - 1/tau) + X(t)
        self.v_m = self.v_m * (1.0 - 1.0 / self.tau) + x;

        if self.v_m >= self.v_thresh {
            self.v_m = self.v_reset;
            true
        } else {
            false
        }
    }
}
