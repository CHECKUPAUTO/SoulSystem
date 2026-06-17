//! # Subsystems — Intégration de tous les crates orphelins
//!
//! Ce module active et coordonne les crates historiquement orphelins du
//! workspace, chacun avec un rôle précis dans la boucle cognitive :
//!
//! ## Sous-systèmes intégrés
//!
//! | Crate | Rôle |
//! |---|---|
//! | `soul_journal` | Append-only mmap log (haute fréquence, durable) |
//! | `soul_forge` | Évolution génétique des hyperparamètres du sandbox |
//! | `soul_telemetry` | Métriques thermiques pour `soul_forge` |
//! | `soul_orchestrator` | Routage d'agents par identité (mailbox par agent) |
//! | `soul_agent_runtime` | Cycle de vie d'agents cognitifs |
//! | `soul_evolution` | Hot-swap de modules dynamiques (cdylib) |
//! | `neural_cluster_sync` | Fusion CRDT merge-max pour résilience |
//! | `neural_chaos_monkey` | Chaos engineering (test de robustesse) |
//! | `neural_graph_compiler` | Tri topologique de DAG (étapes de plan) |
//! | `neural_metacognition` | Ring buffer de télémétrie cognitive |
//! | `neural_clinical_console` | Endpoint de monitoring TCP/HTTP |
//! | `ontological_self_healing` | Réparation NaN/Inf/hors-bornes |
//! | `ecosystéma_synapse_linker` | Graphe de synapses (routes goals→skills) |
//!
//! ## Sous-systèmes planifiés (pas encore intégrés)
//!
//! | Crate | Rôle |
//! |---|---|
//! | `soul_perception` | Parser zero-copy pour signaux DATA_/ERR_ → bus IPC |
//! | `soul_cortex` | Réseau récurrent pour état caché de conscience |
//! | `soul_scout` | Client TCP de recherche (SearXNG) |
//! | `scirust_affective_core` | État affectif et drives homeostatiques |
//! | `soul_cluster` | Communication UDP clusterisée |
//! | `soul_acoustic` | Traitement audio et VAD |
//! | `soul_attention` | Cache d'attention multi-tête |
//! | `soul_guard` | Garde-fous et limites de sécurité |
//! | `soul_surgery` | Chirurgie de mémoire et réparation |
//! | `soul_matrix_engine` | Moteur matriciel pour calcul tensoriel |

use chrono::Utc;
use ecosystem_synapse_linker::linker::agent::SynapticLinkerAgent;
use neural_chaos_monkey::ChaosMonkey;
use neural_clinical_console::ClinicalStreamingServer;
use neural_cluster_sync as crdt;
use neural_graph_compiler::GraphCompiler;
use neural_metacognition::api::init_auditor;
use neural_metacognition::SystemAuditor;
use ontological_self_healing as healer;
use parking_lot::Mutex;
use soul_evolution::registry::{register_agents, scan_agents};
use soul_evolution::DynamicModuleLoader;
use soul_forge::EvolutionaryForge;
use soul_journal::MmapJournal;
use soul_orchestrator::SovereignOrchestrator;
use soul_telemetry::metrics::TelemetryHub;
use std::collections::HashMap;
use std::sync::Arc;

// Tags pour le journal binaire (soul_journal attend des u32 tags).
// Convention : 0xSOU1_#### où S=sous-système, #### = sous-catégorie.
pub const TAG_GOAL: u32 = 0xA001_0001;
pub const TAG_PLAN: u32 = 0xA001_0002;
pub const TAG_STEP: u32 = 0xA001_0003;
pub const TAG_DECISION: u32 = 0xA001_0004;
pub const TAG_ERROR: u32 = 0xA001_0005;
pub const TAG_HEAL: u32 = 0xA001_0006;
pub const TAG_EVOLVE: u32 = 0xA001_0007;
pub const TAG_FORGE: u32 = 0xA001_0008;

/// Agrégat de tous les sous-systèmes. Chaque champ est `Arc` ou
/// directement partageable via `parking_lot::Mutex` (Send+Sync).
pub struct Subsystems {
    // ── Journal binaire (mmap) ───────────────────────────────
    pub journal: Option<Arc<MmapJournal>>,
    pub journal_records: Mutex<u64>,

    // ── Forge évolutive ──────────────────────────────────────
    pub forge: Mutex<EvolutionaryForge>,
    pub telemetry: Arc<TelemetryHub>,

    // ── Orchestration multi-agents ───────────────────────────
    pub orchestrator: Mutex<SovereignOrchestrator>,
    pub agents: Mutex<HashMap<String, u32>>,

