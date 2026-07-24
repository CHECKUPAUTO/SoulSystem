use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    #[serde(with = "duration_serde")]
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub max_divergence: f64,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        SandboxConfig {
            timeout: Duration::from_millis(50),
            max_output_bytes: 10_240,
            max_divergence: 100.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicsConfig {
    pub attraction_threshold: f64,
    pub repulsion_threshold: f64,
    pub entropy_decay: f64,
    pub entropy_reset: f64,
    pub base_force: f64,
    pub entropy_force_factor: f64,
}

impl Default for DynamicsConfig {
    fn default() -> Self {
        DynamicsConfig {
            attraction_threshold: 30.0,
            repulsion_threshold: 70.0,
            entropy_decay: 0.90,
            entropy_reset: 0.5,
            base_force: 5.0,
            entropy_force_factor: 8.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub active_capacity: usize,
    pub consolidation_threshold: f64,
    pub decay_rate: f64,
    pub initial_strength: f64,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        MemoryConfig {
            active_capacity: 100,
            consolidation_threshold: 3.0,
            decay_rate: 0.05,
            initial_strength: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationConfig {
    pub test_data_size: usize,
    pub test_data_count: usize,
    pub exploration_rate: f64,
}

impl Default for MutationConfig {
    fn default() -> Self {
        MutationConfig {
            test_data_size: 128,
            test_data_count: 8,
            exploration_rate: 0.6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CervoConfig {
    pub sandbox: SandboxConfig,
    pub dynamics: DynamicsConfig,
    pub memory: MemoryConfig,
    pub mutation: MutationConfig,
}

impl Default for CervoConfig {
    fn default() -> Self {
        CervoConfig {
            sandbox: SandboxConfig::default(),
            dynamics: DynamicsConfig::default(),
            memory: MemoryConfig::default(),
            mutation: MutationConfig::default(),
        }
    }
}

impl CervoConfig {
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.sandbox.max_output_bytes == 0 {
            return Err("max_output_bytes must be > 0".into());
        }
        if self.sandbox.max_divergence < 0.0 {
            return Err("max_divergence must be >= 0".into());
        }
        if self.dynamics.attraction_threshold >= self.dynamics.repulsion_threshold {
            return Err("attraction_threshold must be < repulsion_threshold".into());
        }
        if self.dynamics.entropy_decay <= 0.0 || self.dynamics.entropy_decay > 1.0 {
            return Err("entropy_decay must be in (0, 1]".into());
        }
        if self.memory.active_capacity == 0 {
            return Err("active_capacity must be > 0".into());
        }
        if self.memory.decay_rate < 0.0 {
            return Err("decay_rate must be >= 0".into());
        }
        if self.mutation.exploration_rate < 0.0 || self.mutation.exploration_rate > 1.0 {
            return Err("exploration_rate must be in [0, 1]".into());
        }
        Ok(())
    }
}

mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_millis().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}
