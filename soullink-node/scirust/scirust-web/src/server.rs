//! Tiny HTTP server for serving models and handling inference requests.
//!
//! Uses only `std::net` (no external HTTP library).
//! Serves static files from `www/` and exposes a `/predict` JSON endpoint.

use scirust_inference::layers::{Linear, Layer, ReLU};
use scirust_inference::pipeline::SequentialModel;
use scirust_inference::tensor::Tensor;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

/// A simple HTTP server for serving model predictions.
pub struct InferenceHttpServer {
    model: Arc<SequentialModel>,
    port: u16,
    running: bool,
}

impl InferenceHttpServer {
    /// Create a new HTTP server hosting the given model.
    pub fn new(model: SequentialModel, port: u16) -> Self {
        Self { model: Arc::new(model), port, running: false }
    }

    /// Start the server (blocking).
    pub fn serve(&mut self) -> Result<(), String> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| format!("HTTP bind: {}", e))?;
        self.running = true;

        println!("SciRust HTTP server listening on http://{}", addr);

        let model = self.model.clone();
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let m = model.clone();
                    thread::spawn(move || {
                        handle_http_request(&mut stream, &m);
                    });
                }
                Err(e) => eprintln!("HTTP error: {}", e),
            }
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool { self.running }
}

fn handle_http_request(stream: &mut TcpStream, model: &SequentialModel) {
    let mut buf = [0u8; 4096];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return,
    };

    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse HTTP method and path
    let (method, path) = match parse_http_request_line(&request) {
        Some(p) => p,
        None => {
            let _ = write_http_response(stream, 400, "Bad Request");
            return;
        }
    };

    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => serve_file(stream, "www/index.html", "text/html"),
        ("GET", "/health") => {
            let _ = write_http_response(stream, 200, "{\"status\":\"ok\"}");
        }
        ("POST", "/predict") => {
            handle_predict(stream, &request, model);
        }
        _ => {
            let _ = write_http_response(stream, 404, "Not Found");
        }
    }
}

fn handle_predict(stream: &mut TcpStream, request: &str, model: &SequentialModel) {
    // Extract JSON body
    let body = match extract_body(request) {
        Some(b) => b,
        None => {
            let _ = write_http_response(stream, 400, "{\"error\":\"no body\"}");
            return;
        }
    };

    // Parse JSON: {"data": [...], "shape": [...]}
    let input_data: Vec<f32> = body.split(',')
        .filter_map(|s| {
            let s = s.trim().trim_matches(|c| c == '[' || c == ']' || c == '{' || c == '}' || c == '"');
            s.parse::<f32>().ok()
        })
        .collect();

    if input_data.is_empty() {
        let _ = write_http_response(stream, 400, "{\"error\":\"invalid input\"}");
        return;
    }

    let input_shape = vec![1, input_data.len()];
    let input = match Tensor::<f32>::from_vec(input_data, &input_shape) {
        Ok(t) => t,
        Err(e) => {
            let _ = write_http_response(stream, 400, &format!("{{\"error\":\"{}\"}}", e));
            return;
        }
    };

    let output = match model.forward(&input) {
        Ok(o) => o,
        Err(e) => {
            let _ = write_http_response(stream, 500, &format!("{{\"error\":\"{}\"}}", e));
            return;
        }
    };

    let out_str: String = output.data().iter()
        .map(|v| format!("{:.6}", v))
        .collect::<Vec<_>>()
        .join(",");

    let json = format!("{{\"prediction\":[{}],\"shape\":{:?}}}", out_str, output.shape());
    let _ = write_http_response(stream, 200, &json);
}

fn write_http_response(stream: &mut TcpStream, status: u16, body: &str) -> Result<(), String> {
    let status_line = match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        404 => "404 Not Found",
        500 => "500 Internal Server Error",
        _ => "200 OK",
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        status_line, body.len(), body
    );
    stream.write_all(response.as_bytes())
        .map_err(|e| format!("Write: {}", e))
}

fn parse_http_request_line(request: &str) -> Option<(&str, &str)> {
    let first_line = request.lines().next()?;
    let mut parts = first_line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    Some((method, path))
}

fn extract_body(request: &str) -> Option<&str> {
    let parts: Vec<&str> = request.split("\r\n\r\n").collect();
    if parts.len() >= 2 { Some(parts[1]) } else { None }
}

fn serve_file(stream: &mut TcpStream, path: &str, content_type: &str) {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n{}",
                content_type, content.len(), content
            );
            let _ = stream.write_all(response.as_bytes());
        }
        Err(_) => {
            let _ = write_http_response(stream, 404, "File not found");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_http_request() {
        let req = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let (method, path) = parse_http_request_line(req).unwrap();
        assert_eq!(method, "GET");
        assert_eq!(path, "/health");
    }

    #[test]
    fn test_extract_body() {
        let req = "POST /predict HTTP/1.1\r\n\r\n{\"data\":[1.0,2.0]}";
        let body = extract_body(req).unwrap();
        assert_eq!(body, "{\"data\":[1.0,2.0]}");
    }

    #[test]
    fn test_write_response() {
        let mut buf: Vec<u8> = Vec::new();
        // Use a simple Write impl for testing
        struct TestStream(Vec<u8>);
        impl Write for TestStream {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
        }
        let mut test = TestStream(Vec::new());
        let result = super::write_http_response_impl(&mut test, 200, "{\"status\":\"ok\"}");
        let response = String::from_utf8(test.0).unwrap();
        assert!(response.contains("200 OK"));
        assert!(response.contains("{\"status\":\"ok\"}"));
    }
}

// Helper for testing: non-stream version of write_http_response
fn write_http_response_impl(w: &mut impl Write, status: u16, body: &str) -> Result<(), String> {
    let status_line = match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        _ => "200 OK",
    };
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        status_line, body.len(), body
    );
    w.write_all(response.as_bytes()).map_err(|e| format!("Write: {}", e))
}
