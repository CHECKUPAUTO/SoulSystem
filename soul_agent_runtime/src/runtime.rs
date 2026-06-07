use soul_scheduler::queue::Task;
use soul_scheduler::scheduler::AgentScheduler;
use soul_matrix_engine::engine::{MatrixEngine, MatrixDescriptor};
use soul_storage::index::{VectorStore, SearchResult};
use soul_ipc::bus::InterAgentBus;


/// Structure d'exécution d'un Agent Souverain (Alignée à l'octet près)
#[repr(align(64))]
pub struct CognitiveAgent {
    pub agent_id: u32,
    pub target_core: usize,
    // Pointers vers les infrastructures globales partagées de l'OS
    pub scheduler_ptr: *const AgentScheduler,
    pub matrix_engine_ptr: *const MatrixEngine,
    pub storage_ptr: *const VectorStore,
    pub ipc_bus_ptr: *const InterAgentBus,
}

unsafe impl Send for CognitiveAgent {}
unsafe impl Sync for CognitiveAgent {}

/// Point d'entrée brut de la boucle cognitive.
/// Cette fonction respecte la signature `extern "C" fn(*mut u8)` requise par le Scheduler.
pub extern "C" fn run_agent_cognitive_step(ctx_ptr: *mut u8) {
    if ctx_ptr.is_null() { return; }

    unsafe {
        let agent = &*(ctx_ptr as *const CognitiveAgent);
        let bus = &*agent.ipc_bus_ptr;
        let storage = &*agent.storage_ptr;
        let matrix_engine = &*agent.matrix_engine_ptr;
        let scheduler = &*agent.scheduler_ptr;

        // 1. PHASE D'ÉCOUTE ET INTERCEPTION (IPC)
        // L'agent vérifie s'il a reçu une commande ou un token sur le bus MPMC
        if let Some(message) = bus.try_recv(agent.agent_id) {
            println!("[AGENT-RUN-TIME] Agent #{} : Signal 0x{:X} intercepté.", agent.agent_id, message.signal_code);

            // 2. PHASE DE CONTEXTUALISATION (Neural Storage Scan)
            // On extrait un vecteur fictif représentant le signal pour sonder notre mémoire interne
            let mut fake_query = [0.0f32; 1024];
            fake_query[0] = message.signal_code as f32; // Injection du code signal dans l'espace d'embedding

            let mut search_buffer = [SearchResult { id: 0, score: 0.0 }; 5];
            let found_memories = storage.knn_search(&fake_query, 5, &mut search_buffer);

            if found_memories > 0 {
                println!("[AGENT-RUN-TIME] Agent #{} : {} souvenirs pertinents extraits de la mémoire.", agent.agent_id, found_memories);
            }

            // 3. PHASE DE CALCUL INTENSIF (SIMD GEMM Execution)
            // L'agent compute la transformation linéaire de son état interne (Inférence ultra-rapide)
            let mut mat_a_data = vec![0.5f32; 64 * 64];
            let mut mat_b_data = vec![1.2f32; 64 * 64];
            let mut mat_c_data = vec![0.0f32; 64 * 64];

            let a_desc = MatrixDescriptor { data: mat_a_data.as_mut_ptr(), rows: 64, cols: 64 };
            let b_desc = MatrixDescriptor { data: mat_b_data.as_mut_ptr(), rows: 64, cols: 64 };
            let mut c_desc = MatrixDescriptor { data: mat_c_data.as_mut_ptr(), rows: 64, cols: 64 };

            // Exécute le produit matriciel vectorisé au niveau du processeur (AVX/Neon)
            matrix_engine.execute_gemm(&a_desc, &b_desc, &mut c_desc);
            println!("[AGENT-RUN-TIME] Agent #{} : Transformation matricielle accomplie (Top-Left C: {}).", agent.agent_id, mat_c_data[0]);

            // 4. RÉ-INJECTION EN CASCADE
            // L'agent se ré-enregistre lui-même dans le planificateur pour l'itération machine suivante
            let next_step_task = Task {
                execute: run_agent_cognitive_step,
                context: ctx_ptr,
            };
            scheduler.submit_to(agent.target_core, next_step_task);
        } else {
            // Aucun message en attente : relâchement léger pour ne pas saturer inutilement le cœur
            let loop_back_task = Task {
                execute: run_agent_cognitive_step,
                context: ctx_ptr,
            };
            scheduler.submit_to(agent.target_core, loop_back_task);
        }
    }
}
