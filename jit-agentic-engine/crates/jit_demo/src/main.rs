use jit_compiler::{Forge, JitConfig};
use dynamic_loader::SkillManager;
use anyhow::Result;

fn main() -> Result<()> {
    println!("🚀 Starting JIT Agentic Engine Integration Test...");

    // 1. Define a simple skill in Rust source code (The "AI generated" part)
    let skill_source = r#"
        use ffi_bridge::*;

        #[no_mangle]
        pub extern "C" fn skill_execute(input_ptr: *const u8, output_ptr: *mut u8, len: usize) {
            let input = unsafe { std::slice::from_raw_parts(input_ptr, len) };
            let output = unsafe { std::slice::from_raw_parts_mut(output_ptr, len) };

            // Simple logic: invert the buffer (simulating a high-perf kernel)
            for i in 0..len {
                output[i] = input[len - 1 - i];
            }
        }
    "#;

    // 2. Compile the skill using Forge
    println!("🛠️  Compiling skill...");
    let config = JitConfig::default();
    let lib_path = Forge::compile_skill(skill_source, &config)?;
    println!("✅ Skill compiled to: {:?}", lib_path);

    // 3. Load and execute the skill using DynamicLoader
    println!("🔌 Loading skill into runtime...");
    let mut manager = SkillManager::new();
    manager.swap_skill(&lib_path)?;

    // 4. Test data (Zero-copy buffers)
    let input_data = b"Hello JIT World!";
    let mut output_data = vec![0u8; input_data.len()];

    println!("🏃 Executing skill...");
    manager.execute(input_data, &mut output_data)?;

    let result_str = String::from_utf8_lossy(&output_data);
    println!("Input : {}", String::from_utf8_lossy(input_data));
    println!("Output: {}", result_str);

    if result_str == "!dlroW TIJ olleH" {
        println!("🎉 Integration Test PASSED!");
    } else {
        println!("❌ Integration Test FAILED!");
        std::process::exit(1);
    }

    Ok(())
}
