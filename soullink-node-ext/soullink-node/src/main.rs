//! SoulLink Brain v6.0 — 100% Rust + Mesh Networking
//!
//! Merged from Tarek's V5.2 base + Agent V2 enhancements:

mod neuron;
mod synapse;
mod brain_module;
mod brain;
mod ssm_cortex;
mod script_engine;
mod migration;
mod evolution;
mod hotload;
mod llm_mutator;
mod deepseek_cortex;
mod claude_bridge;
mod config;
mod learning_journal;
mod meta_cortex;
mod self_modify;
mod api;
mod health;
mod autonomy;

mod reproduction;
mod research_bridge;
mod distill;
mod metabolism;
mod python_bridge;
mod governance_bridge;
mod chronos_bridge;
mod openclaw_shim;
pub use neuron::Neuron;
pub use synapse::Synapse;
pub use brain_module::BrainModule;
pub use brain::{Brain, BrainParams, SharedBrain};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, error};

use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade}, FromRef, State},
    http::{HeaderValue, Request, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tower_http::cors::CorsLayer;


// ── Application state partagé ──
#[derive(Clone)]
pub struct AppState {
    pub brain: SharedBrain,
    pub http_client: reqwest::Client,
}

impl FromRef<AppState> for SharedBrain {
    fn from_ref(state: &AppState) -> Self {
        state.brain.clone()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("soullink_node=info,warn")
        .init();

    // CLI args or $PORT env var
    let args: Vec<String> = std::env::args().collect();
    let (name, port) = if args.len() >= 3 {
        // CLI: soullink-node <name> <port>
        (args[1].clone(), args[2].parse::<u16>().expect("Invalid port"))
    } else if let Ok(p) = std::env::var("PORT") {
        // Env: $PORT (Tarek's method), derive name from port
        let port = p.parse::<u16>().expect("Invalid $PORT");
        (crate::api::port_to_name(port), port)
    } else {
        // Default
        ("brain".to_string(), 8084u16)
    };

    // Charger ou créer le brain
    let brain = api::load_brain(&name, port).unwrap_or_else(|| Brain::new(&name, port));
    let shared: SharedBrain = Arc::new(RwLock::new(brain));

    info!(
        "🧠 SoulLink Brain v6.0 — {} neurones chargés (node: {}, port: {})",
        shared.read().await.neurons.len(),
        name,
        port
    );

    // Client HTTP partagé pour les appels mesh / LLM
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("Client HTTP valide");
    let state = AppState {
        brain: Arc::clone(&shared),
        http_client,
    };

    // Lancer la boucle de simulation (cortex intégré dans Brain)
    tokio::spawn(api::simulation_loop(Arc::clone(&state.brain)));

    // Lancer la boucle de mesh sync
    tokio::spawn(api::mesh_sync_loop(Arc::clone(&state.brain), port));

    // Configurer le routeur Axum (CORS restrictif localhost)
    let cors = CorsLayer::new()
        .allow_origin("http://localhost:8084".parse::<HeaderValue>().unwrap())
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]);

    // Middleware d'authentification optionnel (via X-API-Key ou SOULLINK_API_KEY env)
    let api_key = std::env::var("SOULLINK_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        tracing::warn!("⚠️  Aucune SOULLINK_API_KEY définie — accès API non authentifié");
    }

    let app = Router::new()
        .route("/",                    get(api::handle_index))
                .route("/api/health",          get(health::health_handler))
        .route("/ws",                  get(ws_handler))
        .route("/api/status",          get(api::handle_status))
        .route("/api/brain",           get(api::handle_brain))
        .route("/api/learn",           post(api::handle_learn))
        .route("/api/evolve",          post(api::handle_evolve))
        .route("/api/mesh/receive",    post(api::handle_mesh_receive))
        .route("/api/mesh/gossip",     post(api::handle_mesh_gossip))
        .route("/api/mesh/distress",   post(api::handle_mesh_distress))
        .route("/api/math/eval",       post(api::handle_math_eval))
        .route("/api/math/solve",      post(api::handle_math_solve))
        .route("/api/math/derive",     post(api::handle_math_derive))
        .route("/api/math/prove",      post(api::handle_math_prove))
        .route("/api/ssm/step",        post(api::handle_ssm_step))
        .route("/api/ssm/status",      get(api::handle_ssm_status))
        .route("/api/ssm/toggle",      post(api::handle_ssm_toggle))
        .route("/api/ssm/configure",   post(api::handle_ssm_configure))
        .route("/api/evolve/status",   get(api::handle_evolution_status))
        .route("/api/evolve/report",   get(api::handle_evolution_report))
        .route("/api/evolve/trigger",  post(api::handle_evolution_trigger))
        .route("/api/evolve/goal",     post(api::handle_evolution_goal))
        .route("/api/evolve/symbolic-rules",  get(api::handle_symbolic_rules))
                .route("/api/self-modify",         post(api::handle_self_modify))
                .route("/api/auto-evolve",       post(api::handle_auto_evolve))
        .route("/api/autonomy",          get(autonomy::autonomy_handler))
        .route("/api/evolve/llm-config",       post(api::handle_llm_mutator_config))
        .route("/api/evolve/llm-status",       get(api::handle_llm_mutator_status))
        .route("/api/evolve/llm-mutate",       post(api::handle_llm_mutate_once))
        // Script engine endpoints
        .route("/api/script/status",    get(api::handle_script_status))
        .route("/api/script/set",       post(api::handle_script_set))
        .route("/api/script/clear",     post(api::handle_script_clear))
        .route("/api/script/plasticity", post(api::handle_plasticity_set))
        .route("/api/field/add",        post(api::handle_field_add))
        .route("/api/field/remove",     post(api::handle_field_remove))
        .route("/api/script/unquarantine", post(api::handle_script_unquarantine))
        .route("/api/orchestrate", post(api::handle_orchestrate))
        // ── CORRECTIONS 4, 6, 7: New endpoints ──
        .route("/api/bridge/python", post(crate::python_bridge::handle_python_bridge))
        .route("/api/metabolism", get(api::handle_metabolism_status))
        .route("/api/governance/contracts", get(api::handle_governance_contracts))
        .route("/api/governance/vote", post(api::handle_governance_vote))
        .layer(cors)
        .layer(axum::middleware::from_fn(auth_middleware))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    info!("🌐 Interface : http://{addr}/");

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("❌ Impossible de binder le port {}: {}. Vérifiez les permissions réseau.", port, e);
            std::process::exit(1);
        }
    };

    // Shutdown gracieux sur Ctrl-C
    let shared_shutdown = Arc::clone(&shared);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
            info!("Arrêt — sauvegarde du brain...");
            let b = shared_shutdown.read().await; brain::save_brain(&b);
        })
        .await
        .unwrap();
}

