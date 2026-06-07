use crate::scheduler::AgentScheduler;
use crate::queue::Task;

#[no_mangle]
pub extern "C" fn soul_scheduler_init() -> *mut AgentScheduler {
    Box::into_raw(Box::new(AgentScheduler::new()))
}

#[no_mangle]
/// # Safety
/// The returned pointer is valid and must be freed with `soul_scheduler_free`.
pub unsafe extern "C" fn soul_scheduler_start(ptr: *mut AgentScheduler) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    (*ptr).launch();
    0
}

#[no_mangle]
/// # Safety
/// ptr must be a valid pointer returned by `soul_scheduler_init`.
pub unsafe extern "C" fn soul_scheduler_get_core_count(ptr: *const AgentScheduler) -> u32 {
    if ptr.is_null() {
        return 0;
    }
    (*ptr).manifest.total_logical_cores as u32
}

#[no_mangle]
/// # Safety
/// ptr must be a valid pointer returned by `soul_scheduler_init`.
pub unsafe extern "C" fn soul_scheduler_submit_task(
    ptr: *mut AgentScheduler,
    core_id: u32,
    execute_fn: extern "C" fn(*mut u8),
    context_ptr: *mut u8,
) -> i32 {
    if ptr.is_null() {
        return -1;
    }
    let task = Task {
        execute: execute_fn,
        context: context_ptr,
    };
    if (*ptr).submit_to(core_id as usize, task) {
        0
    } else {
        -2
    }
}

#[no_mangle]
/// # Safety
/// ptr must be a valid pointer returned by `soul_scheduler_init`.
pub unsafe extern "C" fn soul_scheduler_stop(ptr: *mut AgentScheduler) {
    if !ptr.is_null() {
        (*ptr).shutdown();
    }
}

#[no_mangle]
/// # Safety
/// ptr must be a valid pointer returned by `soul_scheduler_init`.
pub unsafe extern "C" fn soul_scheduler_free(ptr: *mut AgentScheduler) {
    if !ptr.is_null() {
        let _ = Box::from_raw(ptr);
    }
}
