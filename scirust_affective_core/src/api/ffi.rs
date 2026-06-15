use crate::affect::autograd_hook::EmotionalAutogradHook;
use crate::affect::drives::DriveRegistry;
use crate::affect::space::AffectiveState;
use parking_lot::Mutex;
use std::sync::Arc;

#[allow(static_mut_refs)]
static mut GLOBAL_AFFECTIVE_STATE: Option<Arc<AffectiveState>> = None;
#[allow(static_mut_refs)]
static mut GLOBAL_DRIVE_REGISTRY: Option<Arc<Mutex<DriveRegistry>>> = None;
#[allow(static_mut_refs)]
static mut GLOBAL_HOOK: Option<EmotionalAutogradHook> = None;

/// # Safety
/// Safe because it initializes global state once. Must be called before other FFI functions.
#[no_mangle]
pub unsafe extern "C" fn affective_core_init() {
    GLOBAL_AFFECTIVE_STATE = Some(Arc::new(AffectiveState::new()));
    GLOBAL_DRIVE_REGISTRY = Some(Arc::new(Mutex::new(DriveRegistry::new_instantiated())));
    GLOBAL_HOOK = Some(EmotionalAutogradHook::new(0.5));
}

/// # Safety
/// Safe because GLOBAL_AFFECTIVE_STATE is set by affective_core_init first.
#[no_mangle]
pub unsafe extern "C" fn affective_core_inject_stimulus(val_p: f32, val_a: f32, val_d: f32) {
    if let Some(ref state) = GLOBAL_AFFECTIVE_STATE {
        let stim = crate::affect::space::Tensor::new_vector(vec![val_p, val_a, val_d]);
        (**state).apply_stimulus(&stim);
    }
}

/// # Safety
/// Safe because out_ptr must point to a valid buffer of at least 3 f32 values.
#[no_mangle]
pub unsafe extern "C" fn affective_core_get_current_state(out_ptr: *mut f32) {
    if let Some(ref state) = GLOBAL_AFFECTIVE_STATE {
        let coords = state.get_coordinates();
        std::ptr::copy_nonoverlapping(coords.as_ptr(), out_ptr, 3);
    }
}

/// # Safety
/// Safe because out_ptr must point to a valid f32 value.
#[no_mangle]
pub unsafe extern "C" fn affective_core_compute_gate(out_ptr: *mut f32) {
    // Use raw pointer access to avoid creating shared refs to mutable statics
    // SAFETY: We're reading the static through a raw pointer without creating a reference
    let state_opt: Option<Arc<AffectiveState>> =
        std::ptr::read_volatile(&raw const GLOBAL_AFFECTIVE_STATE);
    let reg_opt: Option<Arc<Mutex<DriveRegistry>>> =
        std::ptr::read_volatile(&raw const GLOBAL_DRIVE_REGISTRY);
    let hook_opt: Option<EmotionalAutogradHook> = std::ptr::read_volatile(&raw const GLOBAL_HOOK);

    let state_ptr: *const AffectiveState = state_opt
        .map(|x| x.as_ref() as *const _)
        .unwrap_or(std::ptr::null());
    let reg_ptr: *const Mutex<DriveRegistry> = reg_opt
        .map(|x| x.as_ref() as *const _)
        .unwrap_or(std::ptr::null());
    let hook_ptr: *const EmotionalAutogradHook = hook_opt
        .as_ref()
        .map(|x| x as *const _)
        .unwrap_or(std::ptr::null());

    if !state_ptr.is_null() && !reg_ptr.is_null() && !hook_ptr.is_null() {
        let state = &*state_ptr;
        let reg_lock = &*reg_ptr;
        let hook = &*hook_ptr;

        let mut graph = scirust::autodiff::reverse::Tape::new();
        let grads = (*hook).backpropagate_emotional_tension(&mut graph, &reg_lock.lock(), state);
        let gate = (*hook).compute_weight_gate(&grads);
        *out_ptr = gate;
    }
}
