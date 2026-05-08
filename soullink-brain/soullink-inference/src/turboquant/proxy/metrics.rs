//! Proxy metrics — request counting and performance tracking.

#[derive(Debug, Default, Clone)]
pub struct ProxyMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_tokens_in: u64,
    pub total_tokens_out: u64,
    pub offload_events: u64,
    pub recall_events: u64,
}

impl ProxyMetrics {
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 { return 0.0; }
        self.successful_requests as f64 / self.total_requests as f64
    }

    pub fn avg_tokens_per_request(&self) -> f64 {
        if self.total_requests == 0 { return 0.0; }
        (self.total_tokens_in + self.total_tokens_out) as f64 / self.total_requests as f64
    }
}
