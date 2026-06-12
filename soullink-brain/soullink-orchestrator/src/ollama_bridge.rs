//! Ollama bridge — non-streaming /api/generate client.
//!
//! Phase 6a-hybrid: one request, one response, block until done. Streaming
//! comes in Phase 6b (coordinated with the gateway's message-edit path).
//!
//! # Config
//!
//! The `OllamaConfig` is loaded from `/etc/soullink/orchestrator.toml` via
//! `config::load_default()`. If the `[ollama]` section is absent, the
//! bridge is disabled and the orchestrator falls back to synth_fallback.
//!
//! # Why a dedicated HTTP client (not the shared one)
//!
//! `soullink-core::http::shared_client` is tuned for the guardian/orch
//! fan-out path: small payloads, 10 s request timeout, 3 s connect
//! timeout. Ollama cloud calls are the opposite: larger payloads, p99
//! latency up to 30 s on cold start. Using the shared client killed
//! requests at 10 s regardless of per-request `.timeout()` overrides in
//! reqwest 0.12 (the connect_timeout + overall timeout interaction is
//! not what we want here).
//!
//! So we build a tiny dedicated client sized for this workload, cached
//! via `OnceLock` so we still pay build cost only once.

use std::{sync::OnceLock, time::Duration};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::mesh_context::MeshContext;
use soullink_memory::MemoryGraph;
use std::sync::Arc;

/// Lazily-initialised dedicated client for Ollama. Separate from the
/// core shared client because Ollama has a very different latency
/// profile (tens of seconds vs milliseconds).
static OLLAMA_HTTP: OnceLock<Client> = OnceLock::new();

/// Build (or retrieve) the Ollama-specific HTTP client. Its per-request
/// timeout is `None` so the caller's `.timeout(cfg.timeout)` is what
/// actually governs the wall-clock bound.
fn ollama_client() -> &'static Client {
    OLLAMA_HTTP.get_or_init(|| {
        Client::builder()
            .pool_max_idle_per_host(4)
            .pool_idle_timeout(Duration::from_secs(120))
            .tcp_keepalive(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            // Intentionally NO .timeout(...) here — per-request override
            // governs. Ollama cloud responses can legitimately take 30s+.
            .user_agent(concat!("soullink-orchestrator/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("ollama http client build failed")
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(with = "crate::config::duration_secs", default = "default_timeout")]
    pub timeout: Duration,
    /// Optional: max prompt tokens to send. Defers to server default if
    /// None. (Relevant once we start attaching long mesh contexts.)
    #[serde(default)]
    pub num_ctx: Option<u32>,
    /// Optional: lower bound on "be concise" — system-level instruction
    /// prepended to every prompt.
    #[serde(default = "default_system")]
    pub system: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            model: default_model(),
            timeout: default_timeout(),
            num_ctx: None,
            system: default_system(),
        }
    }
}

fn default_base_url() -> String {
    "http://127.0.0.1:11434".into()
}
fn default_model() -> String {
    "qwen2.5-coder:3b".into()
}
fn default_timeout() -> Duration {
    Duration::from_secs(60)
}
fn default_system() -> String {
    "You are SoulLink, an AI interface to a mesh of specialized neural modules (brains). \
     Each user message comes with live telemetry from the mesh (attractor state, \
     turbulence, top concepts). Use this telemetry to ground your reply. Be concise \
     (2-4 short paragraphs, no lists unless asked). Never invent concepts not in the \
     telemetry."
        .into()
}

#[derive(Debug, Error)]
pub enum OllamaError {
    #[error("ollama disabled (no [ollama] section in config)")]
    #[allow(dead_code)] // reserved for future use when config is dynamic
    Disabled,
    #[error("http error: {0}")]
    Http(String),
    #[error("ollama returned empty response")]
    EmptyResponse,
}

#[derive(Debug, Serialize)]
struct GenerateReq<'a> {
    model: &'a str,
    prompt: &'a str,
    system: &'a str,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<GenerateOptions>,
}

#[derive(Debug, Serialize)]
struct GenerateOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GenerateResp {
    #[serde(default)]
    response: String,
    #[serde(default)]
    done: bool,
}

/// Compose the user prompt: the user's query followed by mesh telemetry.
/// Kept as a pub fn so tests and benches can exercise the formatting
/// without an HTTP round-trip.
pub fn compose_prompt(ctx: &MeshContext) -> String {
    format!(
        "User query: {}\n\n{}\n\n\
         Reply to the user in a way that's informed by (but doesn't verbatim quote) the telemetry above.",
        ctx.query,
        ctx.format_for_prompt(),
    )
}

#[derive(Debug, Serialize)]
struct EmbedReq<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(Debug, Deserialize)]
struct EmbedResp {
    embedding: Vec<f32>,
}

