use crate::neuromodulation::chemical_map::NeurochemistryProfile;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

pub struct AlgorithmicParameters {
    pub llm_temperature: AtomicU32,
    pub llm_top_p: AtomicU32,
    pub hnsw_ef_search: AtomicU64,
    pub ivf_nprobe: AtomicU64,
    pub speculative_lookahead: AtomicU32,
}

impl Default for AlgorithmicParameters {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgorithmicParameters {
    pub fn new() -> Self {
        Self {
            llm_temperature: AtomicU32::new(0.7f32.to_bits()),
            llm_top_p: AtomicU32::new(0.9f32.to_bits()),
            hnsw_ef_search: AtomicU64::new(100),
            ivf_nprobe: AtomicU64::new(10),
            speculative_lookahead: AtomicU32::new(1.5f32.to_bits()),
        }
    }

    pub fn update_parameters_inline(&self, chem: &NeurochemistryProfile) {
        let temp = 0.7 + (0.6 * chem.dopamine) - (0.3 * chem.serotonin);
        self.llm_temperature
            .store(temp.clamp(0.1, 2.5).to_bits(), Ordering::Relaxed);

        let top_p = 0.9 + (0.08 * chem.serotonin);
        self.llm_top_p
            .store(top_p.clamp(0.5, 1.0).to_bits(), Ordering::Relaxed);

        let ef = 100 + (400.0 * chem.noradrenaline) as u64;
        self.hnsw_ef_search
            .store(ef.clamp(10, 1000), Ordering::Relaxed);

        let nprobe = 10 + (60.0 * chem.noradrenaline) as u64;
        self.ivf_nprobe
            .store(nprobe.clamp(1, 128), Ordering::Relaxed);

        let lookahead = 1.5 + (3.0 * chem.dopamine);
        self.speculative_lookahead
            .store(lookahead.clamp(1.0, 8.0).to_bits(), Ordering::Relaxed);
    }

    pub fn get_temp(&self) -> f32 {
        f32::from_bits(self.llm_temperature.load(Ordering::Relaxed))
    }
}
