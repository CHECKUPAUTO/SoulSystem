#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
use reqwest::Proxy;

/// A rotating proxy pool.
#[derive(Debug)]
pub struct ProxyPool {
    urls: Vec<String>,
    current: std::sync::atomic::AtomicUsize,
}

impl Clone for ProxyPool {
    fn clone(&self) -> Self {
        Self {
            urls: self.urls.clone(),
            current: std::sync::atomic::AtomicUsize::new(
                self.current.load(std::sync::atomic::Ordering::Relaxed),
            ),
        }
    }
}

impl ProxyPool {
    /// Create a new pool from a list of proxy URLs (e.g. `http://127.0.0.1:8080`).
    pub fn new(urls: Vec<String>) -> Self {
        Self {
            urls,
            current: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Get the next proxy in round-robin order.
    pub fn next_proxy(&self) -> Option<Proxy> {
        let urls = &self.urls;
        if urls.is_empty() {
            return None;
        }
        let idx = self
            .current
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % urls.len();
        let url = &urls[idx];
        Proxy::all(url).ok()
    }

    /// Build a reqwest client with the next proxy applied.
    pub fn build_client(&self) -> reqwest::Client {
        let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10));
        if let Some(proxy) = self.next_proxy() {
            builder = builder.proxy(proxy);
        }
        builder.build().expect("valid client configuration")
    }
}
