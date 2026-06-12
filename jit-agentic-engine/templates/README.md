# JIT Skills Templates

This directory contains guides for AI agents (like OpenClaw) to generate compatible high-performance Rust skills.

## Generation Rules
1. **Crate Type**: Must be `cdylib`.
2. **Entry Point**: Function must be named `skill_execute`.
3. **ABI**: Use `#[no_mangle]` and `pub extern "C"`.
4. **Zero-Copy**: Operate directly on pointers: `*const u8` (input) and `*mut u8` (output).

## Implementation Guide
Always use the following pattern to ensure safety and performance:
```rust
use ffi_bridge::*;

#[no_mangle]
pub extern "C" fn skill_execute(input_ptr: *const u8, output_ptr: *mut u8, len: usize) {
    let input = unsafe { std::slice::from_raw_parts(input_ptr, len) };
    let output = unsafe { std::slice::from_raw_parts_mut(output_ptr, len) };

    // Your optimized logic here
}
```
