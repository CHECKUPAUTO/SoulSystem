## 2025-05-15 - JIT Compilation Speed vs. Runtime Efficiency
**Learning:** In JIT-heavy applications where code is compiled at runtime, `lto=thin` offers a better trade-off than `lto=fat`. While `lto=fat` can provide slightly better execution performance, the significantly longer link times negatively impact the "startup" or "swap" time of JIT skills. Additionally, `panic=abort` is essential for FFI-based JIT to avoid unwinding overhead and potential undefined behavior.
**Action:** Default to `lto=thin` and `panic=abort` for JIT compilation flags to optimize the end-to-end responsiveness of the agent.

## 2025-05-15 - FFI Path Resolution in JIT
**Learning:** When generating a `Cargo.toml` for JIT compilation from a temporary directory, absolute paths to local dependencies (like `ffi_bridge`) must be correctly resolved relative to the actual workspace root, not hardcoded to outdated directory structures.
**Action:** Use `std::env::current_dir()` combined with the correct relative path from the root to find local crates during JIT project generation.
