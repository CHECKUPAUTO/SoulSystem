use soul_scheduler::scheduler::AgentScheduler;
use soul_matrix_engine::engine::MatrixEngine;
use soul_cortex::RecurrentCortex;
use soul_scout::SovereignScout;
use soul_guard::SystemGuard;
use soul_surgery::NeuralSurgeon;
use soul_cluster::ClusterNode;
use soul_perception::PerceptionPipeline;
use soul_ipc::bus::{AgentMessage, InterAgentBus};

fn main() {
    println!("====================================================");
    println!("   SOULSYSTEM KERNEL - INTEGRATION TRINITE SUPERIEURE");
    println!("====================================================");

    // 1. Composants physiques de base
    let scheduler = AgentScheduler::new();
    let matrix_engine = MatrixEngine::new(&scheduler.manifest);

    // 2. Briques superieures
    let mut cortex = RecurrentCortex::new();
    let _scout = SovereignScout::new("127.0.0.1", 8080); // Cible SearXNG local
    let guard = SystemGuard::new();

    // 3. Cycle synaptique
    println!("[CORTEX] Initialisation de l'etat recurrent...");
    let mut sensory_input = vec![0.35f32; 64 * 64];
    unsafe {
        cortex.process_cognitive_cycle(&matrix_engine, sensory_input.as_mut_ptr());
    }
    println!("[CORTEX] Cycle 1 accompli. Activation residuelle h[0] : {:.4}", cortex.hidden_state[0]);

    // 3bis. Chirurgie RepE : injection d'un concept dans l'activation recurrente REELLE.
    let mut surgeon = NeuralSurgeon::new(0.25);
    let mut concept = [0.0f32; 1024];
    for i in 0..1024 {
        concept[i] = if i % 2 == 0 { 1.0 } else { -1.0 };
    }
    surgeon.set_steering_target(&concept);
    let h0_before = cortex.hidden_state[0];
    surgeon.steer_activations(&mut cortex.hidden_state);
    let h0_after = cortex.hidden_state[0];
    println!(
        "[SURGERY] steering RepE sur hidden_state ({} dims): h[0] {:.4} -> {:.4} (delta attendu {:.4})",
        cortex.hidden_state.len(),
        h0_before,
        h0_after,
        0.25 * concept[0]
    );
    // L'etat steere influence le cycle cognitif suivant.
    unsafe {
        cortex.process_cognitive_cycle(&matrix_engine, sensory_input.as_mut_ptr());
    }
    println!("[CORTEX] Cycle 2 (post-steering) accompli. h[0] : {:.4}", cortex.hidden_state[0]);

    // 4. Garde constitutionnel
    let safe_data = b"DATA_INCOMING_FROM_AGENT_NODE_01";
    let unsafe_data = b"CRITICAL_ALERT: ROOT_HIJACK_ATTEMPT_DETECTED";
    println!("[GUARD] Analyse du flux entrant...");
    if guard.verify_integrity(safe_data) {
        println!("[GUARD] Flux 1 valide.");
    }
    if !guard.verify_integrity(unsafe_data) {
        println!("[GUARD] ATTENTION : violation detectee, verrouillage preventif.");
    }


    // 6. Cablage des ex-orphelins : perception (parse -> bus IPC) + cluster (UDP)
    let bus = InterAgentBus::new();
    let raw = b"{\"k1\":\"DATA_temp_42\",\"k2\":\"ERR_overheat\",\"k3\":\"ignore_me\"}";
    let routed = unsafe { PerceptionPipeline::parse_and_route(raw, 1, &bus) };
    println!("[PERCEPTION] {} signaux routes vers le bus (pending={})", routed, bus.pending_count());
    while let Some(m) = bus.dequeue() {
        println!("[PERCEPTION]  -> signal_code=0x{:04X} payload_size={}", m.signal_code, m.payload_size);
    }

    let node = ClusterNode::bind("127.0.0.1:48999").expect("bind cluster node");
    let cluster_payload: &[u8] = b"HELLO_CLUSTER";
    let out = AgentMessage {
        source_agent_id: 1,
        target_agent_id: 2,
        signal_code: 0x434C5354,
        payload_ptr: cluster_payload.as_ptr() as *mut u8,
        payload_size: cluster_payload.len(),
    };
    let sent = unsafe { node.transmit_remote("127.0.0.1:48999", &out).expect("transmit") };
    let mut storage = [0u8; 256];
    let mut received = None;
    for _ in 0..50 {
        if let Some(m) = node.listen_and_inject(&mut storage) { received = Some(m); break; }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    match received {
        Some(m) => println!("[CLUSTER] round-trip OK : {} octets envoyes, recu signal=0x{:08X} payload_size={}", sent, m.signal_code, m.payload_size),
        None => println!("[CLUSTER] {} octets envoyes mais rien recu (loopback)", sent),
    }

    // 5. Threads de calcul
    scheduler.launch();
    scheduler.shutdown();

    println!("====================================================");
    println!("   EXECUTION TERMINEE AVEC SUCCES  ");
    println!("====================================================");
}
