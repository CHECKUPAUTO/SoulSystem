use avid_scout::ScoutEngine;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;
use std::time::Instant;

fn tiny_http_server(html: String) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to address");
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        // Handle multiple requests if needed, but for benchmark one might be enough
        while let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                html.len(),
                html
            );
            let _ = stream.write_all(response.as_bytes());
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

    // Increase iterations for better stability and to measure the impact of single-parse optimization.
    let iterations = 50;
    let start = Instant::now();
    for _ in 0..iterations {
        // Use a new engine instance to bypass the in-memory page cache and measure extraction performance.
        let engine = ScoutEngine::new();
        let _ = engine.crawl(&url, 0).await.unwrap();
    }
    let duration = start.elapsed();

    println!(
        "⚡ Bolt: Crawl benchmark took {:?} for {} iterations ({:?} per iteration)",
        duration,
        iterations,
        duration / iterations
    );
}
