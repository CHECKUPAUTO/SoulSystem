//! Métriques Prometheus — compteurs, jauges, histogrammes pour les bridges
//!
//! Format Prometheus text format (0.0.4).
//! Exposé via `GET /metrics` sur :9023.
//!
//! Métriques exposées :
//!   - soulsystem_bridge_state{name="..."} 1|0 (1=reachable)
//!   - soulsystem_bridge_calls_total{name="...",result="ok|err"} counter
//!   - soulsystem_bridge_latency_seconds{name="..."} histogram
//!   - soulsystem_circuit_state{name="..."} 0=closed, 1=half_open, 2=open
//!   - soulsystem_build_info gauge (constant)

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone, Default)]
pub struct Counter {
    pub value: u64,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct Gauge {
    pub value: f64,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Default)]
pub struct Metrics {
    pub counters: HashMap<String, Vec<Counter>>,
    pub gauges: HashMap<String, Vec<Gauge>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_counter(&mut self, name: &str, labels: HashMap<String, String>) {
        let key = format!("{}_{}", name, Self::label_key(&labels));
        let counters = self.counters.entry(name.to_string()).or_default();
        if let Some(c) = counters.iter_mut().find(|c| c.labels == labels) {
            c.value += 1;
        } else {
            counters.push(Counter { value: 1, labels });
        }
        let _ = key; // for future keyed indexing
    }

    pub fn set_gauge(&mut self, name: &str, value: f64, labels: HashMap<String, String>) {
        let gauges = self.gauges.entry(name.to_string()).or_default();
        if let Some(g) = gauges.iter_mut().find(|g| g.labels == labels) {
            g.value = value;
        } else {
            gauges.push(Gauge { value, labels });
        }
    }

    fn label_key(labels: &HashMap<String, String>) -> String {
        let mut entries: Vec<_> = labels.iter().collect();
        entries.sort_by_key(|(k, _)| *k);
        entries
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }

    pub fn render(&self) -> String {
        let mut out = String::new();

        // HELP/TYPE pour counters
        for (name, counters) in &self.counters {
            out.push_str(&format!("# HELP soulsystem_{} Total count\n", name));
            out.push_str(&format!("# TYPE soulsystem_{} counter\n", name));
            for c in counters {
                out.push_str(&format!(
                    "soulsystem_{}{{{}}} {}\n",
                    name,
                    Self::label_str(&c.labels),
                    c.value
                ));
            }
        }

        // HELP/TYPE pour gauges
        for (name, gauges) in &self.gauges {
            out.push_str(&format!("# HELP soulsystem_{} Gauge value\n", name));
            out.push_str(&format!("# TYPE soulsystem_{} gauge\n", name));
            for g in gauges {
                out.push_str(&format!(
                    "soulsystem_{}{{{}}} {}\n",
                    name,
                    Self::label_str(&g.labels),
                    g.value
                ));
            }
        }

        // build_info
        out.push_str("# HELP soulsystem_build_info Build info (always 1)\n");
        out.push_str("# TYPE soulsystem_build_info gauge\n");
        out.push_str(&format!(
            "soulsystem_build_info{{version=\"{}\",features=\"\"}} 1\n",
            env!("CARGO_PKG_VERSION")
        ));

        out
    }

    fn label_str(labels: &HashMap<String, String>) -> String {
        if labels.is_empty() {
            return String::new();
        }
        let mut entries: Vec<_> = labels.iter().collect();
        entries.sort_by_key(|(k, _)| *k);
        entries
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v))
            .collect::<Vec<_>>()
            .join(",")
    }
}

#[derive(Clone)]
pub struct MetricsRegistry {
    inner: Arc<RwLock<Metrics>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Metrics::new())),
        }
    }

    pub async fn inc_counter(&self, name: &str, labels: HashMap<String, String>) {
        let mut m = self.inner.write().await;
        m.inc_counter(name, labels);
    }

    pub async fn set_gauge(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        let mut m = self.inner.write().await;
        m.set_gauge(name, value, labels);
    }

    pub async fn render(&self) -> String {
        let m = self.inner.read().await;
        m.render()
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialise le registre global de métriques.
pub fn init() -> MetricsRegistry {
    let r = MetricsRegistry::new();
    info!("📊 Metrics registry initialized");
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_counter_inc() {
        let r = MetricsRegistry::new();
        let mut l = HashMap::new();
        l.insert("name".to_string(), "test".to_string());
        r.inc_counter("bridge_calls", l.clone()).await;
        r.inc_counter("bridge_calls", l).await;
        let out = r.render().await;
        assert!(out.contains("soulsystem_bridge_calls"));
    }

    #[tokio::test]
    async fn test_gauge_set() {
        let r = MetricsRegistry::new();
        let mut l = HashMap::new();
        l.insert("name".to_string(), "test".to_string());
        r.set_gauge("bridge_state", 1.0, l).await;
        let out = r.render().await;
        assert!(out.contains("soulsystem_bridge_state"));
    }
}
