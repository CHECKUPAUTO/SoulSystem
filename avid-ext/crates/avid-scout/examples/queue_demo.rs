use avid_scout::{ScoutConfig, ScoutEngine};
use std::collections::HashSet;

#[tokio::main]
async fn main() {
    // Audit-compliant initialization (no field_reassign_with_default)
    let config = ScoutConfig {
        max_depth: 1,
        allowed_domains: Some(HashSet::from(["httpbin.org".to_string()])),
        rate_limit_ms: 200,
        ..ScoutConfig::default()
    };

    let engine = ScoutEngine::with_config(config);
    let seed_url = "https://httpbin.org/html";

    println!("Starting deep crawl demo from: {seed_url}");
    let pages = engine.crawl_deep(seed_url).await;

    println!("Crawled {} pages:", pages.len());
    for page in &pages {
        println!(" - {} ({} links found)", page.url, page.links.len());
    }

    let metrics = engine.metrics().await;
    println!(
        "\nFinal Metrics: crawled={}, cached={}, blocked={}",
        metrics
            .pages_crawled
            .load(std::sync::atomic::Ordering::Relaxed),
        metrics
            .pages_cached
            .load(std::sync::atomic::Ordering::Relaxed),
        metrics
            .robots_blocked
            .load(std::sync::atomic::Ordering::Relaxed),
    );
}
