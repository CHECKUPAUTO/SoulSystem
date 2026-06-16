use soul_ipc::AgentMessage;
use soul_matrix_engine::engine::{MatrixDescriptor, MatrixEngine};
use soul_orchestrator::SovereignOrchestrator;
use soul_scheduler::queue::Task;
use soul_scheduler::scheduler::AgentScheduler;
use soul_storage::index::{SearchResult, VectorStore};
use soul_telemetry::TelemetryHub;
use std::sync::Arc;

const SIGNAL_ENCODING_DIM: usize = 32;
const SEARCH_BUFFER_SIZE: usize = 5;
const MATRIX_DIM: usize = 64;

/// Structure d'exécution d'un Agent Souverain (Alignée à l'octet près)
#[repr(align(64))]
pub struct CognitiveAgent {
    pub agent_id: u32,
    pub target_core: usize,
    pub hub: Arc<TelemetryHub>,
    pub scheduler_ptr: *const AgentScheduler,
    pub matrix_engine_ptr: *const MatrixEngine,
    pub storage_ptr: *const VectorStore,
    pub orchestrator_ptr: *const SovereignOrchestrator,
}

unsafe impl Send for CognitiveAgent {}
unsafe impl Sync for CognitiveAgent {}

/// Point d'entrée brut de la boucle cognitive.
/// Cette fonction respecte la signature `extern "C" fn(*mut u8)` requise par le Scheduler.
///
/// # Safety
///
/// `ctx_ptr` doit être un pointeur non-null vers un `CognitiveAgent` alloué et valide.
/// Les pointeurs internes (`orchestrator_ptr`, `storage_ptr`, `matrix_engine_ptr`,
/// `scheduler_ptr`) doivent être valides et non-null lorsque cette fonction est appelée.
/// Cette fonction est appelée uniquement par le scheduler via `Task.execute` et ne doit
/// jamais être invoquée directement depuis du code Rust.
pub extern "C" fn run_agent_cognitive_step(ctx_ptr: *mut u8) {
    if ctx_ptr.is_null() {
        return;
    }

    let start = std::time::Instant::now();

    // SAFETY: ctx_ptr est validé non-null ci-dessus. Le cast est sûr car ctx_ptr
    // pointe toujours vers un CognitiveAgent créé par l'appelant (soul_entity).
    // Les déréférencements des pointeurs internes (orchestrator, storage, etc.)
    // sont sûrs car l'appelant garantit leur validité avant de soumettre la tâche.
    unsafe {
        let agent = &*(ctx_ptr as *const CognitiveAgent);
        let orchestrator = &*agent.orchestrator_ptr;
        let storage = &*agent.storage_ptr;
        let matrix_engine = &*agent.matrix_engine_ptr;
        let scheduler = &*agent.scheduler_ptr;

        // 1. PHASE D'ÉCOUTE ET INTERCEPTION (IPC)
        if let Some(message) = orchestrator.poll(agent.agent_id) {
            tracing::debug!(
                agent_id = agent.agent_id,
                signal_code = message.signal_code,
                "Agent signal intercepted"
            );

            // 2. PHASE DE CONTEXTUALISATION (Neural Storage Scan)
            let mut query = [0.0f32; 1024];
            let code = message.signal_code;
            #[allow(clippy::needless_range_loop)]
            for dim in 0..SIGNAL_ENCODING_DIM {
                let bit = (code >> dim) & 1;
                query[dim] = if bit == 1 { 1.0 } else { -1.0 };
            }
            let src = message.source_agent_id;
            #[allow(clippy::needless_range_loop)]
            for dim in 0..SIGNAL_ENCODING_DIM {
                let bit = (src >> dim) & 1;
                query[SIGNAL_ENCODING_DIM + dim] = if bit == 1 { 0.5 } else { -0.5 };
            }

            let mut search_buffer = [SearchResult { id: 0, score: 0.0 }; SEARCH_BUFFER_SIZE];
            let found_memories = storage.knn_search(&query, SEARCH_BUFFER_SIZE, &mut search_buffer);

            if found_memories > 0 {
                tracing::trace!(
                    agent_id = agent.agent_id,
                    memories_found = found_memories,
                    "Memory extraction"
                );
            }

            // 3. PHASE DE CALCUL INTENSIF (SIMD GEMM Execution)
            let mut mat_a_data = vec![0.5f32; MATRIX_DIM * MATRIX_DIM];
            let mut mat_b_data = vec![1.2f32; MATRIX_DIM * MATRIX_DIM];
            let mut mat_c_data = vec![0.0f32; MATRIX_DIM * MATRIX_DIM];

            let a_desc = MatrixDescriptor {
                data: mat_a_data.as_mut_ptr(),
                rows: MATRIX_DIM,
                cols: MATRIX_DIM,
            };
            let b_desc = MatrixDescriptor {
                data: mat_b_data.as_mut_ptr(),
                rows: MATRIX_DIM,
                cols: MATRIX_DIM,
            };
            let mut c_desc = MatrixDescriptor {
                data: mat_c_data.as_mut_ptr(),
                rows: MATRIX_DIM,
                cols: MATRIX_DIM,
            };

            matrix_engine.execute_gemm(&a_desc, &b_desc, &mut c_desc);

            tracing::trace!(
                agent_id = agent.agent_id,
                result = mat_c_data[0],
                "Matrix transformation complete"
            );

            // 4. RÉ-INJECTION EN CASCADE
            let next_step_task = Task {
                execute: run_agent_cognitive_step,
                context: ctx_ptr,
            };
            scheduler.submit_to(agent.target_core, next_step_task);

            let _elapsed = start.elapsed().as_secs_f64();
            agent.hub.record_execution(agent.target_core, 1, false);
        } else {
            let loop_back_task = Task {
                execute: run_agent_cognitive_step,
                context: ctx_ptr,
            };
            scheduler.submit_to(agent.target_core, loop_back_task);
        }
    }
}

/// Intake d'un agent : recupere le prochain message de SA mailbox via
/// l'orchestrateur (file MPSC par agent).
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
        let m1 = agent_intake(&o, 1).expect("message pour 1");
        assert_eq!(m1.signal_code, 0xAA);
        assert!(agent_intake(&o, 1).is_none(), "mailbox de 1 videe");
        let m2 = agent_intake(&o, 2).expect("message pour 2");
        assert_eq!(m2.signal_code, 0xBB);
    }
}
