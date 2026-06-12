//! nCPU pre-filter — sub-millisecond reject of obviously-bad mutations.
//!
//! Optional HTTP client that delegates to the user's nCPU instance
//! (typically `http://localhost:7450`). The nCPU returns a verdict in <1 ms
//! based on cached patterns: if the candidate matches a known-bad shape
//! (syntax error, complexity explosion, denylist), we skip the costly
//! evaluator round-trip.
//!
//! When `endpoint` is empty or the call fails, the filter is a no-op (always
//! `Verdict::Accept`), so the rest of the engine works unchanged.

use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NcpuConfig {
    /// Empty string = disabled. Otherwise full URL of the nCPU classify endpoint.
    pub endpoint: String,
    pub timeout_ms: u64,
    /// Reject when the nCPU confidence is at or above this threshold.
    pub reject_threshold: f64,
}

impl Default for NcpuConfig {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            timeout_ms: 50,
            reject_threshold: 0.85,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    Reject,
    Unknown,
}

pub struct NcpuFilter {
    cfg: NcpuConfig,
    http: Client,
}

impl NcpuFilter {
    pub fn new(cfg: NcpuConfig) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_millis(cfg.timeout_ms.max(10)))
            .build()
            .expect("build reqwest client");
        Self { cfg, http }
    }

    pub fn is_enabled(&self) -> bool {
        !self.cfg.endpoint.is_empty()
    }

    /// Ask the nCPU for a quick verdict on a candidate.
    /// Always returns `Unknown` (not `Reject`) on transport errors so the
    /// engine doesn't silently drop candidates when the nCPU is down.
    pub async fn classify(&self, code: &str, language: &str) -> Verdict {
        if !self.is_enabled() {
            return Verdict::Unknown;
        }
        match self.call(code, language).await {
            Ok(reject_score) => {
                debug!(reject_score, "nCPU verdict");
                if reject_score >= self.cfg.reject_threshold {
                    Verdict::Reject
                } else {
                    Verdict::Accept
                }
            }
            Err(e) => {
                debug!(error=%e, "nCPU unavailable, treating as Unknown");
                Verdict::Unknown
            }
        }
    }

    async fn call(&self, code: &str, language: &str) -> Result<f64> {
        #[derive(Serialize)]
        struct Req<'a> {
            code: &'a str,
            language: &'a str,
        }
        #[derive(Deserialize)]
        struct Resp {
            /// Probability that the candidate is bad in [0, 1].
            #[serde(default)]
            reject_score: f64,
        }
        let req = Req { code, language };
        let r: Resp = self
            .http
            .post(&self.cfg.endpoint)
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(r.reject_score.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_returns_unknown() {
        let f = NcpuFilter::new(NcpuConfig::default());
        assert!(!f.is_enabled());
        let v = f.classify("fn x(){}", "rust").await;
        assert_eq!(v, Verdict::Unknown);
    }

    #[tokio::test]
    async fn unreachable_endpoint_returns_unknown() {
        let f = NcpuFilter::new(NcpuConfig {
            endpoint: "http://127.0.0.1:1/classify".into(), // garbage port
            timeout_ms: 50,
            ..Default::default()
        });
        let v = f.classify("anything", "rust").await;
        assert_eq!(v, Verdict::Unknown);
    }
}
