use crate::lif::LIFNeuron;
use rand::Rng;

/// Rate Coding (Codage en Fréquence).
/// Translates intensity of activation into spike density over a fixed time window T.
pub struct RateEncoder {
    pub window_size: usize,
}

impl RateEncoder {
    pub fn new(window_size: usize) -> Self {
        Self { window_size }
    }

    pub fn encode(&self, activation: f64) -> Vec<bool> {
        let mut rng = rand::thread_rng();
        (0..self.window_size)
            .map(|_| rng.gen_bool(activation.clamp(0.0, 1.0)))
            .collect()
    }
}

/// Time-to-First-Spike Coding (Codage par Latence).
/// High activation triggers an immediate spike at the start of the window.
pub struct LatencyEncoder {
    pub window_size: usize,
}

impl LatencyEncoder {
    pub fn new(window_size: usize) -> Self {
        Self { window_size }
    }

    pub fn encode(&self, activation: f64) -> Vec<bool> {
        let mut spikes = vec![false; self.window_size];
        if activation <= 0.0 {
            return spikes;
        }

        // Latency is inversely proportional to activation.
        // If activation is 1.0, latency is 0. If activation is near 0, latency is near window_size.
        let latency =
            ((1.0 - activation.clamp(0.0, 1.0)) * (self.window_size as f64 - 1.0)) as usize;
        if latency < self.window_size {
            spikes[latency] = true;
        }
        spikes
    }
}

/// Bridge Layer to convert continuous ANN activations into SNN spikes.
pub struct BridgeLayer {
    neurons: Vec<LIFNeuron>,
}

impl BridgeLayer {
    pub fn new(size: usize, v_thresh: f64, tau: f64, v_reset: f64) -> Self {
        let neurons = (0..size)
            .map(|_| LIFNeuron::new(v_thresh, tau, v_reset))
            .collect();
        Self { neurons }
    }

    /// Continuous to Spikes via LIF dynamics.
    pub fn process(&mut self, activations: &[f64]) -> Vec<bool> {
        self.neurons
            .iter_mut()
            .zip(activations.iter())
            .map(|(neuron, &act)| neuron.update(act))
            .collect()
    }
}
