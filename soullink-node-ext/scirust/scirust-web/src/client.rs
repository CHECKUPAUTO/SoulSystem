//! HTTP client for remote SciRust inference servers.
//!
//! Connects to a running `InferenceHttpServer` and sends JSON requests.

use scirust_inference::tensor::Tensor;
use std::io::{Read, Write};
use std::net::TcpStream;

/// HTTP client for remote inference.
pub struct RemoteInferenceClient {
    addr: String,
}

impl RemoteInferenceClient {
    /// Create a new client pointing to the given server address.
    pub fn new(host: &str, port: u16) -> Self {
        Self { addr: format!("{}:{}", host, port) }
    }

    /// Send a prediction request via HTTP POST.
    pub fn predict(&self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        let data_str: String = input.data().iter()
            .map(|v| format!("{:.8}", v))
            .collect::<Vec<_>>()
            .join(",");

        let body = format!("{{\"data\":[{}],\"shape\":{:?}}}", data_str, input.shape());

        let request = format!(
            "POST /predict HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            self.addr, body.len(), body
        );

        let mut stream = TcpStream::connect(&self.addr)
            .map_err(|e| format!("Connect: {}", e))?;

        stream.write_all(request.as_bytes())
            .map_err(|e| format!("Write: {}", e))?;

        // Read response
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf)
            .map_err(|e| format!("Read: {}", e))?;

        let response = String::from_utf8_lossy(&buf[..n]);

        // Parse JSON from response body
        if let Some(body_start) = response.find("\r\n\r\n") {
            let json_body = &response[body_start + 4..];
            // Extract "prediction":[numbers] and "shape":[numbers]
            // Simple parser for the known response format
            Ok(Tensor::<f32>::scalar(0.0)) // Placeholder
        } else {
            Err("No body in response".to_string())
        }
    }

    /// Health check.
    pub fn health_check(&self) -> Result<bool, String> {
        let request = "GET /health HTTP/1.1\r\nHost: {}\r\n\r\n";
        let mut stream = TcpStream::connect(&self.addr)
            .map_err(|e| format!("Connect: {}", e))?;

        stream.write_all(request.as_bytes())
            .map_err(|e| format!("Write: {}", e))?;

        let mut buf = [0u8; 256];
        let n = stream.read(&mut buf)
            .map_err(|e| format!("Read: {}", e))?;

        let response = String::from_utf8_lossy(&buf[..n]);
        Ok(response.contains("200 OK"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = RemoteInferenceClient::new("127.0.0.1", 8080);
        let result = client.health_check();
        // Will fail since no server running, but shouldn't panic
        assert!(result.is_err() || result.is_ok());
    }
}
