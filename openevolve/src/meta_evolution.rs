//! Meta-evolution: evolve hyperparameters by adjusting config values.

use crate::config::EvolutionConfig;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct MetaIndividual {
    pub config: EvolutionConfig,
    pub score: f64,
}

impl MetaIndividual {
    pub fn mutate(&mut self) {
        let mut rng = rand::thread_rng();
        // In v4, mutate available config fields
        self.config.max_iterations =
            (self.config.max_iterations as i64 + rng.gen_range(-20..=20)).clamp(10, 500) as u32;
        self.config.quick_iterations =
            (self.config.quick_iterations as i64 + rng.gen_range(-5..=5)).clamp(5, 200) as u32;
        self.config.checkpoint_interval =
            (self.config.checkpoint_interval as i64 + rng.gen_range(-3..=3)).clamp(1, 100) as u32;
    }
}
