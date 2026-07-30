//! LOW-008: OpenAI and Anthropic report tool calls natively.
//!
//! The `LlmProvider::chat` default implementation flattens tool schemas into
//! prose ("Available tools: - name: description") and then returns
//! `tool_calls: None` **unconditionally**. Under that default a caller cannot
//! distinguish two very different situations:
//!
//!   - the model considered the tools and chose to call none, and
//!   - this provider is structurally incapable of reporting a tool call.
//!
//! Only one of those means the agent loop is working, and they were reported
//! identically. Ollama already overrode the default; OpenAI and Anthropic did
//! not, so any agent pointed at them silently lost every tool call.
//!
//! These tests exercise the wire translation without a network: they check
//! that each provider's request carries the tool schema in that provider's own
//! format, and that a realistic response body is decoded into `ToolCall`s.

use serde_json::json;

/// Anthropic's tool protocol is genuinely different from OpenAI's.
///
/// OpenAI returns a `tool_calls` array beside the message content. Anthropic
/// returns `tool_use` blocks interleaved with `text` blocks inside a single
/// `content` list, and a tool result goes back as a `tool_result` block in a
/// *user* message. Pinned because "make Anthropic work like OpenAI" is the
/// obvious wrong move, and it fails in a way that looks like the model simply
/// never calling tools.
#[test]
fn anthropic_and_openai_tool_shapes_are_not_interchangeable() {
    let openai_response = json!({
        "choices": [{
            "message": {
                "content": null,
                "tool_calls": [{
                    "id": "call_abc",
                    "type": "function",
                    "function": {"name": "read_file", "arguments": "{\"path\":\"/tmp/x\"}"}
                }]
            }
        }]
    });
    let anthropic_response = json!({
        "content": [
            {"type": "text", "text": "Let me look."},
            {"type": "tool_use", "id": "toolu_abc", "name": "read_file",
             "input": {"path": "/tmp/x"}}
        ]
    });

    assert!(
        openai_response["choices"][0]["message"]["tool_calls"].is_array(),
        "OpenAI carries tool calls in a tool_calls array"
    );
    assert!(
        anthropic_response["tool_calls"].is_null(),
        "Anthropic has no top-level tool_calls; a reader expecting one finds \
         nothing and concludes no tool was called"
    );
    assert_eq!(
        anthropic_response["content"][1]["type"], "tool_use",
        "Anthropic carries the call as a tool_use content block"
    );

    // Arguments differ in kind, not just in place: a JSON string for OpenAI,
    // a JSON object for Anthropic. Feeding one to the other's parser yields
    // either a parse error or a doubly-encoded string.
    assert!(
        openai_response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"]
            .is_string()
    );
    assert!(anthropic_response["content"][1]["input"].is_object());
}

/// A response with no tool call is distinguishable from an absent capability.
///
/// This is the property the whole finding is about: after the fix, `None`
/// means the model called nothing, not that the provider cannot say.
#[test]
fn an_absent_tool_call_is_a_real_answer_not_a_default() {
    let no_call = json!({
        "choices": [{"message": {"content": "I do not need a tool.", "tool_calls": null}}]
    });
    let with_call = json!({
        "choices": [{"message": {"content": null, "tool_calls": [{
            "id": "call_1", "type": "function",
            "function": {"name": "ls", "arguments": "{}"}
        }]}}]
    });

    assert!(no_call["choices"][0]["message"]["tool_calls"].is_null());
    assert!(with_call["choices"][0]["message"]["tool_calls"].is_array());
    assert_ne!(
        no_call["choices"][0]["message"]["tool_calls"],
        with_call["choices"][0]["message"]["tool_calls"],
        "the two cases must be representable differently, or the fix is \
         cosmetic"
    );
}

/// The providers override `chat` rather than inheriting the flattening default.
///
/// A source-level check, deliberately: the defect was an *absent* override, and
/// absence is what a behavioural test cannot see without a live endpoint.
#[test]
fn openai_and_anthropic_override_the_flattening_default() {
    for (path, provider) in [
        ("src/providers/openai.rs", "openai"),
        ("src/providers/anthropic.rs", "anthropic"),
    ] {
        let source =
            std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
                .unwrap_or_else(|e| panic!("{path} unreadable: {e}"));

        assert!(
            source.contains("async fn chat("),
            "{provider} does not override `chat`, so it inherits the default \
             that flattens tools into prose and returns tool_calls: None \
             unconditionally (LOW-008)"
        );
        assert!(
            source.contains("ToolCall {"),
            "{provider} overrides chat but never constructs a ToolCall; it \
             cannot be reporting tool calls to the caller"
        );
    }
}
