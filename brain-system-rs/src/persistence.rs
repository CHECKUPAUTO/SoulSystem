use crate::{BrainState, neuron};
use std::path::PathBuf;

const PERSIST_PATH: &str = "/mnt/nvme/soullink_brain";

fn state_file() -> PathBuf {
    PathBuf::from(PERSIST_PATH).join("brain_state.json")
}

pub async fn load_or_default() -> BrainState {
    let path = state_file();
    if path.exists() {
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(data) => {
                        let neurons: Vec<neuron::Neuron> = data.get("neurons")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default();
                        let synapses: Vec<neuron::Synapse> = data.get("synapses")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default();
                        let total_neurons = data.get("total_neurons").and_then(|v| v.as_u64()).unwrap_or(0);
                        let growth_events = data.get("growth_events").and_then(|v| v.as_u64()).unwrap_or(0);
                        let total_spikes = data.get("total_spikes").and_then(|v| v.as_u64()).unwrap_or(0);
                        tracing::info!("📂 State loaded: {} neurons, {} synapses", neurons.len(), synapses.len());
                        return BrainState { neurons, synapses, total_neurons, growth_events, total_spikes };
                    }
                    Err(e) => tracing::warn!("⚠️ Parse error: {}", e),
                }
            }
            Err(e) => tracing::warn!("⚠️ Read error: {}", e),
        }
    }
    tracing::info!("🧠 Starting fresh brain");
    BrainState { neurons: vec![], synapses: vec![], total_neurons: 0, growth_events: 0, total_spikes: 0 }
}

pub async fn save(state: &BrainState) {
    let path = state_file();
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let data = serde_json::json!({
        "total_neurons": state.total_neurons,
        "growth_events": state.growth_events,
        "total_spikes": state.total_spikes,
        "neurons": state.neurons,
        "synapses": state.synapses,
    });
    match tokio::fs::write(&path, serde_json::to_string_pretty(&data).unwrap()).await {
        Ok(_) => tracing::info!("💾 State saved: {} neurons, {} synapses", state.neurons.len(), state.synapses.len()),
        Err(e) => tracing::warn!("⚠️ Save error: {}", e),
    }
}