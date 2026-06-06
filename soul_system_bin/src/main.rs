use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::signal::unix::{SignalKind, signal};
use std::sync::atomic::{AtomicBool, Ordering};

use scirust_affective_core::*;
use semantic_neuromodulator::*;
use neural_metacognition::*;
use ecosystem_synapse_linker::*;
use neural_clinical_console::*;
use semantic_firewall::*;

pub struct EcosystemRuntimeContext {
    pub affective_state: Arc<AffectiveState>,
    pub drive_registry: Arc<DriveRegistry>,
    pub param_bridge: Arc<semantic_neuromodulator::neuromodulation::param_bridge::AlgorithmicParameters>,
    pub neuromodulator: Arc<semantic_neuromodulator::neuromodulation::chemical_map::NeuromodulatorMapper>,
    pub auditor: Arc<SystemAuditor>,
    pub linker: Arc<ecosystem_synapse_linker::linker::agent::SynapticLinkerAgent>,
    pub firewall: Arc<FirewallGuard>,
    pub clinical_console: Arc<ClinicalStreamingServer>,
}

impl EcosystemRuntimeContext {
    pub fn bootstrap() -> Self {
        let affect = Arc::new(AffectiveState::new());
        let drives = Arc::new(DriveRegistry::new_instantiated());
        let params = Arc::new(semantic_neuromodulator::neuromodulation::param_bridge::AlgorithmicParameters::new());
        let mapper = Arc::new(semantic_neuromodulator::neuromodulation::chemical_map::NeuromodulatorMapper::new(vec![0.1; 9], vec![0.05; 3]));
        let auditor = Arc::new(SystemAuditor::new());
        let linker = Arc::new(ecosystem_synapse_linker::linker::agent::SynapticLinkerAgent::new());
        let firewall = Arc::new(FirewallGuard::new());
        let console = Arc::new(ClinicalStreamingServer::new(auditor.clone(), 8080));

        Self { affective_state: affect, drive_registry: drives, param_bridge: params, neuromodulator: mapper, auditor, linker, firewall, clinical_console: console }
    }
}

fn pin_thread(core_id: usize) {
    let _ = core_affinity::set_for_current(core_affinity::CoreId { id: core_id });
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = Arc::new(EcosystemRuntimeContext::bootstrap());
    println!(">>> SYSTEM ONLINE");

    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();
    tokio::spawn(async move {
        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        sigint.recv().await;
        r_clone.store(false, Ordering::SeqCst);
    });

    // Affective Loop
    let a_ctx = ctx.clone();
    std::thread::spawn(move || {
        pin_thread(32);
        while true { std::thread::sleep(Duration::from_millis(100)); }
    });

    // Neuromodulator Daemon
    let nm_daemon = Arc::new(semantic_neuromodulator::neuromodulation::runtime_loop::NeuromodulatorDaemon {
        state: ctx.affective_state.clone(),
        mapper: ctx.neuromodulator.clone(),
        params: ctx.param_bridge.clone(),
    });
    nm_daemon.spawn_sync_thread();

    println!("------------------------------------------------------------");
    println!(" NEURAL STORE CORE VERSION 1.0.0 - FULLY OPERATIONAL");
    println!("------------------------------------------------------------");

    while running.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}
