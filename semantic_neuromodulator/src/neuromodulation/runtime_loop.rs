use std::sync::Arc;
use std::thread;
use std::time::Duration;
use crate::neuromodulation::chemical_map::{NeuromodulatorMapper, NeurochemistryProfile};
use crate::neuromodulation::param_bridge::AlgorithmicParameters;
use scirust_affective_core::AffectiveState;

pub struct NeuromodulatorDaemon {
    pub state: Arc<AffectiveState>,
    pub mapper: Arc<NeuromodulatorMapper>,
    pub params: Arc<AlgorithmicParameters>,
}

impl NeuromodulatorDaemon {
    pub fn spawn_sync_thread(self: Arc<Self>) {
        thread::spawn(move || {
            core_affinity::set_for_current(core_affinity::CoreId { id: 2 });
            loop {
                let pad = &self.state.get_coordinates();
                let pad_tensor = scirust::Tensor::from_slice(&pad);
                let chemistry = self.mapper.compute_chemical_levels(&pad_tensor);
                self.params.update_parameters_inline(&chemistry);
                thread::sleep(Duration::from_millis(20));
            }
        });
    }
}
