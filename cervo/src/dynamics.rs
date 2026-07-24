use crate::config::DynamicsConfig;

pub fn apply_attraction(rsi: &mut f64, entropy: f64, saturation: f64, config: &DynamicsConfig) {
    let damping = (1.0 - saturation * 0.5).max(0.1);
    let force = (config.base_force + entropy * config.entropy_force_factor) * damping;
    *rsi += force;
    *rsi = rsi.clamp(0.0, 100.0);
}

pub fn apply_repulsion(rsi: &mut f64, entropy: f64, saturation: f64, config: &DynamicsConfig) {
    let damping = (1.0 - saturation * 0.5).max(0.1);
    let force = (config.base_force + entropy * config.entropy_force_factor) * damping;
    *rsi -= force;
    *rsi = rsi.clamp(0.0, 100.0);
}

pub fn apply_entropy(rsi: &mut f64, entropy: &mut f64, config: &DynamicsConfig) {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let perturbation: f64 = rng.gen_range(-1.0..1.0);
    *rsi += perturbation * *entropy * 3.0;
    *rsi = rsi.clamp(0.0, 100.0);
    *entropy *= config.entropy_decay;
    if *entropy < 0.05 {
        *entropy = config.entropy_reset;
    }
}

pub fn cycle_step(
    rsi: &mut f64,
    entropy: &mut f64,
    saturation: f64,
    config: &DynamicsConfig,
) -> &'static str {
    if *rsi < config.attraction_threshold {
        apply_attraction(rsi, *entropy, saturation, config);
        *entropy = (*entropy + 0.15).min(1.0);
        "attraction"
    } else if *rsi > config.repulsion_threshold {
        apply_repulsion(rsi, *entropy, saturation, config);
        *entropy = (*entropy + 0.15).min(1.0);
        "repulsion"
    } else {
        apply_entropy(rsi, entropy, config);
        "entropy"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DynamicsConfig;

    fn config() -> DynamicsConfig {
        DynamicsConfig::default()
    }

    #[test]
    fn test_attraction_lowers_rsi() {
        let mut rsi = 10.0;
        let mut entropy = 0.5;
        let phase = cycle_step(&mut rsi, &mut entropy, 0.0, &config());
        assert_eq!(phase, "attraction");
        assert!(rsi > 10.0);
    }

    #[test]
    fn test_repulsion_raises_rsi() {
        let mut rsi = 80.0;
        let mut entropy = 0.5;
        let phase = cycle_step(&mut rsi, &mut entropy, 0.0, &config());
        assert_eq!(phase, "repulsion");
        assert!(rsi < 80.0);
    }

    #[test]
    fn test_entropy_bounds() {
        let mut rsi = 50.0;
        let mut entropy = 0.5;
        let phase = cycle_step(&mut rsi, &mut entropy, 0.0, &config());
        assert_eq!(phase, "entropy");
        assert!((0.0..=100.0).contains(&rsi));
    }

    #[test]
    fn test_attraction_clamps() {
        let mut rsi = 0.0;
        apply_attraction(&mut rsi, 1.0, 0.0, &config());
        assert!((0.0..=100.0).contains(&rsi));
    }

    #[test]
    fn test_repulsion_clamps() {
        let mut rsi = 100.0;
        apply_repulsion(&mut rsi, 1.0, 0.0, &config());
        assert!((0.0..=100.0).contains(&rsi));
    }

    #[test]
    fn test_saturation_dampens_attraction() {
        let mut rsi_low = 10.0;
        let mut rsi_high = 10.0;
        apply_attraction(&mut rsi_low, 0.5, 0.0, &config());
        apply_attraction(&mut rsi_high, 0.5, 0.9, &config());
        assert!(rsi_low > rsi_high);
    }

    #[test]
    fn test_entropy_decay_and_reset() {
        let mut rsi = 50.0;
        let mut entropy = 0.03;
        apply_entropy(&mut rsi, &mut entropy, &config());
        assert!(entropy > 0.03);
    }

    #[test]
    fn test_cycle_step_all_phases() {
        let cfg = config();
        let mut rsi = 10.0;
        let mut entropy = 0.5;
        assert_eq!(cycle_step(&mut rsi, &mut entropy, 0.0, &cfg), "attraction");

        rsi = 50.0;
        assert_eq!(cycle_step(&mut rsi, &mut entropy, 0.0, &cfg), "entropy");

        rsi = 90.0;
        assert_eq!(cycle_step(&mut rsi, &mut entropy, 0.0, &cfg), "repulsion");
    }
}
