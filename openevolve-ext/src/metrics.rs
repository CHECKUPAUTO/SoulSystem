//! Prometheus metrics + per-provider cost tracking.
//!
//! Exposes counters, gauges and histograms via the `metrics` crate facade and
//! a `/metrics` endpoint via `metrics-exporter-prometheus`. The exporter is
//! installed *globally* on first call to `init()` — calling it twice is a no-op.
//!
//! Counters / gauges declared:
//!
//!   `openevolve_iterations_total`              counter, labels: status (ok|failed|repaired)
//!   `openevolve_mutations_total`               counter, labels: op
//!   `openevolve_repair_attempts_total`         counter
//!   `openevolve_repair_successes_total`        counter
//!   `openevolve_evaluations_total`             counter, labels: outcome
//!   `openevolve_best_score`                    gauge
//!   `openevolve_population_size`               gauge, labels: island
//!   `openevolve_breaker_open`                  gauge, labels: provider (0|1)
//!   `openevolve_iteration_duration_seconds`    histogram
//!   `openevolve_llm_tokens_in_total`           counter, labels: provider, model
//!   `openevolve_llm_tokens_out_total`          counter, labels: provider, model
//!   `openevolve_llm_usd_total`                 counter, labels: provider, model

use anyhow::{Context, Result};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::OnceLock;

static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Initialise the global Prometheus exporter. Idempotent.
pub fn init() -> Result<()> {
    if HANDLE.get().is_some() {
        return Ok(());
    }
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .context("install metrics-exporter-prometheus recorder")?;
    let _ = HANDLE.set(handle);
    Ok(())
}

/// Render the current metrics snapshot as Prometheus exposition format.
pub fn render() -> String {
    HANDLE.get().map(|h| h.render()).unwrap_or_default()
}

// ── Convenience emitters ────────────────────────────────────────────────

pub fn inc_iteration(status: &'static str) {
    metrics::counter!("openevolve_iterations_total", "status" => status).increment(1);
}

pub fn inc_mutation(op: &str) {
    metrics::counter!("openevolve_mutations_total", "op" => op.to_string()).increment(1);
}

pub fn record_repair(success: bool) {
    metrics::counter!("openevolve_repair_attempts_total").increment(1);
    if success {
        metrics::counter!("openevolve_repair_successes_total").increment(1);
    }
}

pub fn record_evaluation(outcome: &'static str) {
    metrics::counter!("openevolve_evaluations_total", "outcome" => outcome).increment(1);
}

pub fn set_best_score(s: f64) {
    metrics::gauge!("openevolve_best_score").set(s);
}

pub fn set_island_size(island: usize, size: usize) {
    metrics::gauge!("openevolve_population_size", "island" => island.to_string()).set(size as f64);
}

pub fn set_breaker(provider: &str, open: bool) {
    metrics::gauge!("openevolve_breaker_open", "provider" => provider.to_string()).set(if open {
        1.0
    } else {
        0.0
    });
}

pub fn record_iteration_duration(secs: f64) {
    metrics::histogram!("openevolve_iteration_duration_seconds").record(secs);
}

pub fn record_llm_call(provider: &str, model: &str, tokens_in: u64, tokens_out: u64, usd: f64) {
    let p = provider.to_string();
    let m = model.to_string();
    metrics::counter!("openevolve_llm_tokens_in_total",  "provider" => p.clone(), "model" => m.clone())
        .increment(tokens_in);
    metrics::counter!("openevolve_llm_tokens_out_total", "provider" => p.clone(), "model" => m.clone())
        .increment(tokens_out);
    // Prometheus counters are integers; we scale USD by 1e6 (micro-dollars).
    metrics::counter!("openevolve_llm_usd_total",        "provider" => p,         "model" => m)
        .increment((usd * 1_000_000.0) as u64);
}

// ── Cost tracker (in-process aggregator) ────────────────────────────────

#[derive(Debug, Default, Clone, Serialize)]
pub struct CostSummary {
    pub provider: String,
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub usd_estimate: f64,
}

#[derive(Debug, Default)]
pub struct CostTracker {
    inner: Mutex<HashMap<(String, String), CostSummary>>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one LLM call. Also pushes to Prometheus and to the SQLite
    /// `costs` table via the caller (we don't hold a DB ref here to avoid
    /// circular deps).
    pub fn record(&self, provider: &str, model: &str, tokens_in: u64, tokens_out: u64) -> f64 {
        let usd = estimate_cost_usd(provider, model, tokens_in, tokens_out);
        record_llm_call(provider, model, tokens_in, tokens_out, usd);
        let mut g = self.inner.lock();
        let key = (provider.to_string(), model.to_string());
        let entry = g.entry(key).or_insert_with(|| CostSummary {
            provider: provider.into(),
            model: model.into(),
            tokens_in: 0,
            tokens_out: 0,
            usd_estimate: 0.0,
        });
        entry.tokens_in += tokens_in;
        entry.tokens_out += tokens_out;
        entry.usd_estimate += usd;
        usd
    }

    pub fn snapshot(&self) -> Vec<CostSummary> {
        self.inner.lock().values().cloned().collect()
    }

    pub fn total_usd(&self) -> f64 {
        self.inner.lock().values().map(|c| c.usd_estimate).sum()
    }
}

/// Pricing table — best-effort, rounded, easy to update.
/// Returns USD for the given (provider, model, tokens_in, tokens_out) call.
/// Local Ollama returns 0.
pub fn estimate_cost_usd(provider: &str, model: &str, tokens_in: u64, tokens_out: u64) -> f64 {
    let (in_per_mtok, out_per_mtok) = match provider {
        "ollama" => (0.0, 0.0),
        "openai" => match model {
            m if m.starts_with("gpt-4o-mini") => (0.15, 0.60),
            m if m.starts_with("gpt-4o") => (2.50, 10.00),
            m if m.starts_with("gpt-4") => (10.00, 30.00),
            m if m.starts_with("gpt-3.5") => (0.50, 1.50),
            _ => (1.00, 3.00),
        },
        "anthropic" => match model {
            m if m.contains("opus") => (15.00, 75.00),
            m if m.contains("sonnet") => (3.00, 15.00),
            m if m.contains("haiku") => (0.80, 4.00),
            _ => (3.00, 15.00),
        },
        "gemini" => match model {
            m if m.contains("pro") => (1.25, 5.00),
            m if m.contains("flash") => (0.075, 0.30),
            _ => (0.50, 1.50),
        },
        _ => (1.00, 3.00),
    };
    let mtok_in = tokens_in as f64 / 1_000_000.0;
    let mtok_out = tokens_out as f64 / 1_000_000.0;
    mtok_in * in_per_mtok + mtok_out * out_per_mtok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_is_free() {
        let usd = estimate_cost_usd("ollama", "qwen3:latest", 1_000_000, 1_000_000);
        assert!(usd.abs() < 1e-9);
    }

    #[test]
    fn openai_gpt4o_pricing() {
        // 1M in + 1M out at gpt-4o → ~$12.50
        let usd = estimate_cost_usd("openai", "gpt-4o", 1_000_000, 1_000_000);
        assert!((usd - 12.5).abs() < 1e-6);
    }

    #[test]
    fn cost_tracker_accumulates() {
        let t = CostTracker::new();
        t.record("openai", "gpt-4o-mini", 1_000, 500);
        t.record("openai", "gpt-4o-mini", 2_000, 1_000);
        let s = t.snapshot();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].tokens_in, 3000);
        assert_eq!(s[0].tokens_out, 1500);
        assert!(t.total_usd() > 0.0);
    }
}
