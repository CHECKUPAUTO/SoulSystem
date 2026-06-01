use serde::{Deserialize, Serialize};

// LIF parameters
pub const VR: f64 = -70.0;
pub const VT: f64 = -55.0;
pub const VZ: f64 = -75.0;
pub const TAU: f64 = 20.0;
pub const TREF: f64 = 3.0;
pub const DT: f64 = 0.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub neuron_count: usize,
    pub color: String,
    pub weight: f64,
    pub pos: [f64; 3],
}

pub fn default_modules() -> Vec<Module> {
    vec![
        Module { name: "perception".into(), neuron_count: 20, color: "#3dffc0".into(), weight: 0.15, pos: [-158.0, 82.0, 78.0] },
        Module { name: "memory".into(),     neuron_count: 50, color: "#ff6b6b".into(), weight: 0.20, pos: [142.0, 20.0, 20.0] },
        Module { name: "reasoning".into(),  neuron_count: 40, color: "#ffd93d".into(), weight: 0.25, pos: [-80.0, -96.0, 120.0] },
        Module { name: "action".into(),     neuron_count: 30, color: "#6bcbff".into(), weight: 0.15, pos: [160.0, -50.0, -80.0] },
        Module { name: "emotion".into(),    neuron_count: 25, color: "#c084fc".into(), weight: 0.10, pos: [-200.0, 40.0, -60.0] },
        Module { name: "meta".into(),       neuron_count: 35, color: "#fb923c".into(), weight: 0.15, pos: [40.0, 160.0, 20.0] },
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neuron {
    pub id: u64,
    pub module: String,
    pub v: f64,
    pub refractory: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Neuron {
    pub fn new(id: u64, module: &str, x: f64, y: f64, z: f64) -> Self {
        Self { id, module: module.to_string(), v: VR, refractory: 0.0, x, y, z }
    }

    pub fn update(&mut self, input: f64) -> bool {
        if self.refractory > 0.0 {
            self.refractory -= DT;
            return false;
        }
        self.v += (VR - self.v + input) / TAU * DT;
        if self.v >= VT {
            self.v = VZ;
            self.refractory = TREF;
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Synapse {
    pub source: u64,
    pub target: u64,
    pub weight: f64,
    pub delay: f64,
}

impl Synapse {
    pub fn new(source: u64, target: u64, weight: f64) -> Self {
        let mut rng = rand::thread_rng();
        Self { source, target, weight, delay: 1.0 + rng.gen::<f64>() * 5.0 }
    }
}

use rand::Rng;

pub fn random_pos(center: [f64; 3], spread: f64) -> (f64, f64, f64) {
    let mut rng = rand::thread_rng();
    (
        center[0] + rng.gen_range(-spread..spread),
        center[1] + rng.gen_range(-spread..spread),
        center[2] + rng.gen_range(-spread..spread),
    )
}