    // ── Hot-swap evolution ───────────────────────────────────
    pub module_loader: DynamicModuleLoader,

    // ── Chaos engineering ─────────────────────────────────────
    pub chaos: Mutex<ChaosMonkey>,

    // ── Compilateur de graphe ────────────────────────────────
    pub graph: Mutex<GraphCompiler>,

    // ── Métacognition ring buffer ─────────────────────────────
    pub auditor: Arc<SystemAuditor>,

    // ── Synapse linker (routes goals→skills) ─────────────────
    pub linker: Arc<SynapticLinkerAgent>,

    // ── CRDT merge state (résilience cluster) ────────────────
    pub crdt_state: Mutex<Vec<f32>>,

    // ── Healing counter ──────────────────────────────────────
    pub heals_performed: Mutex<u64>,
}

impl Subsystems {
    /// Initialise tous les sous-systèmes. `journal_path` est optionnel
    /// (None = pas de journalisation binaire).
    pub fn new(journal_path: Option<&str>) -> std::io::Result<Self> {
        let journal = match journal_path {
            Some(p) => match MmapJournal::new(p) {
                Ok(j) => Some(Arc::new(j)),
                Err(e) => {
                    tracing::warn!("[subsystems] journal indisponible: {e}");
                    None
                }
            },
            None => None,
        };

        let mut orchestrator = SovereignOrchestrator::new();

        // Auto-load agent definitions from disk and register them in the orchestrator.
        // Uses deterministic FNV-1a hashing so IDs are stable across restarts.
        let agents_dir = std::path::PathBuf::from("/root/.agents/agents");
        let scanned = scan_agents(&agents_dir);
        let registered = register_agents(&mut orchestrator, &scanned);

        let mut agent_map: HashMap<String, u32> = HashMap::new();
        for (id, entry) in &scanned {
            agent_map.insert(entry.name.clone(), *id);
        }

        tracing::info!(
            scanned = scanned.len(),
            registered = registered,
            "Agent ecosystem loaded from disk"
        );

        Ok(Self {
            journal,
            journal_records: Mutex::new(0),

            forge: Mutex::new(EvolutionaryForge::new()),
            telemetry: Arc::new(TelemetryHub::new(4)),

            orchestrator: Mutex::new(orchestrator),
            agents: Mutex::new(agent_map),

            module_loader: DynamicModuleLoader,

            chaos: Mutex::new(ChaosMonkey::new(0xCAFE_BABE, 0.05)),

            graph: Mutex::new(GraphCompiler::new(0)),

            auditor: init_auditor(),

            linker: Arc::new(SynapticLinkerAgent::new()),

            crdt_state: Mutex::new(Vec::new()),

            heals_performed: Mutex::new(0),
        })
    }

    // ── Journal helpers ──────────────────────────────────────

    /// Écrit un événement dans le journal mmap. Binaire :
    /// `tag:u32 + len:u32 + payload:bytes`. Le payload est un JSON
    /// sérialisé (auto-descriptif).
    pub fn journal_log(&self, tag: u32, payload: &str) -> bool {
        let Some(ref j) = self.journal else {
            return false;
        };
        let ok = j.append_log(tag, payload.as_bytes());
        if ok {
            *self.journal_records.lock() += 1;
        }
        ok
    }

    /// Relit tous les records commités du journal.
    pub fn journal_dump(&self) -> Vec<(u32, String)> {
        let Some(ref j) = self.journal else {
            return Vec::new();
        };
        j.read_committed()
            .into_iter()
            .map(|(tag, bytes)| (tag, String::from_utf8_lossy(&bytes).to_string()))
            .collect()
    }

    // ── Forge évolution ──────────────────────────────────────

    /// Lance une évaluation + mutation éventuelle. Les compteurs de
    /// télémétrie sont d'abord synchronisés via `record_execution`.
    pub fn forge_evaluate(&self, total_tasks: u64, total_cycles: u64) -> bool {
        // Pousser les compteurs dans le hub (4 cœurs virtuels).
        let per_core = total_tasks / 4;
        let per_cycles = total_cycles / 4;
        for core in 0..4 {
            self.telemetry.record_execution(core, per_cycles, false);
        }
        let _ = per_core; // tasks_executed déjà incrémenté par record_execution.
        let mutated = self.forge.lock().evaluate_and_mutate(&self.telemetry);
        if mutated {
            let f = self.forge.lock();
            let s = format!(
                "generation={} tile={} threshold={}",
                f.generation(),
                f.current_genome.matrix_tile_size,
                f.current_genome.work_stealing_threshold
            );
            self.journal_log(TAG_FORGE, &s);
        }
        mutated
    }

