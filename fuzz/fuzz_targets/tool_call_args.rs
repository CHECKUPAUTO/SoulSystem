#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz tool call argument parsing (JSON args in tool calls).
// Tool call arguments can contain arbitrary JSON from the LLM.
fuzz_target!(|data: &[u8]| {
    // Parse as generic JSON — must never panic
    let _ = serde_json::from_slice::<serde_json::Value>(data);

    // Try to parse as a ToolCall — must never panic
    let _ = serde_json::from_slice::<soul_llm::ToolCall>(data);

    // Try to parse as a ToolSchema — must never panic
    let _ = serde_json::from_slice::<soul_llm::ToolSchema>(data);
});