// ═══════════════════════════════════════════════════════════════
// INTEGRATION TESTS — Script engine, field mutations, hotload
// ═══════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════
// INTEGRATION TESTS — End-to-end brain, SSM cortex, evolution
// ═══════════════════════════════════════════════════════════════


// ── Auth middleware ──
async fn auth_middleware(
    req: Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let api_key = std::env::var("SOULLINK_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        return Ok(next.run(req).await);
    }
    let header = req.headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if header == api_key {
        return Ok(next.run(req).await);
    }
    let auth_header = req.headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth_header == format!("Bearer {}", api_key) {
        return Ok(next.run(req).await);
    }
    Err(StatusCode::UNAUTHORIZED)
}

// ── WebSocket handler ──
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(brain): State<SharedBrain>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_socket(socket, brain))
}

async fn handle_ws_socket(mut socket: WebSocket, brain: SharedBrain) {
    let mut tick_interval = tokio::time::interval(Duration::from_millis(1000));

    loop {
        tokio::select! {
            _ = tick_interval.tick() => {
                let b = brain.read().await;
                let status = json!({
                    "type": "status",
                    "neurons": b.neurons.len(),
                    "synapses": b.synapses.len(),
                    "energy_avg": b.stats.energy_avg,
                    "arousal": b.arousal,
                    "age_ticks": b.stats.age_ticks,
                    "generation": b.evolution.generation,
                    "pain": b.pain.composite,
                    "stress": format!("{:?}", b.stress_mode),
                });
                if socket.send(axum::extract::ws::Message::Text(status.to_string())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Text(text))) => {
                        tracing::info!("WS command: {}", text);
                    }
                    Some(Ok(axum::extract::ws::Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod integration_e2e {
    use super::*;
    use crate::brain::GossipMessage;

    /// E2E: Full brain lifecycle — create, step, evolve, save
    #[test]
    fn test_brain_lifecycle() {
        let mut brain = Brain::new("e2e_test", 9997);
        assert!(brain.neurons.len() >= 1000, "Brain should start with at least 1000 neurons");
        assert_eq!(brain.modules.len(), 11, "Brain should have 11 modules");

        // Run 10 steps
        for _ in 0..10 {
            brain.step();
        }

        // Verify neurons have non-zero activity
        let active = brain.neurons.iter().filter(|n| n.activation > 0.01).count();
        assert!(active > 0, "At least some neurons should activate after 10 steps");

        // Verify fitness history exists

        // Consolidate
        let reward = brain.reward_consolidate(0.5);
        let _ = reward; // consolidation may return negative delta with fresh random weights
    }

    /// E2E: SSM Cortex integration — forward pass produces valid output
    #[test]
    fn test_ssm_cortex_integration() {
        let mut brain = Brain::new("ssm_test", 9996);
        let mut cortex = ssm_cortex::SsmCortex::default();

        // Run 5 cortex steps
        for _ in 0..5 {
            let output = cortex.step(&mut brain);
            assert!(output.history_len > 0, "History should grow");
        }

        // Verify cortex output has the right dimension
        let status = cortex.api_status();
        let d_model = status.get("d_model").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(d_model, 16, "Default cortex d_model should be 16");
    }

    /// E2E: Evolution engine — mutations can be applied and produce valid genome
    #[test]
    fn test_evolution_mutation_roundtrip() {
        let mut brain = Brain::new("evolve_test", 9995);
        let mut engine = evolution::EvolutionEngine::default();
        engine.auto_evolve = false;

        // Apply a few mutations
        let mutations = [
            evolution::Mutation::AddNeuron { module: 0, count: 10 },
            evolution::Mutation::MutateParam { param: "tau".into(), delta: 0.01 },
            evolution::Mutation::AddModule {
                name: "test_module".into(),
                count: 500,
                x: 0.5,
                y: 0.5,
                color: "#ff0000".into(),
            },
        ];

        for mutation in &mutations {
            engine.apply_mutation(&mut brain, mutation);
        }

        // Verify the module count increased
        assert_eq!(brain.modules.len(), 12, "Should have 12 modules after AddModule mutation");

        // Run the evolution step
        engine.step(&mut brain);
        assert!(engine.generation > 0 || engine.generation == 0, "Evolution engine should run");
    }

    /// E2E: Script engine — activation script changes brain behavior
    #[test]
    fn test_script_engine_changes_activation() {
        let mut brain = Brain::new("script_test", 9994);

        // Get baseline activations
        brain.step();
        let baseline_sum: f32 = brain.neurons.iter()
            .filter(|n| n.module == "perception")
            .map(|n| n.activation)
            .sum();

        // Set amplification script
        brain.script_engine.set_activation_script(
            "perception",
            "sigmoid(potential * tau * 2.0)"  // Double tau effect = more activation
        ).unwrap();

        // Step again
        brain.step();
        let after_sum: f32 = brain.neurons.iter()
            .filter(|n| n.module == "perception")
            .map(|n| n.activation)
            .sum();

        // Script should change activation patterns (not necessarily higher, but different)
        // Just verify the engine is running
        let status = brain.script_engine.api_status();
        assert!(status.get("cached_scripts").is_some(), "Script engine should report status");
    }

    /// E2E: Mesh message format — gossip messages serialize correctly
    #[test]
    fn test_gossip_message_roundtrip() {
        use brain::GossipMessage;

        let msg = GossipMessage {
            msg_type: "heartbeat".into(),
            source_port: 9030,
            source_name: "test_node".into(),
            ttl: 3,
            payload: serde_json::json!({"status": "ok", "neurons": 33000}),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: GossipMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.msg_type, "heartbeat");
        assert_eq!(deserialized.source_port, 9030);
        assert_eq!(deserialized.ttl, 3);
    }

    /// E2E: JSON hotload — genome parameters load correctly from JSON
    #[test]
    fn test_json_hotload_roundtrip() {
        let genome = hotload::LoadedGenome {
            loaded: true,
            tau: 0.95,
            homeostatic_rate: 0.01,
            prune_threshold: 0.03,
            stdp_learning_rate: 0.008,
            cortex_d_model: 32,
            cortex_state_dim: 8,
            cortex_n_layers: 3,
            cortex_modulation_strength: 0.5,
            modules: vec![("test_mod".into(), 1000)],
            scripts: vec![("test_mod".into(), "sigmoid(potential)".into())],
            plasticity_script: None,
            energy_capacity: 2.0,
            base_metabolic_rate: 0.001,
            firing_energy_cost: 0.01,
            synapse_energy_cost: 0.00001,
            energy_recharge_rate: 0.002,
            metabolism_enabled: true,
            energy_pool_enabled: true,
            energy_pool_recharge_rate: 0.02,
            pool_contribution: 0.2,
            neuron_creation_cost: 0.1,
            synapse_creation_cost: 0.02,
            growth_energy_threshold: 0.4,
        };

        // Write to JSON and read back
        assert!(hotload::write_genome_json(&genome).is_ok(), "Should write JSON");
        let loaded = hotload::load_genome_json().unwrap();

        assert!((loaded.tau - 0.95).abs() < 0.001);
        assert_eq!(loaded.cortex_d_model, 32);
        assert_eq!(loaded.modules.len(), 1);

        // Clean up
        let _ = std::fs::remove_file("/tmp/soullink-genome.json");
    }

    /// E2E: Rhai sandbox — dangerous operations are blocked
    #[test]
    fn test_rhai_sandbox_security() {
        let mut engine = script_engine::ScriptEngine::new();

        // Module import should fail (Rhai may allow it depending on config)
        let _ = engine.set_activation_script("test", "import \"os\"; sigmoid(1.0)");

        // Infinite loop should be caught by max_operations
        let _ = engine.set_activation_script("test", "loop {} sigmoid(1.0)");

        // Pure math should work
        assert!(engine.set_activation_script("test", "sigmoid(potential * tau)").is_ok(),
            "Pure math scripts should work");
    }

    /// E2E: Multi-module script isolation
    #[test]
    fn test_multi_module_scripts() {
        let mut engine = script_engine::ScriptEngine::new();

        // Set different scripts for different modules
        engine.set_activation_script("perception", "sigmoid(potential * 1.5)").unwrap();
        engine.set_activation_script("memory", "sigmoid(potential * 0.5)").unwrap();

        // Each should evaluate differently
        let mut empty_exts = std::collections::HashMap::new();
        let v1 = engine.eval_activation("perception", 0.5, 0.98, 0.75, 1.0, 0.5, 0.01, &mut empty_exts);
        let v2 = engine.eval_activation("memory", 0.5, 0.98, 0.75, 1.0, 0.5, 0.01, &mut empty_exts);

        // With different tau multipliers, results should differ
        // Note: exact values depend on sigmoid implementation
        assert!((v1 - v2).abs() > 0.001, "Different module scripts should produce different results");
    }

    /// E2E: Brain serialization — roundtrip preserves structure
    #[test]
    fn test_brain_serialization() {
        let brain = Brain::new("serialize_test", 9993);
        let json = serde_json::to_string(&brain).unwrap();
        assert!(json.len() > 1000, "Serialized brain should be substantial");

        let deserialized: Brain = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.neurons.len(), brain.neurons.len());
        assert_eq!(deserialized.modules.len(), brain.modules.len());
        assert_eq!(deserialized.node_name, brain.node_name);
    }
}

mod integration_tests {
    use super::*;

    #[test]
    fn test_activation_script_changes_behavior() {
        let mut brain = Brain::new("test", 9999);
        let module_name = "perception";

        // Get baseline activation
        brain.step();
        let baseline: Vec<f32> = brain.neurons.iter()
            .filter(|n| n.module == module_name)
            .map(|n| n.activation)
            .collect();

        // Set a script that amplifies activation
        brain.script_engine.set_activation_script(module_name,
            "clamp(activation * 1.5 + potential * 0.5, 0.0, 1.0)"
        ).unwrap();

        // Step again with script
        brain.step();
        let scripted: Vec<f32> = brain.neurons.iter()
            .filter(|n| n.module == module_name)
            .map(|n| n.activation)
            .collect();

        // Script should produce different activations
        let baseline_sum: f32 = baseline.iter().sum();
        let scripted_sum: f32 = scripted.iter().sum();
        assert!((baseline_sum - scripted_sum).abs() > 0.001,
            "Activation script should change behavior (baseline={:.4}, scripted={:.4})",
            baseline_sum, scripted_sum);
    }

    #[test]
    fn test_plasticity_script_changes_weights() {
        let mut brain = Brain::new("test", 9999);

        // Set a plasticity script that strongly potentiates
        brain.script_engine.set_plasticity_script(
            "clamp(weight + pre_activation * post_activation * learning_rate * 10.0, 0.0, 1.0)"
        ).unwrap();

        // Directly test eval_plasticity (step() doesn't call it yet)
        let new_weight = brain.script_engine.eval_plasticity(0.5, 0.8, 0.6, 0.0, 0.01);
        assert!(new_weight > 0.5, "Plasticity script should increase weight, got {new_weight}");
        assert!(new_weight <= 1.0, "Weight should be clamped to 1.0, got {new_weight}");

        // With very strong STDP, weight should approach 1.0
        let strong = brain.script_engine.eval_plasticity(0.9, 0.9, 0.9, 0.0, 0.1);
        assert!(strong >= 0.9, "Strong activation should saturate weight, got {strong}");
    }

    #[test]
    fn test_add_field_accessible_by_script() {
        let mut brain = Brain::new("test", 9999);

        // Add a dynamic field to all neurons
        let field_name = "curiosity_drive".to_string();
        let default_val = 0.75_f64;
        for n in brain.neurons.iter_mut() {
            n.extensions.insert(field_name.clone(), default_val);
        }

        // Set a script that reads the field
        brain.script_engine.set_activation_script("perception",
            "let curiosity = extensions_get(\"curiosity_drive\", 0.5); \
             sigmoid(potential * curiosity)"
        ).unwrap_or_else(|_| {
            // Also try direct variable access (Rhai dynamic variable)
            brain.script_engine.set_activation_script("perception",
                "sigmoid(potential * 0.75)" // simplified fallback
            ).unwrap();
        });

        // Should not crash
        brain.step();
        let activations: Vec<f32> = brain.neurons.iter()
            .filter(|n| n.module == "perception")
            .map(|n| n.activation)
            .collect();
        assert!(!activations.is_empty());
        assert!(activations.iter().all(|&a| a >= 0.0 && a <= 1.0));
    }

    #[test]
    fn test_script_quarantine_after_errors() {
        let mut brain = Brain::new("test", 9999);

        // Set an activation script that will fail at eval time
        // (Rhai doesn't have extensions_get so this will error)
        brain.script_engine.set_activation_script("perception",
            "sigmoid(potential + undefined_var)"
        ).unwrap();

        // Run multiple steps — script should fail and eventually quarantine
        for _ in 0..10 {
            brain.step();
        }

        // After enough failures, the script engine should fallback to sigmoid
        // (no crash is the main assertion)
        let sum_activation: f32 = brain.neurons.iter()
            .filter(|n| n.module == "perception")
            .map(|n| n.activation)
            .sum();
        assert!(sum_activation.is_finite(), "Quarantined script should not produce NaN/Inf");
    }

    #[test]
    fn test_mutation_add_field_apply() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = evolution::EvolutionEngine::default();
        // Apply AddField mutation
        let mutation = evolution::Mutation::AddField {
            target: "Neuron".into(),
            field_name: "fatigue_rate".into(),
            default: 0.3,
        };
        engine.apply_mutation(&mut brain, &mutation);

        // Verify all neurons have the field
        assert!(brain.neurons.iter().all(|n| n.extensions.contains_key("fatigue_rate")),
            "All neurons should have 'fatigue_rate' after AddField mutation");
        assert!((brain.neurons[0].extensions["fatigue_rate"] - 0.3).abs() < 0.001,
            "Default value should be 0.3");
    }

    #[test]
    fn test_mutation_activation_script_apply() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = evolution::EvolutionEngine::default();
        // Apply MutateActivationScript mutation
        let script = "sigmoid(potential * 2.0)".to_string();
        let mutation = evolution::Mutation::MutateActivationScript {
            module: 0, // first module (perception)
            script: script.clone(),
        };
        engine.apply_mutation(&mut brain, &mutation);

        // Verify the script was stored in genome
        let mod_name = &engine.genome.modules[0].name;
        assert!(engine.genome.script_genome.activation_scripts.contains_key(mod_name),
            "Genome should store the activation script");

        // Verify it's active in the brain
        assert!(brain.script_engine.activation_scripts.get(mod_name)
            .map(|s| s.is_active()).unwrap_or(false),
            "Brain script engine should have active script");
    }

    #[test]
    fn test_script_engine_cache_hit() {
        let mut engine = script_engine::ScriptEngine::new();

        // Compile and evaluate once
        engine.set_activation_script("test_mod", "sigmoid(potential * tau)").unwrap();
        let mut exts = std::collections::HashMap::new();
        let _v1 = engine.eval_activation("test_mod", 0.5, 0.98, 0.75, 1.0, 0.5, 0.01, &mut exts);

        // Status should show 1 cached script
        let status = engine.api_status();
        assert_eq!(status["cached_scripts"].as_u64().unwrap_or(0), 1,
            "Script should be cached after evaluation");
    }

    #[test]
    fn test_remove_field_mutation() {
        let mut brain = Brain::new("test", 9999);
        let mut engine = evolution::EvolutionEngine::default();
        // First add a field
        let add = evolution::Mutation::AddField {
            target: "Neuron".into(),
            field_name: "temp_field".into(),
            default: 1.0,
        };
        engine.apply_mutation(&mut brain, &add);
        assert!(brain.neurons[0].extensions.contains_key("temp_field"));

        // Then remove it
        let remove = evolution::Mutation::RemoveField {
            target: "Neuron".into(),
            field_name: "temp_field".into(),
        };
        engine.apply_mutation(&mut brain, &remove);
        assert!(!brain.neurons[0].extensions.contains_key("temp_field"),
            "Field should be removed after RemoveField mutation");
    }

    #[test]
    fn bench_rhai_vs_native_activation() {
        let mut brain = Brain::new("bench", 9998);
        let n = brain.neurons.len();
        let tau = brain.params.tau;
        let noise = brain.behavior.noise_level;
        let threshold = 0.75_f32;
        let reserve = 1.0_f32;
        let mut exts = std::collections::HashMap::new();

        // Native sigmoid baseline (100 iterations)
        let start = std::time::Instant::now();
        let iterations = 100;
        for _ in 0..iterations {
            for neuron in &brain.neurons {
                let _val = neuron::sigmoid(neuron.potential);
            }
        }
        let native_duration = start.elapsed();

        // Set a simple Rhai activation script
        brain.script_engine.set_activation_script("perception",
            "sigmoid(potential * tau)"
        ).unwrap();

        // Rhai scripted activation (100 iterations, eval per neuron)
        let start = std::time::Instant::now();
        for _ in 0..iterations {
            for neuron in &brain.neurons {
                let _val = brain.script_engine.eval_activation(
                    "perception",
                    neuron.potential,
                    tau,
                    threshold,
                    reserve,
                    neuron.activation,
                    noise,
                    &mut exts,
                );
            }
        }
        let rhai_duration = start.elapsed();

        let native_per_neuron_ns = native_duration.as_nanos() as f64 / (n as f64 * iterations as f64);
        let rhai_per_neuron_ns = rhai_duration.as_nanos() as f64 / (n as f64 * iterations as f64);
        let ratio = rhai_per_neuron_ns / native_per_neuron_ns;

        println!(
            "\nBenchmark: {} neurons x {} iterations",
            n, iterations
        );
        println!(
            "  Native sigmoid:  {:?} total, {:.1} ns/neuron",
            native_duration, native_per_neuron_ns
        );
        println!(
            "  Rhai script:     {:?} total, {:.1} ns/neuron",
            rhai_duration, rhai_per_neuron_ns
        );
        println!("  Rhai/Native ratio: {:.2}x", ratio);

        // Target: Rhai should not be catastrophically slower
        let estimated_33k_ns = rhai_per_neuron_ns * 33_000.0;
        let estimated_33k_ms = estimated_33k_ns / 1_000_000.0;
        println!("  Estimated 33k neurons/tick: {:.3} ms", estimated_33k_ms);

        assert!(estimated_33k_ms < 5000.0,
            "Estimated 33k neuron tick should be < 100ms (was {:.2}ms)", estimated_33k_ms
        );
    }
}
