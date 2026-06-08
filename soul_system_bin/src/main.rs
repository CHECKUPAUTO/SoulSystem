#[allow(unused_imports)]
use scirust_affective_core::*;
#[allow(unused_imports)]
use semantic_neuromodulator::*;
#[allow(unused_imports)]
use ecosystem_synapse_linker::*;

use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::{SignalKind, signal};
use std::sync::atomic::{AtomicBool, Ordering};

use neural_metacognition::SystemAuditor;
use neural_clinical_console::ClinicalStreamingServer;
use semantic_firewall::FirewallGuard;

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

        // Pare-feu constitutionnel : on enregistre un PATTERN neurochimique interdit
        // (signature de panique : noradrenaline dominante) AVANT de partager le guard.
        let mut firewall_guard = FirewallGuard::new();
        let forbidden_panic =
            scirust::autodiff::reverse::Tensor::from_vec(vec![0.0, 1.0, 0.0], 1, 3);
        firewall_guard.register_forbidden(&forbidden_panic);
        let firewall = Arc::new(firewall_guard);

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
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[SYSTEM] echec setup SIGINT: {} -> signal non disponible", e);
                return;
            }
        };
        sigint.recv().await;
        r_clone.store(false, Ordering::SeqCst);
    });

    // Boucle affective : decroissance homeostatique reelle vers la ligne de base.
    let a_ctx = ctx.clone();
    std::thread::spawn(move || {
        pin_thread(32);
        loop {
            a_ctx.affective_state
                .decay_towards_baseline(0.1, &[0.0, 0.0, 0.0], &[0.01, 0.01, 0.01]);
            std::thread::sleep(Duration::from_millis(100));
        }
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
        // Porte de surete constitutionnelle : on gate l'etat neurochimique REEL
        // (calcule depuis le PAD courant) a chaque cycle.
        let pad = ctx.affective_state.get_coordinates();
        let pad_t = scirust::autodiff::reverse::Tensor::from_vec(vec![pad[0], pad[1], pad[2]], 1, 3);
        let profile = ctx.neuromodulator.compute_chemical_levels(&pad_t);
        let chem_t = scirust::autodiff::reverse::Tensor::from_vec(
            vec![profile.dopamine, profile.noradrenaline, profile.serotonin],
            1,
            3,
        );
        if !ctx.firewall.check_safety(&chem_t) {
            eprintln!(
                "[FIREWALL] etat neurochimique interdit (cos={:.3} >= {:.2}) D={:.3} N={:.3} S={:.3} -> retour homeostatique",
                ctx.firewall.max_similarity(&chem_t),
                ctx.firewall.threshold,
                profile.dopamine,
                profile.noradrenaline,
                profile.serotonin
            );
            ctx.affective_state
                .decay_towards_baseline(1.0, &[0.0, 0.0, 0.0], &[0.5, 0.5, 0.5]);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use semantic_firewall::FirewallGuard;
    use semantic_neuromodulator::neuromodulation::chemical_map::NeuromodulatorMapper;

    fn st(v: Vec<f32>) -> scirust::autodiff::reverse::Tensor {
        let n = v.len();
        scirust::autodiff::reverse::Tensor::from_vec(v, 1, n)
    }

    #[test]
    fn gate_autorise_etat_neurochimique_sain() {
        let mut fw = FirewallGuard::new();
        assert!(fw.register_forbidden(&st(vec![0.0, 1.0, 0.0]))); // pattern panique interdit
        let mapper = NeuromodulatorMapper::new(vec![0.1; 9], vec![0.05; 3]); // = bootstrap
        let pad = st(vec![0.2, 0.2, 0.2]);
        let p = mapper.compute_chemical_levels(&pad);
        let chem = st(vec![p.dopamine, p.noradrenaline, p.serotonin]);
        let sim = fw.max_similarity(&chem);
        assert!(fw.check_safety(&chem), "etat equilibre doit passer (cos={})", sim);
        println!(
            "PREUVE gate sain : profil [{:.3},{:.3},{:.3}] cos={:.3} < 0.85 -> autorise",
            p.dopamine, p.noradrenaline, p.serotonin, sim
        );
    }

    #[test]
    fn gate_bloque_etat_pathologique_via_pipeline() {
        let mut fw = FirewallGuard::new();
        fw.register_forbidden(&st(vec![0.0, 1.0, 0.0]));
        // mapper non-trivial : route uniquement vers la noradrenaline (ligne 1 de la 3x3)
        let weights = vec![
            0.0, 0.0, 0.0,
            1.0, 1.0, 1.0,
            0.0, 0.0, 0.0,
        ];
        let mapper = NeuromodulatorMapper::new(weights, vec![0.0; 3]);
        let pad = st(vec![1.0, 1.0, 1.0]);
        let p = mapper.compute_chemical_levels(&pad); // -> [0, 1, 0]
        let chem = st(vec![p.dopamine, p.noradrenaline, p.serotonin]);
        let sim = fw.max_similarity(&chem);
        assert!(!fw.check_safety(&chem), "etat nora-dominant doit etre bloque (cos={})", sim);
        println!(
            "PREUVE gate pathologique : profil [{:.3},{:.3},{:.3}] cos={:.3} >= 0.85 -> BLOQUE",
            p.dopamine, p.noradrenaline, p.serotonin, sim
        );
    }
}
