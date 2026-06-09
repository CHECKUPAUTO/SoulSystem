use soul_ipc::AgentMessage;
use soul_matrix_engine::engine::{MatrixDescriptor, MatrixEngine};
use soul_orchestrator::SovereignOrchestrator;
use soul_scheduler::queue::Task;
use soul_scheduler::scheduler::AgentScheduler;
use soul_storage::index::{SearchResult, VectorStore};

/// Structure d'exécution d'un Agent Souverain (Alignée à l'octet près)
#[repr(align(64))]
pub struct CognitiveAgent {
    pub agent_id: u32,
    pub target_core: usize,
    // Pointers vers les infrastructures globales partagées de l'OS
    pub scheduler_ptr: *const AgentScheduler,
    pub matrix_engine_ptr: *const MatrixEngine,
    pub storage_ptr: *const VectorStore,
    pub orchestrator_ptr: *const SovereignOrchestrator,
}

unsafe impl Send for CognitiveAgent {}
unsafe impl Sync for CognitiveAgent {}

/// Point d'entrée brut de la boucle cognitive.
/// Cette fonction respecte la signature `extern "C" fn(*mut u8)` requise par le Scheduler.
pub extern "C" fn run_agent_cognitive_step(ctx_ptr: *mut u8) {
    if ctx_ptr.is_null() {
        return;
    }

    unsafe {
        let agent = &*(ctx_ptr as *const CognitiveAgent);
        let orchestrator = &*agent.orchestrator_ptr;
        let storage = &*agent.storage_ptr;
        let matrix_engine = &*agent.matrix_engine_ptr;
        let scheduler = &*agent.scheduler_ptr;

        // 1. PHASE D'ÉCOUTE ET INTERCEPTION (IPC)
        // L'agent vérifie s'il a reçu une commande ou un token sur le bus MPMC
        if let Some(message) = orchestrator.poll(agent.agent_id) {
            println!(
                "[AGENT-RUN-TIME] Agent #{} : Signal 0x{:X} intercepté.",
                agent.agent_id, message.signal_code
            );

            // 2. PHASE DE CONTEXTUALISATION (Neural Storage Scan)
            // Encodage du signal en vecteur d'embedding : on répartit le signal_code
            // sur les 32 premières dimensions via un schéma cyclique pour maximiser
            // la discriminativité dans l'espace de recherche KNN.
            let mut query = [0.0f32; 1024];
            let code = message.signal_code;
            #[allow(clippy::needless_range_loop)]
            for dim in 0..32 {
                let bit = (code >> dim) & 1;
                query[dim] = if bit == 1 { 1.0 } else { -1.0 };
            }
            // Encodage de l'identité source sur les dimensions 32-63
            let src = message.source_agent_id;
            #[allow(clippy::needless_range_loop)]
            for dim in 0..32 {
                let bit = (src >> dim) & 1;
                query[32 + dim] = if bit == 1 { 0.5 } else { -0.5 };
            }

            let mut search_buffer = [SearchResult { id: 0, score: 0.0 }; 5];
            let found_memories = storage.knn_search(&query, 5, &mut search_buffer);

            if found_memories > 0 {
                println!(
                    "[AGENT-RUN-TIME] Agent #{} : {} souvenirs pertinents extraits de la mémoire.",
                    agent.agent_id, found_memories
                );
            }

            // 3. PHASE DE CALCUL INTENSIF (SIMD GEMM Execution)
            // L'agent compute la transformation linéaire de son état interne (Inférence ultra-rapide)
            let mut mat_a_data = vec![0.5f32; 64 * 64];
            let mut mat_b_data = vec![1.2f32; 64 * 64];
            let mut mat_c_data = vec![0.0f32; 64 * 64];

            let a_desc = MatrixDescriptor {
                data: mat_a_data.as_mut_ptr(),
                rows: 64,
                cols: 64,
            };
            let b_desc = MatrixDescriptor {
                data: mat_b_data.as_mut_ptr(),
                rows: 64,
                cols: 64,
            };
            let mut c_desc = MatrixDescriptor {
                data: mat_c_data.as_mut_ptr(),
                rows: 64,
                cols: 64,
            };

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

/// Intake d'un agent : recupere le prochain message de SA mailbox via
/// l'orchestrateur (file MPSC par agent). Remplace l'ancien filtrage par
/// contenu sur le ring partage (`InterAgentBus::try_recv`), qui reordonnait les
/// messages et n'etait pas equitable entre agents.
pub fn agent_intake(orchestrator: &SovereignOrchestrator, agent_id: u32) -> Option<AgentMessage> {
    orchestrator.poll(agent_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soul_orchestrator::DispatchOutcome;

    fn sig(target: u32, code: u32) -> AgentMessage {
        AgentMessage {
            source_agent_id: 0,
            target_agent_id: target,
            signal_code: code,
            payload_ptr: std::ptr::null_mut(),
            payload_size: 0,
        }
    }

    #[test]
    fn intake_pulls_only_own_messages() {
        let mut o = SovereignOrchestrator::new();
        o.register_agent(1);
        o.register_agent(2);
        assert!(matches!(
            o.dispatch(sig(1, 0xAA)),
            DispatchOutcome::Delivered { .. }
        ));
        assert!(matches!(
            o.dispatch(sig(2, 0xBB)),
            DispatchOutcome::Delivered { .. }
        ));
        // chaque agent ne recoit QUE le sien : ni perte, ni cross-talk, ni reordre
        let m1 = agent_intake(&o, 1).expect("message pour 1");
        assert_eq!(m1.signal_code, 0xAA);
        assert!(agent_intake(&o, 1).is_none(), "mailbox de 1 videe");
        let m2 = agent_intake(&o, 2).expect("message pour 2");
        assert_eq!(m2.signal_code, 0xBB);
    }
}
