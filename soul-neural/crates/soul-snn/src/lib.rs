pub mod bridge;
pub mod lif;

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// A recurrent spiking neural network with spontaneous activity.
pub struct RecurrentSNN {
    neurons: Vec<f64>,
    weights: Vec<Vec<f64>>,
    eligibility: Vec<Vec<f64>>,
    activity_threshold: f64,
    spontaneous_decay: f64,
    external_input: Vec<f64>,
    spike_buffer: Vec<Vec<bool>>,
}

impl RecurrentSNN {
    pub fn new(num_neurons: usize) -> Self {
        let neurons = vec![0.0; num_neurons];
        let weights = vec![vec![0.0; num_neurons]; num_neurons];
        let eligibility = vec![vec![0.0; num_neurons]; num_neurons];
        RecurrentSNN {
            neurons,
            weights,
            eligibility,
            activity_threshold: 0.7,
            spontaneous_decay: 0.95,
            external_input: vec![0.0; num_neurons],
            spike_buffer: Vec::new(),
        }
    }

    pub fn inject(&mut self, stimulus: &[f64]) {
        for (i, &val) in stimulus.iter().enumerate() {
            if i < self.external_input.len() {
                self.external_input[i] += val;
            }
        }
    }

    pub fn step(&mut self, dt: f64, reward: f64) -> bool {
        for (neuron, &inp) in self.neurons.iter_mut().zip(&self.external_input) {
            *neuron += inp * dt;
        }
        self.external_input.fill(0.0);

        let mut spikes = vec![false; self.neurons.len()];
        for (i, neuron) in self.neurons.iter_mut().enumerate() {
            if *neuron >= 1.0 {
                spikes[i] = true;
                *neuron = 0.0;
            }
        }

        for i in 0..self.neurons.len() {
            if spikes[i] {
                for j in 0..self.neurons.len() {
                    if i != j {
                        self.neurons[j] += self.weights[i][j];
                    }
                }
            }
        }

        for i in 0..self.neurons.len() {
            for j in 0..self.neurons.len() {
                let pre = spikes[i] as u8;
                let post = spikes[j] as u8;
                self.eligibility[i][j] =
                    self.eligibility[i][j] * 0.9 + (pre as f64 * post as f64) * 0.1;
                self.weights[i][j] += 0.001 * self.eligibility[i][j] * reward;
            }
        }

        for neuron in &mut self.neurons {
            *neuron *= self.spontaneous_decay;
            *neuron += rand::random::<f64>() * 0.001;
        }

        self.spike_buffer.push(spikes.clone());
        if self.spike_buffer.len() > 100 {
            self.spike_buffer.remove(0);
        }

        let active_ratio = spikes.iter().filter(|&&s| s).count() as f64 / self.neurons.len() as f64;
        active_ratio > self.activity_threshold
    }

    pub fn current_spike_pattern(&self) -> Vec<f64> {
        self.spike_buffer
            .last()
            .map(|v| v.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect())
            .unwrap_or_else(|| vec![0.0; self.neurons.len()])
    }

    pub async fn run_loop<F>(self, on_thought: F, dt_ms: u64)
    where
        F: Fn(Vec<f64>) + Send + 'static,
    {
        let snn = Arc::new(Mutex::new(self));
        let snn_clone = snn.clone();
        tokio::spawn(async move {
            loop {
                let thought = {
                    let mut snn = snn_clone.lock().unwrap();
                    snn.step(0.01, 0.0)
                };
                if thought {
                    let pattern = snn_clone.lock().unwrap().current_spike_pattern();
                    on_thought(pattern);
                }
                tokio::time::sleep(Duration::from_millis(dt_ms)).await;
            }
        });
    }
}

/// Asynchronous spike event.
#[derive(Debug, Clone)]
pub struct SpikeEvent {
    pub neuron_id: usize,
    pub timestamp: u64,
}

/// Orchestrates asynchronous spike propagation.
pub struct AsyncSpikeOrchestrator {
    sender: mpsc::Sender<SpikeEvent>,
    receiver: mpsc::Receiver<SpikeEvent>,
}

impl AsyncSpikeOrchestrator {
    pub fn new(buffer_size: usize) -> Self {
        let (tx, rx) = mpsc::channel(buffer_size);
        Self {
            sender: tx,
            receiver: rx,
        }
    }

