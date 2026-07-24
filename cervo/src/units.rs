use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use crate::{
    config::CervoConfig,
    dynamics,
    evolution::{EvolutionTracker, SharedEvolutionTracker},
    generate_mutation,
    memory::Memory,
    pipeline::TransformationRegistry,
    stability,
    swarm::SwarmBus,
    swarm::SwarmMessage,
    Data, Transformation,
};

/// Marge de fitness minimale au-delà de laquelle une unité adopte l'algorithme
/// partagé par un pair (transfert horizontal). Évite les oscillations.
const ADOPTION_MARGIN: f64 = 0.10;
/// Écart de fitness qui déclenche une demande de synchronisation (pull).
const SYNC_MARGIN: f64 = 0.15;

#[derive(Debug, Clone)]
pub struct MutationReport {
    pub accepted: bool,
    pub previous_algorithm: String,
    pub new_algorithm: Option<String>,
    pub sandbox_result: Option<stability::StabilityReport>,
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnitSnapshot {
    pub id: Uuid,
    pub rsi_index: f64,
    pub algorithm_name: String,
    pub memory_active_count: usize,
    pub memory_long_term_count: usize,
    pub saturation: f64,
    pub entropy: f64,
    pub rejected_count: usize,
    pub total_mutations: u64,
    pub evolution_summary: Vec<(String, u64, u64, f64)>,
    /// Nombre de données réellement traitées par l'unité (flux entrant).
    pub processed_count: u64,
    /// Nombre d'algorithmes adoptés depuis un pair via le bus swarm.
    pub adopted_count: u64,
    /// Nombre de pairs dont l'unité a reçu un rapport de santé.
    pub known_peers: usize,
    /// Motif du dernier rejet de mutation (algorithme tenté + raison), s'il y en a eu.
    pub last_rejection: Option<String>,
}

/// Trace d'une mutation rejetée par le bac à sable de stabilité. Les champs sont
/// exposés via [`UnitSnapshot::last_rejection`] (diagnostic réel, plus un rebut).
#[derive(Debug, Clone)]
struct RejectedMutation {
    attempted_algorithm: String,
    reason: String,
}

/// Vue de santé d'un pair, alimentée par ses `HealthReport`/`Heartbeat`.
#[derive(Debug, Clone, Default)]
struct PeerHealth {
    rsi: f64,
    saturation: f64,
    active_memory: usize,
    fitness: f64,
    last_seen: u64,
}

struct ActorState {
    algorithm: Box<dyn Transformation>,
    id: Uuid,
    rsi_index: f64,
    saturation: f64,
    entropy: f64,
    rejected_mutations: Vec<RejectedMutation>,
    config: CervoConfig,
    evolution: SharedEvolutionTracker,
    memory: Memory,
    swarm: SwarmBus,
    /// Registre servant à reconstruire un algorithme partagé par un pair.
    registry: TransformationRegistry,
    /// Santé connue des autres unités du swarm (protocole vivant).
    peer_health: HashMap<Uuid, PeerHealth>,
    processed_count: u64,
    adopted_count: u64,
}

impl ActorState {
    fn new(
        algorithm: Box<dyn Transformation>,
        config: CervoConfig,
        swarm: SwarmBus,
        evolution: SharedEvolutionTracker,
    ) -> Self {
        let memory = Memory::new(&config.memory);
        ActorState {
            id: Uuid::new_v4(),
            algorithm,
            rsi_index: 50.0,
            saturation: 0.0,
            entropy: 0.5,
            rejected_mutations: Vec::new(),
            config,
            evolution,
            memory,
            swarm,
            registry: TransformationRegistry::default(),
            peer_health: HashMap::new(),
            processed_count: 0,
            adopted_count: 0,
        }
    }

    fn algorithm_name(&self) -> &str {
        self.algorithm.name()
    }

    fn current_fitness(&self) -> f64 {
        self.evolution.read().fitness(self.algorithm_name())
    }

    fn last_rejection(&self) -> Option<String> {
        self.rejected_mutations
            .last()
            .map(|r| format!("{} — {}", r.attempted_algorithm, r.reason))
    }

