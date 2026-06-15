#![no_main]

use libfuzzer_sys::fuzz_target;

/// Fuzz the LLM response deserialization to ensure no panics on malformed JSON.
/// Covers: ChatResponse, AssistantMessage
fuzz_target!(|data: &[u8]| {
    // Try to parse as each response type — must never panic
    let _ = serde_json::from_slice::<soul_llm::ChatResponse>(data);
    let _ = serde_json::from_slice::<soul_llm::AssistantMessage>(data);
});
