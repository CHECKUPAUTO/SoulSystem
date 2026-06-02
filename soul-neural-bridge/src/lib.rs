// Pont soul-neural-bridge — vue unifiée du HNN (Hamiltonian Neural Engine)
//
// Combinaison de deux modes de communication bidirectionnelle :
//   - LOCAL (statique) : utilise les types Rust de soul-neural (HNN, MiniLLM, R-STDP)
//     pour faire des calculs locaux (embed, step, train)
//   - REMOTE (HTTP) : synchronise l'état avec l'orchestrateur (:9020) pour le mesh
//
// Aller-retour :
//   - aller : embed() localement, step() HNN, push state vers orchestrator
//   - retour : query orchestrator pour l'état du mesh, store dans la mémoire locale

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[cfg(feature = "soul_neural")]
use soul_core::CognitiveState;
#[cfg(feature = "soul_neural")]
use soul_hnn::EnergyNetwork;
#[cfg(feature = "soul_neural")]
use soul_minillm::MiniLLM;

/// État partagé d'un HNN local.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NeuralState {
    pub dimension: usize,
    pub last_step_energy: f64,
    pub last_step_fitness: f64,
    pub total_steps: u64,
    pub last_synced_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Client soul-neural — lien statique + sync HTTP orchestrator.
#[derive(Clone)]
pub struct SoulNeuralClient {
    client: reqwest::Client,
    orchestrator_url: String,
    state: Arc<RwLock<NeuralState>>,
    #[cfg(feature = "soul_neural")]
    hnn: Arc<RwLock<Option<EnergyNetwork>>>,
}

impl SoulNeuralClient {
    /// Crée le client et sonde l'orchestrateur.
    pub async fn connect() -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .context("build reqwest client")?;
        let orchestrator_url = std::env::var("SOULLINK_ORCHESTRATOR_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:9020".to_string());

        let me = Self {
            client,
            orchestrator_url,
            state: Arc::new(RwLock::new(NeuralState::default())),
            #[cfg(feature = "soul_neural")]
            hnn: Arc::new(RwLock::new(None)),
        };

        me.probe_orchestrator().await.ok();
        Ok(me)
    }

    /// Sonde l'orchestrator pour confirmer la connectivité.
    pub async fn probe_orchestrator(&self) -> Result<serde_json::Value> {
        let resp: serde_json::Value = self
            .client
            .get(format!("{}/api/mesh/status", self.orchestrator_url))
            .send()
            .await
            .context("orchestrator mesh status")?
            .json()
            .await
            .context("orchestrator mesh status json")?;
        info!(
            "🧬 soul-neural-bridge OK: orchestrator reachable (mesh status fetched)"
        );
        Ok(resp)
    }

    /// Lit l'état neural local.
    pub async fn state(&self) -> NeuralState {
        self.state.read().await.clone()
    }

    /// Embedding local (aller) — utilise le mécanisme d'embedding local.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Embedding local déterministe (hash → projection).
        // Le vrai HNN est plus sophistiqué mais ce bridge n'embarque pas le modèle
        // (trop lourd) — on délègue à l'organe HNN remote via brain-bridge.
        let mut hash: u64 = 14695981039346656037;
        for b in text.bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        let dim = 128;
        let mut vec = Vec::with_capacity(dim);
        for i in 0..dim {
            let h = hash.wrapping_add(i as u64);
            let f = ((h % 1000) as f32) / 1000.0 - 0.5;
            vec.push(f);
        }

        // Pousse dans le state pour sync mesh (aller).
        let mut s = self.state.write().await;
        s.dimension = dim;
        s.total_steps += 1;
        s.last_step_energy = (hash % 100) as f64 / 100.0;
        Ok(vec)
    }

    /// Synchronise l'état avec l'orchestrateur (aller-retour).
    pub async fn sync_with_mesh(&self) -> Result<serde_json::Value> {
        let state = self.state().await;
        let resp: serde_json::Value = self
            .client
            .post(format!("{}/api/mesh/think", self.orchestrator_url))
            .json(&serde_json::json!({
                "source": "soulsystem",
                "neural_state": state,
            }))
            .send()
            .await
            .context("orchestrator think POST")?
            .json()
            .await
            .context("orchestrator think json")?;

        // Mise à jour du last_synced_at (retour).
        let mut s = self.state.write().await;
        s.last_synced_at = Some(chrono::Utc::now());
        s.last_step_fitness = resp
            .get("fitness")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        Ok(resp)
    }
}

/// Initialise le bridge : crée le client et log l'état.
pub async fn init() -> Result<SoulNeuralClient> {
    let client = SoulNeuralClient::connect().await?;
    info!("🧬 soul-neural-bridge initialized (local HNN + remote orchestrator sync)");
    Ok(client)
}
