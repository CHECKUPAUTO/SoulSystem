use prometheus::{
    Histogram, HistogramOpts, IntCounter, IntGauge, Opts, Registry,
    Encoder, TextEncoder,
};
use lazy_static::lazy_static;

lazy_static! {
    static ref REGISTRY: Registry = Registry::new();
}

pub fn init_prometheus() -> &'static Registry {
    &REGISTRY
}

#[derive(Clone)]
pub struct PrometheusMetrics {
    pub tasks_executed: IntCounter,
    pub tasks_stolen: IntCounter,
    pub thermal_backoff_events: IntCounter,
    pub current_temp_celsius: IntGauge,
    pub task_latency_seconds: Histogram,
    pub scheduler_cycles: IntCounter,
    pub active_workers: IntGauge,
    pub queue_depth: IntGauge,
}

impl PrometheusMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let tasks_executed = IntCounter::with_opts(Opts::new(
            "soul_tasks_executed_total",
            "Total number of tasks executed",
        ))?;
        let tasks_stolen = IntCounter::with_opts(Opts::new(
            "soul_tasks_stolen_total",
            "Total number of tasks stolen from other workers",
        ))?;
        let thermal_backoff_events = IntCounter::with_opts(Opts::new(
            "soul_thermal_backoff_events_total",
            "Total number of thermal backoff events",
        ))?;
        let current_temp_celsius = IntGauge::with_opts(Opts::new(
            "soul_current_temp_celsius",
            "Current CPU temperature in Celsius",
        ))?;
        let task_latency_seconds = Histogram::with_opts(HistogramOpts::new(
            "soul_task_latency_seconds",
            "Task execution latency in seconds",
        ))?;
        let scheduler_cycles = IntCounter::with_opts(Opts::new(
            "soul_scheduler_cycles_total",
            "Total scheduler cycles executed",
        ))?;
        let active_workers = IntGauge::with_opts(Opts::new(
            "soul_active_workers",
            "Number of currently active worker threads",
        ))?;
        let queue_depth = IntGauge::with_opts(Opts::new(
            "soul_queue_depth",
            "Current task queue depth",
        ))?;

        REGISTRY.register(Box::new(tasks_executed.clone()))?;
        REGISTRY.register(Box::new(tasks_stolen.clone()))?;
        REGISTRY.register(Box::new(thermal_backoff_events.clone()))?;
        REGISTRY.register(Box::new(current_temp_celsius.clone()))?;
        REGISTRY.register(Box::new(task_latency_seconds.clone()))?;
        REGISTRY.register(Box::new(scheduler_cycles.clone()))?;
        REGISTRY.register(Box::new(active_workers.clone()))?;
        REGISTRY.register(Box::new(queue_depth.clone()))?;

        Ok(Self {
            tasks_executed,
            tasks_stolen,
            thermal_backoff_events,
            current_temp_celsius,
            task_latency_seconds,
            scheduler_cycles,
            active_workers,
            queue_depth,
        })
    }

    pub fn record_task_executed(&self) {
        self.tasks_executed.inc();
    }

    pub fn record_task_stolen(&self) {
        self.tasks_stolen.inc();
    }

    pub fn record_thermal_backoff(&self) {
        self.thermal_backoff_events.inc();
    }

    pub fn set_temperature(&self, celsius: i64) {
        self.current_temp_celsius.set(celsius);
    }

    pub fn record_task_latency(&self, seconds: f64) {
        self.task_latency_seconds.observe(seconds);
    }

    pub fn record_scheduler_cycles(&self, cycles: u64) {
        self.scheduler_cycles.inc_by(cycles);
    }

    pub fn set_active_workers(&self, count: i64) {
        self.active_workers.set(count);
    }

    pub fn set_queue_depth(&self, depth: i64) {
        self.queue_depth.set(depth);
    }
}

pub fn gather_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

pub struct PrometheusExporter {
    metrics: PrometheusMetrics,
}

impl PrometheusExporter {
    pub fn new() -> Result<Self, prometheus::Error> {
        let metrics = PrometheusMetrics::new()?;
        Ok(Self { metrics })
    }

    pub fn update_from_hub(&self, hub: &crate::metrics::TelemetryHub) {
        let (_total_tasks, total_cycles) = hub.aggregate_metrics();
        self.metrics.record_scheduler_cycles(total_cycles);
        
        let temp = hub.current_temp_celsius() as i64;
        self.metrics.set_temperature(temp);
        
        let core_metrics = hub.get_core_metrics();
        for core in core_metrics.iter() {
            self.metrics.tasks_executed.inc_by(core.tasks_executed);
            self.metrics.tasks_stolen.inc_by(core.tasks_stolen);
            self.metrics.thermal_backoff_events.inc_by(core.thermal_backoff_events as u64);
        }
    }

    pub fn metrics(&self) -> &PrometheusMetrics {
        &self.metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prometheus_metrics_creation() {
        let metrics = PrometheusMetrics::new().expect("Failed to create Prometheus metrics");
        metrics.record_task_executed();
        metrics.record_task_stolen();
        metrics.record_thermal_backoff();
        metrics.set_temperature(45);
        metrics.record_task_latency(0.001);
        metrics.record_scheduler_cycles(1000);
        metrics.set_active_workers(8);
        metrics.set_queue_depth(42);
        
        let output = gather_metrics();
        assert!(output.contains("soul_tasks_executed_total"));
        assert!(output.contains("soul_tasks_stolen_total"));
        assert!(output.contains("soul_thermal_backoff_events_total"));
        assert!(output.contains("soul_current_temp_celsius"));
    }
}