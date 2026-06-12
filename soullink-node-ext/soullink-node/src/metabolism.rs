//! Metabolism — Energy management, homeostasis, hibernation, and reproduction triggers.
//!
//! Correction 7: Full metabolic cycle — energy accounting per tick,
//! hibernation thresholds, passive regeneration, reproduction conditions.

use crate::brain::Brain;
use crate::brain_module::StressMode;

/// Configuration du système métabolique
#[derive(Debug, Clone)]
pub struct MetabolismConfig {
    /// Énergie consommée par neurone et par tick
    pub energy_per_neuron_per_tick: f32,
    /// Énergie consommée par synapse active
    pub energy_per_active_synapse: f32,
    /// Taux de régénération passive (fraction du pool par tick)
    pub passive_regen_rate: f32,
    /// Capacité maximale du pool énergétique
    pub max_pool_capacity: f32,
    /// Seuil critique en dessous duquel le système hiberne
    pub hibernation_threshold: f32,
    /// Seuil de reproduction (énergie suffisante pour spawner)
    pub reproduction_threshold: f32,
    /// Coût énergétique de spawner un enfant
    pub spawn_cost: f32,
    /// Multiplicateur de coût pour les opérations cortex
    pub cortex_cost_multiplier: f32,
    /// Multiplicateur de coût pour les opérations d'évolution
    pub evolution_cost_multiplier: f32,
}

impl Default for MetabolismConfig {
    fn default() -> Self {
        Self {
            energy_per_neuron_per_tick: 0.0001,
            energy_per_active_synapse: 0.00005,
            passive_regen_rate: 0.001,
            max_pool_capacity: 100.0,
            hibernation_threshold: 10.0,
            reproduction_threshold: 70.0,
            spawn_cost: 40.0,
            cortex_cost_multiplier: 4.0,
            evolution_cost_multiplier: 2.0,
        }
    }
}

/// Comptabilité métabolique par tick
#[derive(Debug, Clone)]
pub struct MetabolismLedger {
    pub energy_consumed: f32,
    pub energy_regenerated: f32,
    pub neuron_cost: f32,
    pub synapse_cost: f32,
    pub cortex_cost: f32,
    pub evolution_cost: f32,
    pub net_change: f32,
}

impl Default for MetabolismLedger {
    fn default() -> Self {
        Self {
            energy_consumed: 0.0,
            energy_regenerated: 0.0,
            neuron_cost: 0.0,
            synapse_cost: 0.0,
            cortex_cost: 0.0,
            evolution_cost: 0.0,
            net_change: 0.0,
        }
    }
}

impl Brain {
    /// Calcule et applique le métabolisme pour un tick.
    /// Retourne un ledger détaillant chaque composante énergétique.
    pub fn metabolize(&mut self, config: &MetabolismConfig) -> MetabolismLedger {
        let mut ledger = MetabolismLedger::default();

        // 1. Coût des neurones
        ledger.neuron_cost = self.neurons.len() as f32 * config.energy_per_neuron_per_tick;
        ledger.energy_consumed += ledger.neuron_cost;

        // 2. Coût des synapses actives
        let active_synapses = self
            .synapses
            .iter()
            .filter(|s| s.weight.abs() > 0.01)
            .count();
        ledger.synapse_cost = active_synapses as f32 * config.energy_per_active_synapse;
        ledger.energy_consumed += ledger.synapse_cost;

        // 3. Coût des cortex (si actifs)
        if self.cortex.config.enabled {
            ledger.cortex_cost += 0.05 * config.cortex_cost_multiplier;
        }
        if self.deepseek_cortex.config.enabled {
            ledger.cortex_cost += 0.08 * config.cortex_cost_multiplier;
        }
        if self.meta_cortex.correction_score > 0.0 {
            ledger.cortex_cost += 0.02 * config.cortex_cost_multiplier;
        }
        ledger.energy_consumed += ledger.cortex_cost;

        // 4. Coût d'évolution (amorti sur le cycle)
        ledger.evolution_cost = 0.01
            * config.evolution_cost_multiplier
            * self.evolution.generation as f32
            / 100.0;
        ledger.energy_consumed += ledger.evolution_cost;

        // 5. Régénération passive
        ledger.energy_regenerated = self.energy_pool_capacity
            * config.passive_regen_rate
            * (1.0 - self.metabolic_pressure * 0.5);

        // 6. Appliquer
        self.energy_pool = (self.energy_pool - ledger.energy_consumed
            + ledger.energy_regenerated)
            .clamp(0.0, self.energy_pool_capacity);

        ledger.net_change = ledger.energy_regenerated - ledger.energy_consumed;

        // 7. Mise à jour de la pression métabolique
        self.metabolic_pressure =
            (1.0 - self.energy_pool / self.energy_pool_capacity).clamp(0.0, 1.0);

        ledger
    }

    /// Vérifie si les conditions sont réunies pour la reproduction.
    pub fn can_reproduce(&self, config: &MetabolismConfig) -> bool {
        self.energy_pool >= config.reproduction_threshold
            && self.evolution.best_fitness > 0.6
            && self.stress_mode != StressMode::Panic
    }

    /// Vérifie si le système doit hiberner.
    pub fn should_hibernate(&self, config: &MetabolismConfig) -> bool {
        self.energy_pool < config.hibernation_threshold
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::Brain;

    #[test]
    fn test_metabolize_basic() {
        let mut brain = Brain::new("meta_test", 9990);
        let config = MetabolismConfig::default();

        let initial_energy = brain.energy_pool;
        let ledger = brain.metabolize(&config);

        // Energy should decrease (consumption > passive regen for 33k neurons)
        assert!(ledger.energy_consumed > 0.0, "Should consume energy");
        assert!(ledger.energy_regenerated > 0.0, "Should regenerate some energy");
        assert!(
            brain.energy_pool >= 0.0 && brain.energy_pool <= brain.energy_pool_capacity,
            "Energy pool must stay within bounds"
        );
    }

    #[test]
    fn test_hibernation_threshold() {
        let mut brain = Brain::new("hib_test", 9991);
        let config = MetabolismConfig::default();

        // Artificially deplete energy
        brain.energy_pool = 5.0;
        assert!(brain.should_hibernate(&config), "Should hibernate below threshold");

        brain.energy_pool = 50.0;
        assert!(!brain.should_hibernate(&config), "Should not hibernate above threshold");
    }

    #[test]
    fn test_reproduction_energy_cost() {
        let mut brain = Brain::new("rep_test", 9992);
        let config = MetabolismConfig::default();

        // Not enough energy
        brain.energy_pool = 30.0;
        brain.evolution.best_fitness = 0.8;
        assert!(!brain.can_reproduce(&config), "Should not reproduce with low energy");

        // Enough energy
        brain.energy_pool = 80.0;
        assert!(brain.can_reproduce(&config), "Should be able to reproduce");
    }

    #[test]
    fn test_passive_regen() {
        let mut brain = Brain::new("regen_test", 9993);
        let config = MetabolismConfig::default();

        brain.energy_pool = 20.0;
        let ledger = brain.metabolize(&config);

        // Passive regen should contribute positively
        assert!(ledger.energy_regenerated > 0.0, "Passive regen must be positive");
    }

    #[test]
    fn test_metabolic_pressure_update() {
        let mut brain = Brain::new("press_test", 9994);
        let config = MetabolismConfig::default();

        brain.energy_pool = 10.0;
        brain.metabolize(&config);

        // Low energy → high metabolic pressure
        assert!(brain.metabolic_pressure > 0.5, "Low energy should increase metabolic pressure");
    }
}