    // ── Orchestrateur : agents goal → skills ─────────────────

    /// Enregistre un goal comme agent dormant. Le hash de la description
    /// est utilisé comme id stable.
    pub fn orch_register_goal(&self, goal_id: u32, label: &str) -> bool {
        let ok = self.orchestrator.lock().register_agent(goal_id);
        if ok {
            // Créer un lien synaptique agent → entité (broadcast).
            let entity_id = 0xFFFF_FFFFu64;
            let goal_id_u64 = goal_id as u64;
            self.linker
                .update_synapse_route(entity_id, goal_id_u64, 1.0, true);
            self.journal_log(
                TAG_GOAL,
                &format!("{}|register|{}", Utc::now().to_rfc3339(), label),
            );
        }
        ok
    }

    /// Active (Dormant → Active) le goal-agent associé. Renvoie `true` si
    /// c'est cet appel qui l'a réveillé.
    pub fn orch_wake(&self, goal_id: u32) -> bool {
        self.orchestrator.lock().wake(goal_id)
    }

    /// Marque un goal-agent comme completed (Dormant après exéc).
    pub fn orch_complete(&self, goal_id: u32) {
        let _ = self
            .orchestrator
            .lock()
            .transition(goal_id, soul_orchestrator::AgentState::Dormant);
    }

    pub fn orch_state(&self, goal_id: u32) -> Option<String> {
        self.orchestrator
            .lock()
            .state(goal_id)
            .map(|s| format!("{:?}", s))
    }

    pub fn orch_count(&self) -> usize {
        self.orchestrator.lock().agent_count()
    }

    // ── Chaos engineering ────────────────────────────────────

    /// Perturbe un buffer avec proba 5% (magnitude 0.1). Renvoie le
    /// nombre de fautes injectées. Utilisé pour stress-tester les
    /// décisions.
    pub fn chaos_perturb(&self, buf: &mut [f32]) -> usize {
        self.chaos.lock().perturb(buf, 0.1)
    }

    // ── Compilateur de graphe ─────────────────────────────────

