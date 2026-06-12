//! Auto-generated evolved brain module.
//! Generation: 156
//! Timestamp: 1778663821.724

pub fn evolved_module_config() -> Vec<EvolvedModuleDef> {
    vec![
        EvolvedModuleDef { name: "perception".into(), neuron_count: 241, position: (150.0, 100.0), color: "#4affb8".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "memory".into(), neuron_count: 181, position: (350.0, 80.0), color: "#4a9eff".into(), learning_rate: 0.25, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "reasoning".into(), neuron_count: 223, position: (550.0, 100.0), color: "#ff4a6a".into(), learning_rate: 0.15, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "learning".into(), neuron_count: 191, position: (200.0, 220.0), color: "#ffaa4a".into(), learning_rate: 0.3, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "output".into(), neuron_count: 184, position: (400.0, 220.0), color: "#9bff4a".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "merged_50".into(), neuron_count: 3220, position: (600.0, 220.0), color: "#ff4aff".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "merged_100".into(), neuron_count: 3247, position: (150.0, 350.0), color: "#4affff".into(), learning_rate: 0.25, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "vision".into(), neuron_count: 3000, position: (350.0, 350.0), color: "#ff4a4a".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "merged_21".into(), neuron_count: 3000, position: (550.0, 350.0), color: "#4a4aff".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "motor".into(), neuron_count: 203, position: (400.0, 420.0), color: "#ffff4a".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_27".into(), neuron_count: 253, position: (192.7, 89.0), color: "#fcb5a0".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_41".into(), neuron_count: 902, position: (509.6, 463.7), color: "#ac335a".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_45".into(), neuron_count: 942, position: (429.0, 411.1), color: "#72a2b8".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_48".into(), neuron_count: 243, position: (570.1, 376.1), color: "#17acae".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_55".into(), neuron_count: 460, position: (184.0, 414.2), color: "#42f964".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_59".into(), neuron_count: 990, position: (417.8, 214.5), color: "#ad664e".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_66".into(), neuron_count: 278, position: (205.4, 113.7), color: "#807802".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_76".into(), neuron_count: 437, position: (330.0, 173.2), color: "#307ebb".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_79".into(), neuron_count: 530, position: (120.5, 83.2), color: "#98a2f4".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
        EvolvedModuleDef { name: "evolved_102".into(), neuron_count: 441, position: (478.1, 278.9), color: "#ff385c".into(), learning_rate: 0.2, firing_threshold_mean: 0.75, firing_threshold_std: 0.1, synaptic_density: 0.3 },
    ]
}

pub fn evolved_params() -> EvolvedParams {
    EvolvedParams {
        tau: 0.99900,
        homeostatic_rate: 0.00500,
        prune_threshold: 0.05000,
        stdp_learning_rate: 0.00500,
        evolution_interval: 100,
        energy_capacity: 1.00000,
        base_metabolic_rate: 0.0001000,
        firing_energy_cost: 0.02000,
        synapse_energy_cost: 0.00001000,
        energy_recharge_rate: 0.001551,
        metabolism_enabled: true,
        cortex: CortexEvolvedConfig {
            d_model: 17,
            state_dim: 5,
            n_layers: 2,
            modulation_strength: 0.300,
        },
    }
}

pub struct EvolvedModuleDef {
    pub name: String,
    pub neuron_count: usize,
    pub position: (f32, f32),
    pub color: String,
    pub learning_rate: f32,
    pub firing_threshold_mean: f32,
    pub firing_threshold_std: f32,
    pub synaptic_density: f32,
}

pub struct EvolvedParams {
    pub tau: f64,
    pub homeostatic_rate: f64,
    pub prune_threshold: f64,
    pub stdp_learning_rate: f64,
    pub evolution_interval: u64,
    pub energy_capacity: f64,
    pub base_metabolic_rate: f64,
    pub firing_energy_cost: f64,
    pub synapse_energy_cost: f64,
    pub energy_recharge_rate: f64,
    pub metabolism_enabled: bool,
    pub cortex: CortexEvolvedConfig,
}

pub struct CortexEvolvedConfig {
    pub d_model: usize,
    pub state_dim: usize,
    pub n_layers: usize,
    pub modulation_strength: f64,
}

pub fn evolved_script_count() -> i32 { 22 }

pub fn evolved_script_name(idx: i32) -> &'static str {
    match idx {
        0 => "evolved_41",
        1 => "evolved_66",
        2 => "merged_100",
        3 => "perception",
        4 => "evolved_27",
        5 => "motor",
        6 => "reasoning",
        7 => "merged_50",
        8 => "memory",
        9 => "evolved_102",
        10 => "merged_21",
        11 => "evolved_45",
        12 => "learning",
        13 => "extra",
        14 => "evolved_59",
        15 => "evolved_79",
        16 => "audio",
        17 => "evolved_76",
        18 => "evolved_48",
        19 => "attention",
        20 => "output",
        21 => "vision",
        _ => "",
    }
}

pub fn evolved_script_source(idx: i32) -> &'static str {
    match idx {
        0 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        1 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        2 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        3 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        4 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        5 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        6 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        7 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        8 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        9 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        10 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        11 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        12 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        13 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        14 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        15 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        16 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        17 => r###"sigmoid(potential * 1.5 + (reserve - 0.5) * 0.2)"###,
        18 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        19 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        20 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        21 => r###"sigmoid(potential * 1.5 + (noise - 0.5) * 0.2)"###,
        _ => "",
    }
}

pub fn evolved_plasticity_script_present() -> i32 { 0 }
pub fn evolved_plasticity_script_source() -> &'static str { "" }