    pub fn get_sender(&self) -> mpsc::Sender<SpikeEvent> {
        self.sender.clone()
    }

    pub async fn run(mut self) {
        while let Some(event) = self.receiver.recv().await {
            // In a real implementation, this would trigger downstream neurons
            // or update synaptic weights.
            println!(
                "Async Spike: Neuron {} at {}",
                event.neuron_id, event.timestamp
            );
        }
    }
}

/// Decodes spike patterns into named intents via learned linear projection.
pub struct SNNDecoder {
    weights: Vec<Vec<f64>>,
    intent_vocab: Vec<String>,
    learning_rate: f64,
}

impl SNNDecoder {
    pub fn new(num_neurons: usize, intents: Vec<String>) -> Self {
        let num_intents = intents.len();
        let weights = vec![vec![0.0; num_intents]; num_neurons];
        Self {
            weights,
            intent_vocab: intents,
            learning_rate: 0.01,
        }
    }

    pub fn decode(&self, spike_pattern: &[f64]) -> (String, f64) {
        let mut scores = vec![0.0; self.intent_vocab.len()];
        for (i, &spike) in spike_pattern.iter().enumerate() {
            if i < self.weights.len() {
                for (j, score) in scores.iter_mut().enumerate() {
                    *score += spike * self.weights[i][j];
                }
            }
        }
        let probs = softmax(&scores);
        let idx = probs
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        (self.intent_vocab[idx].clone(), probs[idx])
    }

    pub fn update(&mut self, spike_pattern: &[f64], chosen_intent: &str, reward: f64) {
        if let Some(idx) = self.intent_vocab.iter().position(|i| i == chosen_intent) {
            for (i, &spike) in spike_pattern.iter().enumerate() {
                if i < self.weights.len() {
                    self.weights[i][idx] += spike * reward * self.learning_rate;
                }
            }
        }
    }
}

fn softmax(logits: &[f64]) -> Vec<f64> {
    let max = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exps: Vec<f64> = logits.iter().map(|x| (x - max).exp()).collect();
    let sum: f64 = exps.iter().sum();
    if sum == 0.0 {
        return vec![0.0; logits.len()];
    }
    exps.iter().map(|e| e / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{LatencyEncoder, RateEncoder};
    use crate::lif::LIFNeuron;

    #[test]
    fn test_snn_creation() {
        let snn = RecurrentSNN::new(100);
        assert!(!snn.current_spike_pattern().is_empty());
    }

    #[test]
    fn test_snn_step() {
        let mut snn = RecurrentSNN::new(10);
        snn.inject(&[1.0; 10]);
        let _thought = snn.step(0.01, 0.0);
        // Should not panic
        println!(
            "Spike activity: {}",
            snn.current_spike_pattern().iter().sum::<f64>()
        );
    }

    #[test]
    fn test_snn_decoder() {
        let decoder = SNNDecoder::new(10, vec!["intent1".into(), "intent2".into()]);
        let (intent, conf) = decoder.decode(&[1.0; 10]);
        assert!(!intent.is_empty());
        assert!(conf > 0.0);
    }

    #[test]
    fn test_lif_neuron() {
        let mut neuron = LIFNeuron::new(1.0, 10.0, 0.0);
        // V_m(t+1) = 0.0 * 0.9 + 0.5 = 0.5
        assert!(!neuron.update(0.5));
        assert_eq!(neuron.v_m, 0.5);
        // V_m(t+2) = 0.5 * 0.9 + 0.6 = 0.45 + 0.6 = 1.05 -> Spike!
        assert!(neuron.update(0.6));
        assert_eq!(neuron.v_m, 0.0);
    }

    #[test]
    fn test_rate_encoder() {
        let encoder = RateEncoder::new(100);
        let spikes = encoder.encode(0.5);
        let count = spikes.iter().filter(|&&s| s).count();
        // Probability check (rough)
        assert!(count > 30 && count < 70);
    }

    #[test]
    fn test_latency_encoder() {
        let encoder = LatencyEncoder::new(10);
        let spikes = encoder.encode(1.0); // Should spike at index 0
        assert!(spikes[0]);

        let spikes = encoder.encode(0.1); // Should spike later
        let pos = spikes.iter().position(|&s| s).unwrap();
        assert!(pos > 0);
    }
}