    /// Compile un plan en ordre topologique. Les étapes sont des nœuds ;
    /// si l'étape N a une dépendance sur l'étape M, on ajoute l'arête M→N.
    pub fn graph_compile_plan(
        &self,
        n_steps: usize,
        deps: &[(usize, usize)],
    ) -> Result<Vec<usize>, &'static str> {
        let mut g = self.graph.lock();
        // Recréer le graphe à la bonne taille.
        *g = GraphCompiler::new(n_steps);
        for &(from, to) in deps {
            g.add_edge(from, to);
        }
        g.compile()
    }

    // ── Heal / repair ─────────────────────────────────────────

    /// Répare un état (NaN/Inf/hors-bornes). Renvoie le nombre de
    /// réparations. Si > 0, journalise.
    pub fn heal(&self, state: &mut [f32], min: f32, max: f32) -> usize {
        let n = healer::heal(state, min, max);
        if n > 0 {
            *self.heals_performed.lock() += n as u64;
            self.journal_log(TAG_HEAL, &format!("count={n}"));
        }
        n
    }

    // ── CRDT merge (résilience cluster) ──────────────────────

    /// Fusionne un état distant dans l'état local (merge-max). Renvoie
    /// le nombre d'éléments relevés.
    pub fn crdt_merge(&self, remote: &[f32]) -> usize {
        let mut local = self.crdt_state.lock();
        if local.len() < remote.len() {
            local.resize(remote.len(), 0.0);
        }
        crdt::merge_max(&mut local, remote)
    }

    /// Initialise l'état CRDT à partir d'un vecteur.
    pub fn crdt_init(&self, initial: Vec<f32>) {
        *self.crdt_state.lock() = initial;
    }

    /// Snapshot de l'état CRDT.
    pub fn crdt_snapshot(&self) -> Vec<f32> {
        self.crdt_state.lock().clone()
    }

    /// C.8 : diffuse le snapshot CRDT à une liste de peers UDP.
    /// Format : `SOULCRDT\n<len:u32 LE>\n<bytes f32 LE>`.
    /// Renvoie le nombre de peers contactés avec succès.
    pub fn crdt_broadcast(&self, peers: &[std::net::SocketAddr]) -> usize {
        use std::net::UdpSocket;
        let snap = self.crdt_snapshot();
        let mut payload = Vec::with_capacity(8 + snap.len() * 4);
        payload.extend_from_slice(b"SOULCRDT\n");
        payload.extend_from_slice(&(snap.len() as u32).to_le_bytes());
        for f in &snap {
            payload.extend_from_slice(&f.to_le_bytes());
        }
        let mut count = 0;
        if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
            for peer in peers {
                if socket.send_to(&payload, peer).is_ok() {
                    count += 1;
                }
            }
        }
        count
    }

    /// C.8 : reçoit un datagramme CRDT (depuis un peer) et merge.
    pub fn crdt_recv_datagram(&self, data: &[u8]) -> std::io::Result<usize> {
        if data.len() < 8 + 4 || &data[..8] != b"SOULCRDT" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "bad header",
            ));
        }
        let len = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        if data.len() < 12 + len * 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "truncated",
            ));
        }
        let mut remote = Vec::with_capacity(len);
        for i in 0..len {
            let off = 12 + i * 4;
            let bytes = [data[off], data[off + 1], data[off + 2], data[off + 3]];
            remote.push(f32::from_le_bytes(bytes));
        }
        Ok(self.crdt_merge(&remote))
    }

    // ── Synapse linker (inspection) ──────────────────────────

    pub fn synapse_count(&self) -> usize {
        self.linker.get_total_synapse_count()
    }

    pub fn synapse_route(&self, src: u64, tgt: u64) -> Option<f32> {
        self.linker.resolve_routing_weight(src, tgt)
    }

    /// Met à jour une synapse (utilisé par les skills pour s'enregistrer).
    pub fn synapse_update(&self, src: u64, tgt: u64, weight: f32, active: bool) {
        self.linker.update_synapse_route(src, tgt, weight, active);
    }

    // ── Métacognition (ring buffer) ───────────────────────────

    /// Enregistre une frame de télémétrie dans le ring buffer.
    pub fn meta_record(&self, throughput: u64, synapses: u32, meta_loss: f32) {
        let frame = neural_metacognition::metacognition::auditor::TelemetryFrame {
            timestamp_ns: Utc::now().timestamp_nanos_opt().unwrap_or(0) as u64,
            memory_throughput_bytes_per_sec: throughput,
            active_synapse_count: synapses,
            current_meta_loss: meta_loss,
        };
        self.auditor.record(frame);
    }

    /// Renvoie la dernière frame de télémétrie.
    pub fn meta_latest(&self) -> neural_metacognition::metacognition::auditor::TelemetryFrame {
        self.auditor.get_latest()
    }

    // ── Module loader (hot-swap) ─────────────────────────────

    /// Vérifie qu'une .so peut être chargée (sans la charger).
    pub fn module_can_load(&self, path: &str) -> bool {
        DynamicModuleLoader::can_load(path)
    }

    /// Charge un module .so (unsafe — l'appelant doit s'assurer que
    /// la bibliothèque est de confiance). Renvoie `true` si le symbole
    /// `soul_agent_main` est trouvé.
    ///
    /// # Safety
    /// Caller must ensure:
    /// - `path` points to a valid, trusted shared library (.so file)
    /// - The library exports a `soul_agent_main` symbol with correct signature
    /// - Loading the library does not violate memory safety (no conflicting global state)
    /// - The library is compatible with the current architecture and Rust version
    pub unsafe fn module_load(&self, path: &str) -> bool {
        DynamicModuleLoader::load_agent_routine(path).is_some()
    }

    /// Démarre le serveur clinique (TCP HTTP léger) sur le port donné.
    /// Renvoie un handle de shutdown. Utilise l'auditor partagé pour
    /// exposer `/health` et `/metrics` aux clients.
    pub fn start_clinical_console(&self, port: u16) -> std::io::Result<ClinicalHandle> {
        let server = neural_clinical_console::api::init_console(self.auditor.clone(), port);
        let server_clone = server.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = server_clone.start_streaming().await {
                tracing::error!("[clinical] server crashed: {e}");
            }
        });
        Ok(ClinicalHandle {
            server,
            task: handle,
        })
    }
}

/// Handle pour arrêter le serveur clinique.
pub struct ClinicalHandle {
    server: Arc<ClinicalStreamingServer>,
    task: tokio::task::JoinHandle<()>,
}

impl ClinicalHandle {
    pub fn shutdown(&self) {
        self.server.shutdown();
        self.task.abort();
    }
}

