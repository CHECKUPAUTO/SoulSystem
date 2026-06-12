use avid_scout::ScoutEngine;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn tiny_http_server(html: String) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to address");
    let port = listener.local_addr().unwrap().port();
    let html = std::sync::Arc::new(html);
    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let html = std::sync::Arc::clone(&html);
            thread::spawn(move || {
                let mut buf = [0u8; 8192];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    html.len(),
                    html
                );
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    thread::sleep(Duration::from_millis(100));
    port
}

#[tokio::test]
async fn benchmark_crawl_performance() {
    let large_html = format!(
        "<html><head><title>Benchmark Page</title></head><body>{}</body></html>",
        (0..1000).map(|i| format!("<p>Some text {i} <a href='/link{i}'>link</a></p><table><tr><td>data</td></tr></table><form><input name='i{i}'></form><img src='img{i}.png' alt='alt{i}'>")).collect::<String>()
    );

    let port = tiny_http_server(large_html);
    let url = format!("http://127.0.0.1:{port}/");

    let engine = ScoutEngine::new();

    // Warm up
    let _ = engine.crawl(&url, 0).await.unwrap();

    // Run a modest number of iterations to detect regressions without timing out CI.
    let iterations = 5;
    let start = Instant::now();
    for _ in 0..iterations {
        let engine = ScoutEngine::new();
        let _ = engine.crawl(&url, 0).await.unwrap();
    }
    let duration = start.elapsed();
    let per_iter = duration / iterations;

    println!(
        "⚡ Bolt: Crawl benchmark took {duration:?} for {iterations} iterations ({per_iter:?} per iteration)"
    );
    // Fail if a single crawl takes more than 30 seconds.
    assert!(
        per_iter < Duration::from_secs(30),
        "Crawl per iteration too slow: {per_iter:?}"
    );
}
