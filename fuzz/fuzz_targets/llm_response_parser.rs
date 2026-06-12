#![no_main]

use libfuzzer_sys::fuzz_target;

/// Fuzz the LLM response deserialization to ensure no panics on malformed JSON.
/// Covers: ChatResponse, StreamChunk, EmbedResponse, ModelsResponse, GenerateResponse
fuzz_target!(|data: &[u8]| {
    // Try to parse as each response type — must never panic
    let _ = serde_json::from_slice::<soul_llm::ChatResponse>(data);
    let _ = serde_json::from_slice::<soul_llm::StreamChunk>(data);
    let _ = serde_json::from_slice::<soul_llm::EmbedResponse>(data);
    let _ = serde_json::from_slice::<soul_llm::ModelsResponse>(data);
    let _ = serde_json::from_slice::<soul_llm::GenerateResponse>(data);
});
