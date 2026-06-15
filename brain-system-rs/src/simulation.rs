use crate::{neuron, persistence, SharedBrain};
use rand::Rng;
use std::time::Duration;

/// Boucle de sauvegarde automatique toutes les 30s
pub async fn auto_save_loop(brain: SharedBrain) {
    let interval = Duration::from_secs(30);
    loop {
        tokio::time::sleep(interval).await;
        let state = brain.read().await;
        persistence::save(&state).await;
    }
}

/// Boucle de simulation LIF — tick toutes les DT ms
pub async fn simulation_loop(brain: SharedBrain) {
    let tick_ms = (neuron::DT * 1000.0) as u64;
    let interval = Duration::from_millis(tick_ms.max(1));
    loop {
        tokio::time::sleep(interval).await;
        let mut state = brain.write().await;
        step(&mut state);
    }
}

fn step(state: &mut crate::BrainState) {
    let n = state.neurons.len();
    if n == 0 {
        return;
    }

    // Propager les spikes le long des synapses
    let mut inputs = vec![0.0f64; n];
    for syn in &state.synapses {
        let src = syn.source as usize;
        let tgt = syn.target as usize;
        if src < n && tgt < n {
            // Si le neurone source a déchargé récemment (v serait VZ)
            if (state.neurons[src].v - neuron::VZ).abs() < 0.1 {
                inputs[tgt] += syn.weight;
            }
        }
    }

    // Mettre à jour chaque neurone
    let mut spikes = 0u64;
    for (i, input) in inputs.iter().enumerate() {
        if i < n && state.neurons[i].update(*input) {
            spikes += 1;
        }
    }
    state.total_spikes += spikes;
}

/// Croissance Hebbian — ajoute de nouveaux neurones
/// (pas encore déclenchée par la boucle de simulation)
#[allow(dead_code)]
pub fn grow(state: &mut crate::BrainState, count: usize) -> Vec<u64> {
    let modules = neuron::default_modules();
    let mut rng = rand::thread_rng();
    let mod_idx = rng.gen_range(0..modules.len());
    let m = &modules[mod_idx];

    let mut new_ids = Vec::new();
    for _ in 0..count.min(10) {
        let id = state.total_neurons;
        state.total_neurons += 1;
        let (x, y, z) = neuron::random_pos(m.pos, 60.0);
        state
            .neurons
            .push(neuron::Neuron::new(id, &m.name, x, y, z));
        new_ids.push(id);
    }
    state.growth_events += 1;

    // Créer des connexions Hebbian
    let n = state.neurons.len();
    if n > 1 {
        for &src in &new_ids {
            for _ in 0..3 {
                let target = rng.gen_range(0..n) as u64;
                if target != src {
                    let weight = rng.gen::<f64>() * 0.5;
                    state
                        .synapses
                        .push(neuron::Synapse::new(src, target, weight));
                }
            }
        }
    }
    new_ids
}
