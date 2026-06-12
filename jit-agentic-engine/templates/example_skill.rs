use ffi_bridge::*;

#[no_mangle]
pub extern "C" fn skill_execute(input_ptr: *const u8, output_ptr: *mut u8, len: usize) {
    let input = unsafe { std::slice::from_raw_parts(input_ptr, len) };
    let output = unsafe { std::slice::from_raw_parts_mut(output_ptr, len) };

    for i in 0..len {
        output[i] = input[len - 1 - i];
    }
}
