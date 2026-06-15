// Simple RNG module for scirust-core
// Wraps SmallRng in a newtype for deterministic reproducibility.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

pub struct PcgEngine(SmallRng);

impl PcgEngine {
    pub fn new(seed: u64) -> Self {
        PcgEngine(SmallRng::seed_from_u64(seed))
    }

    pub fn float(&mut self) -> f32 {
        self.0.random::<f32>()
    }
}
