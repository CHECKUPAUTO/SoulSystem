use crate::neuromodulation::chemical_map::{NeurochemistryProfile, NeuromodulatorMapper};
use crate::neuromodulation::param_bridge::AlgorithmicParameters;

#[no_mangle]
pub unsafe extern "C" fn neural_neuromodulator_init(
    weights_ptr: *const f32,
    bias_ptr: *const f32,
) -> *mut NeuromodulatorMapper {
    let weights = std::slice::from_raw_parts(weights_ptr, 9).to_vec();
    let bias = std::slice::from_raw_parts(bias_ptr, 3).to_vec();
    Box::into_raw(Box::new(NeuromodulatorMapper::new(weights, bias)))
}

#[no_mangle]
pub unsafe extern "C" fn neural_neuromodulator_inject_override(
    ptr: *mut AlgorithmicParameters,
    da: f32,
    ne: f32,
    ser: f32,
) {
    let params = &*ptr;
    let chem = NeurochemistryProfile {
        dopamine: da,
        noradrenaline: ne,
        serotonin: ser,
    };
    params.update_parameters_inline(&chem);
}