    fn snapshot(&self) -> UnitSnapshot {
        let (active, long_term) = self.memory.snapshot();
        UnitSnapshot {
            id: self.id,
            rsi_index: self.rsi_index,
            algorithm_name: self.algorithm_name().to_string(),
            memory_active_count: active.len(),
            memory_long_term_count: long_term.len(),
            saturation: self.saturation,
            entropy: self.entropy,
            rejected_count: self.rejected_mutations.len(),
            total_mutations: self.evolution.read().total_mutations(),
            evolution_summary: self.evolution.read().summary(),
            processed_count: self.processed_count,
            adopted_count: self.adopted_count,
            known_peers: self.peer_health.len(),
            last_rejection: self.last_rejection(),
        }
    }
}

enum UnitCommand {
    Cycle {
        resp: oneshot::Sender<String>,
    },
    SetRsi {
        value: f64,
        resp: oneshot::Sender<()>,
    },
    Mutate {
        resp: oneshot::Sender<MutationReport>,
    },
    ProcessData {
        data: Vec<Data>,
        resp: oneshot::Sender<Result<Vec<Data>, String>>,
    },
    GetState {
        resp: oneshot::Sender<UnitSnapshot>,
    },
    Shutdown,
}

#[derive(Clone)]
pub struct UnitHandle {
    cmd_tx: mpsc::Sender<UnitCommand>,
}

impl UnitHandle {
    pub fn new(algorithm: Box<dyn Transformation>) -> Self {
        Self::with_config(algorithm, &CervoConfig::default())
    }

    pub fn with_config(algorithm: Box<dyn Transformation>, config: &CervoConfig) -> Self {
        let swarm = SwarmBus::new(64);
        let evolution = Arc::new(parking_lot::RwLock::new(EvolutionTracker::new(
            config.mutation.exploration_rate,
        )));
        Self::with_swarm_evolution(algorithm, swarm, evolution, config)
    }

    pub fn with_swarm(
        algorithm: Box<dyn Transformation>,
        swarm: SwarmBus,
        config: &CervoConfig,
    ) -> Self {
        let evolution = Arc::new(parking_lot::RwLock::new(EvolutionTracker::new(
            config.mutation.exploration_rate,
        )));
        Self::with_swarm_evolution(algorithm, swarm, evolution, config)
    }

    pub fn with_swarm_evolution(
        algorithm: Box<dyn Transformation>,
        swarm: SwarmBus,
        evolution: SharedEvolutionTracker,
        config: &CervoConfig,
    ) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let swarm_rx = swarm.subscribe();
        let state = ActorState::new(algorithm, config.clone(), swarm, evolution);
        tokio::spawn(run_actor(state, cmd_rx, swarm_rx));
        UnitHandle { cmd_tx }
    }

    pub async fn cycle(&self) -> String {
        let (tx, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UnitCommand::Cycle { resp: tx }).await;
        rx.await.unwrap_or_default()
    }

    pub async fn set_rsi(&self, value: f64) {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(UnitCommand::SetRsi {
                value: value.clamp(0.0, 100.0),
                resp: tx,
            })
            .await;
        let _ = rx.await;
    }

    pub async fn mutate(&self) -> MutationReport {
        let (tx, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UnitCommand::Mutate { resp: tx }).await;
        rx.await.unwrap_or_else(|_| MutationReport {
            accepted: false,
            previous_algorithm: "unknown".into(),
            new_algorithm: None,
            sandbox_result: None,
            rejection_reason: Some("actor channel closed".into()),
        })
    }

    pub async fn process_data(&self, data: Vec<Data>) -> Result<Vec<Data>, String> {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(UnitCommand::ProcessData { data, resp: tx })
            .await;
        rx.await.unwrap_or(Err("actor channel closed".into()))
    }

    pub async fn get_state(&self) -> UnitSnapshot {
        let (tx, rx) = oneshot::channel();
        let _ = self.cmd_tx.send(UnitCommand::GetState { resp: tx }).await;
        rx.await.unwrap_or_else(|_| UnitSnapshot {
            id: Uuid::nil(),
            rsi_index: 0.0,
            algorithm_name: "unknown".into(),
            memory_active_count: 0,
            memory_long_term_count: 0,
            saturation: 0.0,
            entropy: 0.0,
            rejected_count: 0,
            total_mutations: 0,
            evolution_summary: Vec::new(),
            processed_count: 0,
            adopted_count: 0,
            known_peers: 0,
            last_rejection: None,
        })
    }