/// Fetch an embedding vector from Ollama's `/api/embeddings` endpoint.
async fn embed_query(cfg: &OllamaConfig, query: &str) -> Result<Vec<f32>, OllamaError> {
    let url = format!("{}/api/embeddings", cfg.base_url.trim_end_matches('/'));
    let body = EmbedReq {
        model: &cfg.model,
        prompt: query,
    };
    let client = ollama_client();
    let resp = client
        .post(&url)
        .timeout(cfg.timeout)
        .json(&body)
        .send()
        .await
        .map_err(|e| OllamaError::Http(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(OllamaError::Http(format!("HTTP {status}: {text}")));
    }

    let parsed: EmbedResp = resp
        .json()
        .await
        .map_err(|e| OllamaError::Http(format!("JSON decode: {e}")))?;

    Ok(parsed.embedding)
}

/// Generate with optional RAG enrichment from the memory graph.
/// If memory is provided and an embedding can be obtained, relevant concepts
/// are prepended to the prompt as `[Memory Context]`.
pub async fn generate_with_memory(
    cfg: &OllamaConfig,
    ctx: &MeshContext,
    memory: Option<&Arc<MemoryGraph>>,
    top_k: usize,
) -> Result<String, OllamaError> {
    let mut enriched_ctx = ctx.clone();

    if let Some(mg) = memory {
        if let Ok(embedding) = embed_query(cfg, &ctx.query).await {
            let results = mg.search(&embedding, top_k);
            if !results.is_empty() {
                let memories: Vec<String> = results
                    .iter()
                    .filter_map(|r| {
                        mg.get(&r.label)
                            .map(|c| format!("{}: {:.2}", r.label, r.score))
                    })
                    .collect();
                if !memories.is_empty() {
                    enriched_ctx.query = format!(
                        "[Memory Context]\n{}\n[/Memory Context]\n\n{}",
                        memories.join("\n"),
                        ctx.query
                    );
                }
            }
        }
    }

    generate(cfg, &enriched_ctx).await
}

pub async fn generate(cfg: &OllamaConfig, ctx: &MeshContext) -> Result<String, OllamaError> {
    let url = format!("{}/api/generate", cfg.base_url.trim_end_matches('/'));
    let prompt = compose_prompt(ctx);
    debug!(
        url = %url,
        model = %cfg.model,
        prompt_len = prompt.len(),
        "ollama → generate"
    );
    let body = GenerateReq {
        model: &cfg.model,
        prompt: &prompt,
        system: &cfg.system,
        stream: false,
        options: if cfg.num_ctx.is_some() {
            Some(GenerateOptions {
                num_ctx: cfg.num_ctx,
            })
        } else {
            None
        },
    };

    let client = ollama_client();
    let resp = client
        .post(&url)
        .timeout(cfg.timeout)
        .json(&body)
        .send()
        .await
        .map_err(|e| OllamaError::Http(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(OllamaError::Http(format!("HTTP {status}: {text}")));
    }

    let parsed: GenerateResp = resp
        .json()
        .await
        .map_err(|e| OllamaError::Http(format!("JSON decode: {e}")))?;

    if !parsed.done {
        warn!("ollama returned done=false on non-stream request");
    }
    let reply = parsed.response.trim().to_string();
    if reply.is_empty() {
        return Err(OllamaError::EmptyResponse);
    }
    info!(len = reply.len(), "ollama → reply");
    Ok(reply)
}

/// Stream Ollama's response as it's generated. Yields each `response`
/// fragment as Ollama emits it. The last item is always followed by an
/// Ollama frame with `done: true` — at that point the stream ends cleanly.
///
/// # Wire format
///
/// Ollama's `/api/generate` with `stream: true` returns one JSON object
/// per line (NDJSON, not SSE). Each object has `{response: "...", done: bool}`.
/// We consume the response bytes line by line and emit each `response`
/// fragment to the caller.
///
/// # Cancellation
///
/// Dropping the returned stream cancels the underlying HTTP request (reqwest
/// body is tied to the stream's lifetime). The gateway uses this when the
/// user interrupts or the bucket rate-limiter decides to stop editing.
/// Stream with optional RAG enrichment from the memory graph.
pub async fn generate_stream_with_memory(
    cfg: &OllamaConfig,
    ctx: &MeshContext,
    memory: Option<&Arc<MemoryGraph>>,
    top_k: usize,
) -> Result<impl futures::Stream<Item = Result<String, OllamaError>>, OllamaError> {
    let mut enriched_ctx = ctx.clone();
    if let Some(mg) = memory {
        if let Ok(embedding) = embed_query(cfg, &ctx.query).await {
            let results = mg.search(&embedding, top_k);
            if !results.is_empty() {
                let memories: Vec<String> = results
                    .iter()
                    .filter_map(|r| {
                        mg.get(&r.label)
                            .map(|c| format!("{}: {:.2}", r.label, r.score))
                    })
                    .collect();
                if !memories.is_empty() {
                    enriched_ctx.query = format!(
                        "[Memory Context]\n{}\n[/Memory Context]\n\n{}",
                        memories.join("\n"),
                        ctx.query
                    );
                }
            }
        }
    }
    generate_stream(cfg, &enriched_ctx).await
}

pub async fn generate_stream(
    cfg: &OllamaConfig,
    ctx: &MeshContext,
) -> Result<impl futures::Stream<Item = Result<String, OllamaError>>, OllamaError> {
    use futures::StreamExt;

    let url = format!("{}/api/generate", cfg.base_url.trim_end_matches('/'));
    let prompt = compose_prompt(ctx);
    debug!(url = %url, model = %cfg.model, "ollama → generate_stream");

    let body = GenerateReq {
        model: &cfg.model,
        prompt: &prompt,
        system: &cfg.system,
        stream: true,
        options: if cfg.num_ctx.is_some() {
            Some(GenerateOptions {
                num_ctx: cfg.num_ctx,
            })
        } else {
            None
        },
    };

    let client = ollama_client();
    let resp = client
        .post(&url)
        .timeout(cfg.timeout)
        .json(&body)
        .send()
        .await
        .map_err(|e| OllamaError::Http(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(OllamaError::Http(format!("HTTP {status}: {text}")));
    }

    // reqwest gives us a stream of Bytes chunks — we need to split them
    // into NDJSON lines. A chunk may contain multiple newlines or split a
    // line across boundaries; the fold below handles both.
    let byte_stream = resp.bytes_stream();

    let line_stream = byte_stream
        .scan(Vec::<u8>::new(), |buf: &mut Vec<u8>, chunk| {
            let chunk = match chunk {
                Ok(b) => b,
                Err(e) => {
                    return futures::future::ready(Some(vec![Err(OllamaError::Http(format!(
                        "stream read: {e}"
                    )))]))
                }
            };
            buf.extend_from_slice(&chunk);

            // Drain complete lines from the buffer
            let mut lines: Vec<Result<String, OllamaError>> = Vec::new();
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line_str = String::from_utf8_lossy(&line[..line.len() - 1]); // drop \n
                let line_trimmed = line_str.trim();
                if line_trimmed.is_empty() {
                    continue;
                }

                match serde_json::from_str::<GenerateResp>(line_trimmed) {
                    Ok(parsed) => {
                        if !parsed.response.is_empty() {
                            lines.push(Ok(parsed.response));
                        }
                        // `done: true` frames have empty response — we
                        // just let the stream terminate naturally when
                        // the byte stream closes.
                    }
                    Err(e) => {
                        lines.push(Err(OllamaError::Http(format!("NDJSON decode: {e}"))));
                    }
                }
            }
            futures::future::ready(Some(lines))
        })
        .flat_map(|batch| futures::stream::iter(batch.into_iter()));

    Ok(line_stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh_context::MeshContext;
    use serde_json::json;
    use std::collections::HashMap;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn test_ctx() -> MeshContext {
        let results = HashMap::from([(
            "science".to_string(),
            json!({"attractor": "DeepBasin", "turbulence": 0.2}),
        )]);
        MeshContext::build("hello", &results, &[])
    }

    #[tokio::test]
    async fn generate_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": "Hello, it's nice to meet you!",
                "done": true,
            })))
            .mount(&server)
            .await;

        let cfg = OllamaConfig {
            base_url: server.uri(),
            timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let reply = generate(&cfg, &test_ctx()).await.unwrap();
        assert_eq!(reply, "Hello, it's nice to meet you!");
    }

    #[tokio::test]
    async fn server_error_surfaces_as_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal"))
            .mount(&server)
            .await;

        let cfg = OllamaConfig {
            base_url: server.uri(),
            timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let err = generate(&cfg, &test_ctx()).await.unwrap_err();
        assert!(matches!(err, OllamaError::Http(_)));
    }

    #[tokio::test]
    async fn empty_response_is_typed_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "response": "  ",  // whitespace only
                "done": true,
            })))
            .mount(&server)
            .await;

        let cfg = OllamaConfig {
            base_url: server.uri(),
            timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let err = generate(&cfg, &test_ctx()).await.unwrap_err();
        assert!(matches!(err, OllamaError::EmptyResponse));
    }

    #[test]
    fn compose_prompt_includes_query_and_telemetry() {
        let ctx = test_ctx();
        let prompt = compose_prompt(&ctx);
        assert!(prompt.contains("hello"));
        assert!(prompt.contains("DeepBasin"));
        assert!(prompt.contains("Reply to the user"));
    }
}
