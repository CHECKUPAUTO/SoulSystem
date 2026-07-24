use std::collections::HashMap;
use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    config::CervoConfig,
    evolution::{EvolutionTracker, SharedEvolutionTracker},
    swarm::{SwarmBus, SwarmMessage},
    Data, Transformation, UnitHandle, UnitSnapshot,
};

/// Vue de santé compacte d'une unité (id, rsi, saturation, mémoire active).
pub type UnitHealth = (Uuid, f64, f64, usize);

#[derive(Debug, Clone)]
pub struct CortexReport {
    pub unit_count: usize,
    pub total_mutations: u64,
    pub evolution_summary: Vec<(String, u64, u64, f64)>,
    pub units: Vec<UnitSnapshot>,
    /// Santé agrégée par unité, alimentée par les instantanés vivants.
    pub health: Vec<UnitHealth>,
    /// Total de données traitées par l'ensemble des unités.
    pub total_processed: u64,
    /// Total d'algorithmes adoptés entre pairs (transfert horizontal).
    pub total_adopted: u64,
}

pub struct Cortex {
    units: HashMap<Uuid, UnitHandle>,
    swarm: SwarmBus,
    pub evolution: SharedEvolutionTracker,
    config: CervoConfig,
}

impl Cortex {
    /// Construit un cortex après **validation** de la configuration. Rend une
    /// erreur explicite si la config est incohérente (au lieu de démarrer un
    /// système mal réglé).
    pub fn try_new(config: CervoConfig) -> Result<Self, String> {
        config.validate()?;
        let evolution = Arc::new(parking_lot::RwLock::new(EvolutionTracker::new(
            config.mutation.exploration_rate,
        )));
        Ok(Cortex {
            units: HashMap::new(),
            swarm: SwarmBus::new(256),
            evolution,
            config,
        })
    }

    /// Variante infaillible : panique si la configuration est invalide.
    pub fn new(config: CervoConfig) -> Self {
        Self::try_new(config).expect("configuration CERVO invalide")
    }

    #[instrument(skip(self, algo))]
    pub async fn spawn(&mut self, algo: Box<dyn Transformation>) -> Uuid {
        let handle = UnitHandle::with_swarm_evolution(
            algo,
            self.swarm.clone(),
            self.evolution.clone(),
            &self.config,
        );
        let state = handle.get_state().await;
        let id = state.id;
        self.units.insert(id, handle);
        id
    }

    pub async fn kill(&mut self, id: Uuid) -> bool {
        if let Some(handle) = self.units.remove(&id) {
            handle.shutdown().await;
            true
        } else {
            false
        }
    }

    pub async fn kill_all(&mut self) {
        for (_, handle) in self.units.drain() {
            handle.shutdown().await;
        }
    }

    pub fn get(&self, id: &Uuid) -> Option<&UnitHandle> {
        self.units.get(id)
    }

    pub fn unit_count(&self) -> usize {
        self.units.len()
    }

    pub fn swarm(&self) -> &SwarmBus {
        &self.swarm
    }

    pub fn config(&self) -> &CervoConfig {
        &self.config
    }

    #[instrument(skip(self))]
    pub async fn tick_all(&self) {
        for handle in self.units.values() {
            handle.cycle().await;
        }
    }

    /// Diffuse un flux de données à **toutes** les unités et collecte leurs
    /// résultats. C'est le chemin par lequel l'écosystème exécute réellement ses
    /// transformations sur des données (au-delà de la simple mutation).
    #[instrument(skip(self, data))]
    pub async fn process_all(&self, data: Vec<Data>) -> Vec<(Uuid, Result<Vec<Data>, String>)> {
        let mut out = Vec::with_capacity(self.units.len());
        for (id, handle) in &self.units {
            let r = handle.process_data(data.clone()).await;
            out.push((*id, r));
        }
        out
    }

    /// Arrêt coordonné du swarm : diffuse un `Shutdown` sur le bus (chaque unité
    /// se termine) puis vide le registre. Utilise le bus swarm plutôt qu'un
    /// arrêt unité par unité.
    pub async fn broadcast_shutdown(&mut self) {
        let _ = self.swarm.publish(SwarmMessage::Shutdown {
            from: Uuid::new_v4(),
        });
        // Laisser les acteurs traiter le message avant de lâcher les handles.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        self.units.clear();
    }

    pub async fn report(&self) -> CortexReport {
        let mut units = Vec::new();
        let mut total_muts = 0u64;
        let mut total_processed = 0u64;
        let mut total_adopted = 0u64;
        let mut health = Vec::new();
        let mut agg: HashMap<String, (u64, u64)> = HashMap::new();

        for handle in self.units.values() {
            let snap = handle.get_state().await;
            total_muts += snap.total_mutations;
            total_processed += snap.processed_count;
            total_adopted += snap.adopted_count;
            health.push((
                snap.id,
                snap.rsi_index,
                snap.saturation,
                snap.memory_active_count,
            ));
            for (family, attempts, successes, _) in &snap.evolution_summary {
                let entry = agg.entry(family.clone()).or_insert((0, 0));
                entry.0 += attempts;
                entry.1 += successes;
            }
            units.push(snap);
        }

        let mut evolution_summary: Vec<_> = agg
            .into_iter()
            .map(|(family, (attempts, successes))| {
                let fitness = if attempts > 0 {
                    successes as f64 / attempts as f64
                } else {
                    0.0
                };
                (family, attempts, successes, fitness)
            })
            .collect();
        evolution_summary
            .sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

        CortexReport {
            unit_count: self.units.len(),
            total_mutations: total_muts,
            evolution_summary,
            units,
            health,
            total_processed,
            total_adopted,
        }
    }
}
