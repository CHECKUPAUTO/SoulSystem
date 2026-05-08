//! AfferentNerve — Hardware senses that feed into the neural mesh.
//!
//! Reads GPU temperature (and eventually other sensors) and converts
//! them into neural signals. GPU thermal noise becomes turbulence
//! in the Hamiltonian dynamics.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// Current sensor readings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorState {
    pub gpu_temp_c: f64,
    pub gpu_power_w: f64,
    pub gpu_util_pct: f64,
    pub cpu_temp_c: f64,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
}

impl Default for SensorState {
    fn default() -> Self {
        Self {
            gpu_temp_c: 45.0,
            gpu_power_w: 30.0,
            gpu_util_pct: 0.0,
            cpu_temp_c: 50.0,
            ram_used_gb: 0.0,
            ram_total_gb: 125.0,
        }
    }
}

/// The afferent nerve — connects hardware sensors to the neural mesh.
pub struct AfferentNerve {
    orchestrator_url: String,
    client: reqwest::Client,
    cached_state: Arc<parking_lot::RwLock<SensorState>>,
}

impl AfferentNerve {
    pub fn new(orchestrator_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("failed to build HTTP client");

        Self {
            orchestrator_url: orchestrator_url.to_string(),
            client,
            cached_state: Arc::new(parking_lot::RwLock::new(SensorState::default())),
        }
    }

    /// Create with default cached state (for testing without orchestrator).
    pub fn with_state(state: SensorState) -> Self {
        Self {
            orchestrator_url: String::new(),
            client: reqwest::Client::new(),
            cached_state: Arc::new(parking_lot::RwLock::new(state)),
        }
    }

    /// Read sensor state from the orchestrator's /api/guardian/snapshot endpoint.
    pub async fn read_sensors(&self) -> SensorState {
        let url = format!("{}/api/guardian/snapshot", self.orchestrator_url);
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                    Ok(data) => {
                        let mut state = self.cached_state.write();
                        state.gpu_temp_c = data.get("gpu_temp_c")
                            .and_then(|v| v.as_f64()).unwrap_or(state.gpu_temp_c);
                        state.gpu_power_w = data.get("gpu_power_w")
                            .and_then(|v| v.as_f64()).unwrap_or(state.gpu_power_w);
                        state.gpu_util_pct = data.get("gpu_util_pct")
                            .and_then(|v| v.as_f64()).unwrap_or(state.gpu_util_pct);
                        state.clone()
                    }
                    Err(e) => {
                        debug!("Failed to parse sensor data: {}", e);
                        self.cached_state.read().clone()
                    }
                }
            }
            _ => {
                // Orchestrator not reachable — use cached state
                self.cached_state.read().clone()
            }
        }
    }

    /// Convert GPU temperature to thermal noise for the HNN.
    ///
    /// Normal: 45-70°C → noise 0.001-0.005 (barely perceptible)
    /// Warm:  70-85°C → noise 0.005-0.05 (moderate turbulence)
    /// Hot:   85°C+   → noise 0.05-0.2 (significant perturbation)
    pub async fn gpu_thermal_noise(&self) -> f64 {
        let state = self.read_sensors().await;
        let temp = state.gpu_temp_c;

        if temp < 70.0 {
            // Normal: minimal noise
            0.001 + (temp - 45.0) * 0.0001
        } else if temp < 85.0 {
            // Warm: increasing turbulence
            0.005 + (temp - 70.0) * 0.003
        } else {
            // Hot: significant perturbation (GPU stress)
            0.05 + (temp - 85.0) * 0.01
        }
    }

    /// Check if GPU is in thermal stress (>85°C).
    pub async fn is_thermal_stress(&self) -> bool {
        let state = self.read_sensors().await;
        state.gpu_temp_c > 85.0
    }

    /// Get cached state without network call.
    pub fn cached(&self) -> SensorState {
        self.cached_state.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thermal_noise_cold() {
        let nerve = AfferentNerve::with_state(SensorState {
            gpu_temp_c: 50.0, ..Default::default()
        });
        // Normal temperature → low noise
        let rt = tokio::runtime::Runtime::new().unwrap();
        let noise = rt.block_on(nerve.gpu_thermal_noise());
        assert!(noise < 0.01, "cold GPU noise should be low, got {}", noise);
    }

    #[test]
    fn thermal_noise_hot() {
        let nerve = AfferentNerve::with_state(SensorState {
            gpu_temp_c: 88.0, ..Default::default()
        });
        let rt = tokio::runtime::Runtime::new().unwrap();
        let noise = rt.block_on(nerve.gpu_thermal_noise());
        assert!(noise > 0.05, "hot GPU noise should be high, got {}", noise);
    }

    #[test]
    fn thermal_stress_detection() {
        let nerve = AfferentNerve::with_state(SensorState {
            gpu_temp_c: 90.0, ..Default::default()
        });
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(rt.block_on(nerve.is_thermal_stress()));
    }

    #[test]
    fn no_thermal_stress() {
        let nerve = AfferentNerve::with_state(SensorState {
            gpu_temp_c: 65.0, ..Default::default()
        });
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(!rt.block_on(nerve.is_thermal_stress()));
    }

    #[test]
    fn cached_state_default() {
        let state = SensorState::default();
        assert_eq!(state.gpu_temp_c, 45.0);
        assert_eq!(state.ram_total_gb, 125.0);
    }
}