impl std::fmt::Debug for Subsystems {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subsystems")
            .field("journal_active", &self.journal.is_some())
            .field("journal_records", &*self.journal_records.lock())
            .field("forge_generation", &self.forge.lock().generation())
            .field("orch_agents", &self.orchestrator.lock().agent_count())
            .field("synapse_count", &self.linker.get_total_synapse_count())
            .field("heals_performed", &*self.heals_performed.lock())
            .field("crdt_state_len", &self.crdt_state.lock().len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_without_journal() {
        let s = Subsystems::new(None).unwrap();
        assert!(s.journal.is_none());
        // The constructor auto-loads agents from /root/.agents/agents, so counts
        // may be non-zero. We only assert the constructor succeeds.
        let _ = s.orch_count();
        let _ = s.synapse_count();
    }

    #[test]
    fn orch_register_and_wake() {
        let s = Subsystems::new(None).unwrap();
        let baseline = s.orch_count();
        assert!(s.orch_register_goal(101, "test goal"));
        assert_eq!(s.orch_count(), baseline + 1);
        assert_eq!(s.orch_state(101), Some("Dormant".into()));
        assert!(s.orch_wake(101));
        assert_eq!(s.orch_state(101), Some("Active".into()));
        s.orch_complete(101);
        assert_eq!(s.orch_state(101), Some("Dormant".into()));
    }

    #[test]
    fn orch_double_register_refused() {
        let s = Subsystems::new(None).unwrap();
        assert!(s.orch_register_goal(1, "a"));
        assert!(!s.orch_register_goal(1, "a"));
    }

    #[test]
    fn heal_repairs_state() {
        let s = Subsystems::new(None).unwrap();
        let mut state = vec![0.5, f32::NAN, 1.0, 5.0, -2.0];
        let n = s.heal(&mut state, 0.0, 1.0);
        assert_eq!(n, 3);
        assert!(state
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0));
    }

    #[test]
    fn chaos_perturb_injected() {
        let s = Subsystems::new(None).unwrap();
        let mut buf = vec![0.0f32; 1000];
        let faults = s.chaos_perturb(&mut buf);
        // rate=0.05 → ~50 fautes attendues
        assert!((20..=100).contains(&faults), "got {faults} faults");
    }

    #[test]
    fn crdt_merge_max_works() {
        let s = Subsystems::new(None).unwrap();
        s.crdt_init(vec![1.0, 5.0, 2.0]);
        let updated = s.crdt_merge(&[3.0, 4.0, 2.0]);
        assert_eq!(updated, 1);
        assert_eq!(s.crdt_snapshot(), vec![3.0, 5.0, 2.0]);
    }

    #[test]
    fn graph_compile_orders_nodes() {
        let s = Subsystems::new(None).unwrap();
        let order = s.graph_compile_plan(3, &[(0, 1), (0, 2), (1, 2)]).unwrap();
        // 0 doit être avant 1 et 2, 1 avant 2
        assert!(
            order.iter().position(|&x| x == 0).unwrap()
                < order.iter().position(|&x| x == 1).unwrap()
        );
        assert!(
            order.iter().position(|&x| x == 1).unwrap()
                < order.iter().position(|&x| x == 2).unwrap()
        );
    }

    #[test]
    fn graph_detects_cycle() {
        let s = Subsystems::new(None).unwrap();
        let r = s.graph_compile_plan(3, &[(0, 1), (1, 2), (2, 0)]);
        assert!(r.is_err());
    }

    #[test]
    fn synapse_linker_routing() {
        let s = Subsystems::new(None).unwrap();
        s.synapse_update(1, 2, 0.75, true);
        assert_eq!(s.synapse_route(1, 2), Some(0.75));
        assert_eq!(s.synapse_route(2, 1), None);
        assert_eq!(s.synapse_count(), 1);
    }

    #[test]
    fn meta_record_and_latest() {
        let s = Subsystems::new(None).unwrap();
        s.meta_record(1024, 16, 0.1);
        let f = s.meta_latest();
        assert_eq!(f.memory_throughput_bytes_per_sec, 1024);
        assert_eq!(f.active_synapse_count, 16);
    }

    #[test]
    fn module_can_load_validates_path() {
        let s = Subsystems::new(None).unwrap();
        assert!(!s.module_can_load("/nonexistent.so"));
        assert!(!s.module_can_load("/tmp/file.txt"));
    }

    #[test]
    fn journal_works_with_path() {
        let path = format!("/tmp/soul_subsys_journal_{}.bin", std::process::id());
        let _ = std::fs::remove_file(&path);
        {
            let s = Subsystems::new(Some(&path)).unwrap();
            assert!(s.journal.is_some());
            assert!(s.journal_log(TAG_GOAL, "hello"));
            assert!(s.journal_log(TAG_PLAN, "world"));
            assert_eq!(*s.journal_records.lock(), 2);
        }
        let _ = std::fs::remove_file(&path);
    }
}