    pub async fn shutdown(&self) {
        let _ = self.cmd_tx.send(UnitCommand::Shutdown).await;
    }
}

async fn run_actor(
    mut state: ActorState,
    mut cmd_rx: mpsc::Receiver<UnitCommand>,
    mut swarm_rx: broadcast::Receiver<SwarmMessage>,
) {
    loop {
        tokio::select! {
            biased;
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(cmd) => {
                        if handle_command(&mut state, cmd).await {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = swarm_rx.recv() => {
                match msg {
                    Ok(msg) => {
                        if handle_swarm_message(&mut state, msg).await {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }
}

async fn handle_command(state: &mut ActorState, cmd: UnitCommand) -> bool {
    match cmd {
        UnitCommand::Cycle { resp } => {
            let phase = dynamics::cycle_step(
                &mut state.rsi_index,
                &mut state.entropy,
                state.saturation,
                &state.config.dynamics,
            );
            state.saturation = (state.saturation * 0.95 + 0.02).min(1.0);
            state.memory.decay();

            // Protocole de santé vivant : l'unité diffuse son état et un
            // battement de cœur ; ses pairs les consomment (peer_health).
            let active_memory = state.memory.snapshot().0.len();
            let fitness = state.current_fitness();
            let _ = state.swarm.publish(SwarmMessage::HealthReport {
                from: state.id,
                rsi: state.rsi_index,
                saturation: state.saturation,
                active_memory,
                fitness,
            });
            let _ = state.swarm.publish(SwarmMessage::Heartbeat {
                from: state.id,
                timestamp: crate::core::data::next_seq(),
            });

            // Pull : si un pair a une meilleure famille, demander sa synchro.
            if let Some((best, best_fit)) = state.evolution.read().best_family() {
                if best != state.algorithm_name() && best_fit > fitness + SYNC_MARGIN {
                    let _ = state.swarm.publish(SwarmMessage::SyncRequest {
                        from: state.id,
                        target_algo: best,
                    });
                }
            }

            let _ = resp.send(phase.to_string());
            false
        }
        UnitCommand::SetRsi { value, resp } => {
            state.rsi_index = value.clamp(0.0, 100.0);
            let _ = resp.send(());
            false
        }
        UnitCommand::Mutate { resp } => {
            let report = handle_mutation(state).await;
            let _ = resp.send(report);
            false
        }
        UnitCommand::ProcessData { data, resp } => {
            let result = process_data(state, data);
            let _ = resp.send(result);
            false
        }
        UnitCommand::GetState { resp } => {
            let _ = resp.send(state.snapshot());
            false
        }
        UnitCommand::Shutdown => true,
    }
}

/// Traite un flux de données via l'algorithme courant : réutilise ce qui est
/// déjà en mémoire (recall), applique la transformation, tamponne la provenance
/// des sorties, les mémorise et les diffuse aux pairs sur le bus swarm.
fn process_data(state: &mut ActorState, input: Vec<Data>) -> Result<Vec<Data>, String> {
    for d in &input {
        // Recall : si la donnée est déjà connue, on la renforce (réutilisation) ;
        // sinon on la stocke. Les écritures mémoire ont donc un lecteur réel.
        if state.memory.recall(&d.payload).is_none() {
            state.memory.store(d.clone());
        }
    }

    let output = state
        .algorithm
        .transform(&input)
        .map_err(|e| e.to_string())?;

    let mut stamped = Vec::with_capacity(output.len());
    for d in output {
        let d = d.with_source(state.id);
        state.memory.store_with_priority(d.clone(), 1.5);
        let _ = state.swarm.publish(SwarmMessage::Data {
            from: state.id,
            payload: d.clone(),
        });
        stamped.push(d);
    }

    state.processed_count += input.len() as u64;
    Ok(stamped)
}

async fn handle_swarm_message(state: &mut ActorState, msg: SwarmMessage) -> bool {
    match msg {
        SwarmMessage::MutationAnnounce {
            from,
            algo_name,
            success: _,
            divergence: _,
            fitness,
        } => {
            if from != state.id {
                state
                    .evolution
                    .write()
                    .record_with_fitness(&algo_name, fitness);
            }
        }
        SwarmMessage::Data { from, payload } => {
            if from != state.id {
                // Ingestion des données produites par un pair.
                state.memory.store(payload);
            }
        }
        SwarmMessage::HealthReport {
            from,
            rsi,
            saturation,
            active_memory,
            fitness,
        } => {
            if from != state.id {
                let entry = state.peer_health.entry(from).or_default();
                entry.rsi = rsi;
                entry.saturation = saturation;
                entry.active_memory = active_memory;
                entry.fitness = fitness;
            }
        }
        SwarmMessage::Heartbeat { from, timestamp } => {
            if from != state.id {
                state.peer_health.entry(from).or_default().last_seen = timestamp;
            }
        }
        SwarmMessage::SyncRequest { from, target_algo } => {
            // Un pair demande le meilleur algo : si on l'exécute, on le partage.
            if from != state.id && state.algorithm_name() == target_algo {
                let _ = state.swarm.publish(SwarmMessage::AlgorithmShare {
                    from: state.id,
                    algo_name: state.algorithm_name().to_string(),
                    algo_config: state.algorithm.config_json(),
                });
            }
        }
        SwarmMessage::AlgorithmShare {
            from,
            algo_name,
            algo_config,
        } => {
            if from != state.id {
                maybe_adopt(state, &algo_name, &algo_config);
            }
        }
        SwarmMessage::Shutdown { from } => {
            // Arrêt coordonné du swarm (émis par le cortex).
            if from != state.id {
                return true;
            }
        }
    }
    false
}

/// Adopte l'algorithme partagé par un pair si sa fitness globale dépasse
/// nettement celle de l'algorithme courant, qu'il est reconstructible via le
/// registre et qu'il se déclare stable. Rollback sûr en cas d'échec de contrôle.
fn maybe_adopt(state: &mut ActorState, algo_name: &str, algo_config: &serde_json::Value) {
    if algo_name == state.algorithm_name() {
        return;
    }
    let (shared_fit, current_fit) = {
        let ev = state.evolution.read();
        (ev.fitness(algo_name), ev.fitness(state.algorithm_name()))
    };
    if shared_fit <= current_fit + ADOPTION_MARGIN {
        return;
    }
    let candidate = match state.registry.create(algo_name, algo_config) {
        Ok(c) => c,
        Err(_) => return,
    };
    if !candidate.is_stable() {
        return;
    }
    // Contrôle de sûreté : la transformation doit s'exécuter sans erreur sur une
    // sonde triviale avant d'être adoptée (sinon on conserve l'actuelle).
    let probe = vec![Data::new(vec![0u8, 1, 2, 3])];
    if candidate.transform(&probe).is_err() {
        return;
    }
    state.algorithm = candidate;
    state.adopted_count += 1;
}

async fn handle_mutation(state: &mut ActorState) -> MutationReport {
    let previous_name = state.algorithm_name().to_string();

    let new_algorithm = generate_mutation(&*state.algorithm, &state.evolution.read());
    let new_name = new_algorithm.name().to_string();
    let test_alg = new_algorithm.clone_box();
    let test_data = stability::create_test_data(
        state.config.mutation.test_data_size,
        state.config.mutation.test_data_count,
    );

    let result = stability::run_sandbox_test(test_alg, test_data, &state.config.sandbox).await;

    // Une mutation n'est adoptée que si le bac à sable réussit ET que le verdict
    // `is_stable` (auto-évaluation de la transformation × divergence) est positif.
    let report = match result {
        Ok(sandbox) if sandbox.is_stable => {
            state.algorithm = new_algorithm;
            state.evolution.write().record(&new_name, true);
            MutationReport {
                accepted: true,
                previous_algorithm: previous_name,
                new_algorithm: Some(new_name.clone()),
                sandbox_result: Some(sandbox),
                rejection_reason: None,
            }
        }
        other => {
            let reason = match other {
                Ok(_) => "transformation déclarée instable".to_string(),
                Err(err) => err.to_string(),
            };
            state.evolution.write().record(&new_name, false);
            state.rejected_mutations.push(RejectedMutation {
                attempted_algorithm: new_name.clone(),
                reason: reason.clone(),
            });
            MutationReport {
                accepted: false,
                previous_algorithm: previous_name,
                new_algorithm: Some(new_name.clone()),
                sandbox_result: None,
                rejection_reason: Some(reason),
            }
        }
    };

    let _ = state.swarm.publish(SwarmMessage::MutationAnnounce {
        from: state.id,
        algo_name: new_name.clone(),
        success: report.accepted,
        divergence: report
            .sandbox_result
            .as_ref()
            .map(|r| r.divergence)
            .unwrap_or(0.0),
        fitness: state.evolution.read().fitness(&new_name),
    });

    // Push : si la mutation est adoptée, partager l'algorithme avec les pairs.
    if report.accepted {
        let _ = state.swarm.publish(SwarmMessage::AlgorithmShare {
            from: state.id,
            algo_name: new_name,
            algo_config: state.algorithm.config_json(),
        });
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AmplifyTransform, IdentityTransform, ReverseTransform};

    #[tokio::test]
    async fn process_data_transforms_and_records() {
        let unit = UnitHandle::new(Box::new(ReverseTransform));
        let input = vec![Data::new(vec![1, 2]), Data::new(vec![3, 4])];
        let out = unit.process_data(input).await.unwrap();
        // reverse inverse l'ordre des éléments
        assert_eq!(out[0].payload, vec![3, 4]);
        assert_eq!(out[1].payload, vec![1, 2]);
        let snap = unit.get_state().await;
        assert_eq!(snap.processed_count, 2);
        // provenance tamponnée
        assert_eq!(out[0].metadata.source_unit, Some(snap.id));
        assert!(out[0].metadata.created_at > 0);
        // les sorties sont mémorisées
        assert!(snap.memory_active_count >= 2);
    }

    #[tokio::test]
    async fn peers_exchange_data_over_swarm() {
        let swarm = SwarmBus::new(64);
        let producer = UnitHandle::with_swarm(
            Box::new(IdentityTransform),
            swarm.clone(),
            &CervoConfig::default(),
        );
        let consumer = UnitHandle::with_swarm(
            Box::new(IdentityTransform),
            swarm.clone(),
            &CervoConfig::default(),
        );
        // laisser les acteurs s'abonner
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let _ = producer
            .process_data(vec![Data::new(vec![9, 9, 9])])
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        let snap = consumer.get_state().await;
        // le consommateur a ingéré la donnée diffusée par le producteur
        assert!(
            snap.memory_active_count >= 1,
            "le pair aurait dû recevoir la donnée"
        );
    }

    #[tokio::test]
    async fn unstable_mutation_target_is_rejected() {
        // Amplify(factor=4) se déclare instable (factor > 3) : run_sandbox_test
        // doit rendre is_stable=false même sans dépassement de taille.
        let cfg = crate::config::SandboxConfig {
            timeout: std::time::Duration::from_millis(100),
            max_output_bytes: 1_000_000,
            max_divergence: 1_000.0,
        };
        let input = stability::create_test_data(2, 2);
        let report =
            stability::run_sandbox_test(Box::new(AmplifyTransform { factor: 4 }), input, &cfg)
                .await
                .unwrap();
        assert!(
            !report.is_stable,
            "un algo auto-déclaré instable ne doit pas être stable"
        );
    }

    #[tokio::test]
    async fn stable_mutation_is_stable() {
        let cfg = crate::config::SandboxConfig::default();
        let input = stability::create_test_data(4, 2);
        let report = stability::run_sandbox_test(Box::new(IdentityTransform), input, &cfg)
            .await
            .unwrap();
        assert!(report.is_stable);
    }

    #[tokio::test]
    async fn set_rsi_overrides_state() {
        let unit = UnitHandle::new(Box::new(IdentityTransform));
        unit.set_rsi(12.0).await;
        let snap = unit.get_state().await;
        assert_eq!(snap.rsi_index, 12.0);
    }
}
