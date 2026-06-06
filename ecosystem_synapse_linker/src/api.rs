use crate::linker::agent::{SynapticLinkerAgent, SynapseRoute};

#[no_mangle] pub extern "C" fn synapse_linker_init() -> *mut SynapticLinkerAgent {
    let linker = Box::new(SynapticLinkerAgent::new()); Box::into_raw(linker)
}
#[no_mangle] pub unsafe extern "C" fn synapse_linker_free(ptr: *mut SynapticLinkerAgent) {
    if !ptr.is_null() { let _ = Box::from_raw(ptr); }
}
#[no_mangle] pub unsafe extern "C" fn synapse_linker_register_route(ptr: *mut SynapticLinkerAgent, source: u64, target: u64, weight: f32, is_active: i32) -> i32 {
    if ptr.is_null() { return -1; } let linker = &*ptr; linker.update_synapse_route(source, target, weight, is_active == 1); 0
}
#[no_mangle] pub unsafe extern "C" fn synapse_linker_resolve_weight(ptr: *const SynapticLinkerAgent, source: u64, target: u64) -> f32 {
    if ptr.is_null() { return -1.0; } let linker = &*ptr; match linker.resolve_routing_weight(source, target) { Some(w) => w, None => -1.0 }
}
#[no_mangle] pub unsafe extern "C" fn synapse_linker_count(ptr: *const SynapticLinkerAgent) -> usize {
    if ptr.is_null() { return 0; } let linker = &*ptr; linker.get_total_synapse_count()
}
