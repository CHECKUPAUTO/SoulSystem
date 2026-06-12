use avid_scout::{ScoutConfig, ScoutEngine};

#[tokio::main]
async fn main() {
    let config = ScoutConfig {
        max_depth: 1,
        rate_limit_ms: 200,
        ..ScoutConfig::default()
    };
    let engine = ScoutEngine::with_config(config);

    let url = "https://httpbin.org/html";
    match engine.crawl(url, 0).await {
        Ok(pages) => {
            for page in &pages {
                println!("URL:      {}", page.url);
                println!("Status:   {}", page.status);
                println!("Title:    {:?}", page.title);
                println!("Lang:     {:?}", page.lang);
                println!("Meta:     {:?}", page.metadata.description);
                println!("Links:    {:?}", page.links);
                println!("Images:   {}", page.images.len());
                println!("Text:     {} chars", page.text.len());
                println!("Charset:  {:?}", page.charset);
                println!("Schema:   {:?}", page.schema_type);
            }
        }
        Err(e) => eprintln!("Crawl failed: {e}"),
    }

    match engine.crawl(url, 0).await {
        Ok(pages) => {
            println!("Second crawl returned {} pages (from cache).", pages.len());
        }
        Err(e) => eprintln!("Second crawl failed: {e}"),
    }

    let metrics = engine.metrics().await;
    println!(
        "Metrics: crawled={}, cached={}, blocked={}",
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
