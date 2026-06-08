mod config;

use soul_scheduler::scheduler::AgentScheduler;
use soul_matrix_engine::engine::MatrixEngine;
use soul_cortex::RecurrentCortex;
use soul_scout::SovereignScout;
use soul_guard::SystemGuard;
use soul_surgery::NeuralSurgeon;
use soul_cluster::ClusterNode;
use soul_perception::PerceptionPipeline;
use soul_ipc::bus::{AgentMessage, InterAgentBus};
use soul_acoustic::VadGate;
use soul_attention::KvCache;

fn main() {
    let cfg = config::SoulConfig::default();
    println!("====================================================");
    println!("   SOULSYSTEM KERNEL - INTEGRATION TRINITE SUPERIEURE");
    println!("====================================================");
    println!("[CONFIG] Configuration chargee : port={}, cluster={}, scout_timeout={}s",
        cfg.clinical_port, cfg.cluster_address, cfg.scout_timeout_secs);

    // 1. Composants physiques de base
    let scheduler = AgentScheduler::new();
    let matrix_engine = MatrixEngine::new(&scheduler.manifest);

    // 2. Briques superieures
    let mut cortex = RecurrentCortex::new();
    let _scout = SovereignScout::new("127.0.0.1", cfg.clinical_port)
        .with_timeout(std::time::Duration::from_secs(cfg.scout_timeout_secs));
    let guard = SystemGuard::new();

    // 2bis. KV-cache pour le cortex (attention sinks + fenetre glissante)
    let mut kv_cache = KvCache::new(64, cfg.kv_cache_sinks, cfg.kv_cache_window);
    println!("[ATTENTION] KV-cache initialise : dim=64, sinks=2, window=16, capacity={}", kv_cache.capacity());

    // 2ter. VAD gate pour filtrer les entrees audio du pipeline de perception
    let mut vad_gate = VadGate::new()
        .with_energy_factor(3.0)
        .with_min_rms(0.01)
        .with_hangover(5);
    println!("[ACOUSTIC] VAD Gate initialise : energy_factor=3.0, min_rms=0.01, hangover=5");

    // 3. Cycle synaptique
    println!("[CORTEX] Initialisation de l'etat recurrent...");
    let mut sensory_input = vec![0.35f32; 64 * 64];
    unsafe {
        cortex.process_cognitive_cycle(&matrix_engine, sensory_input.as_mut_ptr());
    }
    println!("[CORTEX] Cycle 1 accompli. Activation residuelle h[0] : {:.4}", cortex.hidden_state[0]);

    // 3bis. Injection dans le KV-cache : le hidden state devient la valeur
    // pour la position courante, permettant au cortex de "se souvenir" des
    // cycles precedents via attention scaled-dot-product.
    let query = &cortex.hidden_state[..64];
    kv_cache.push(query, &cortex.hidden_state[..64]);
    println!("[ATTENTION] Hidden state injecte dans KV-cache (active_len={})", kv_cache.active_len());

    // 3ter. Chirurgie RepE : injection d'un concept dans l'activation recurrente REELLE.
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

    // 5. Demo VAD : detection d'activite vocale sur un flux PCM synthetique
    println!("[ACOUSTIC] Demo VAD sur flux PCM synthetique...");
    let mut pcm_silence = vec![0i16; 256];
    let mut pcm_voice = vec![16384i16; 256]; // rms ~0.5
    pcm_silence.extend_from_slice(&pcm_voice);
    pcm_silence.extend_from_slice(&vec![0i16; 256]);
    let segments = vad_gate.segment(&pcm_silence, 256);
    println!("[ACOUSTIC] {} segment(s) voise(s) detecte(s)", segments.len());
    for (i, seg) in segments.iter().enumerate() {
        println!("[ACOUSTIC]   segment {}: echantillons [{}..{}]", i, seg.start, seg.end);
    }

    // 6. Cablage perception (parse -> bus IPC) + cluster (UDP)
    let bus = InterAgentBus::new();
    let raw = b"{\"k1\":\"DATA_temp_42\",\"k2\":\"ERR_overheat\",\"k3\":\"ignore_me\"}";
    let routed = unsafe { PerceptionPipeline::parse_and_route(raw, 1, &bus) };
    println!("[PERCEPTION] {} signaux routes vers le bus (pending={})", routed, bus.pending_count());
    while let Some(m) = bus.dequeue() {
        println!("[PERCEPTION]  -> signal_code=0x{:04X} payload_size={}", m.signal_code, m.payload_size);
    }

    let node = match ClusterNode::bind("127.0.0.1:48999") {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[CLUSTER] bind echoue : {} -> mode loopback desactive", e);
            scheduler.launch();
            scheduler.shutdown();
            println!("====================================================");
            println!("   EXECUTION TERMINEE (cluster indisponible)  ");
            println!("====================================================");
            return;
        }
    };
    let cluster_payload: &[u8] = b"HELLO_CLUSTER";
    let out = AgentMessage {
        source_agent_id: 1,
        target_agent_id: 2,
        signal_code: 0x434C5354,
        payload_ptr: cluster_payload.as_ptr() as *mut u8,
        payload_size: cluster_payload.len(),
    };
    let sent = match unsafe { node.transmit_remote("127.0.0.1:48999", &out) } {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[CLUSTER] transmit echoue : {}", e);
            0
        }
    };
    let mut storage = [0u8; 256];
    let mut received = None;
    for _ in 0..50 {
        if let Some(m) = node.listen_and_inject(&mut storage) {
            received = Some(m);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    match received {
        Some(m) => println!(
            "[CLUSTER] round-trip OK : {} octets envoyes, recu signal=0x{:08X} payload_size={}",
            sent, m.signal_code, m.payload_size
        ),
        None => println!("[CLUSTER] {} octets envoyes mais rien recu (loopback)", sent),
    }

    // 7. Threads de calcul
    scheduler.launch();
    scheduler.shutdown();

    println!("====================================================");
    println!("   EXECUTION TERMINEE AVEC SUCCES  ");
    println!("====================================================");
}
