//! TCP model client.
//!
//! Connects to a `ModelServer` and sends prediction requests.
//! Supports connection pooling and retry logic.

use crate::protocol::{next_id, read_message, write_message, DistributedMessage};
use scirust_inference::tensor::Tensor;
use std::io::{BufReader, BufWriter};
use std::net::TcpStream;
use std::time::Duration;

/// Response from a model info request.
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub num_layers: usize,
    pub num_parameters: usize,
    pub layer_names: Vec<String>,
}

/// TCP client for distributed inference.
pub struct ModelClient {
    stream: TcpStream,
    timeout: Duration,
}

impl ModelClient {
    /// Connect to a model server at the given address.
    pub fn connect(addr: &str) -> Result<Self, String> {
        let stream = TcpStream::connect(addr)
            .map_err(|e| format!("Connect to '{}': {}", addr, e))?;

        stream.set_read_timeout(Some(Duration::from_secs(10)))
            .map_err(|e| format!("Set timeout: {}", e))?;

        Ok(Self { stream, timeout: Duration::from_secs(10) })
    }

    /// Connect with a custom timeout.
    pub fn connect_with_timeout(addr: &str, timeout_secs: u64) -> Result<Self, String> {
        let stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| format!("Addr parse: {}", e))?,
            Duration::from_secs(timeout_secs),
        ).map_err(|e| format!("Connect to '{}': {}", addr, e))?;

        let timeout = Duration::from_secs(timeout_secs);
        stream.set_read_timeout(Some(timeout))
            .map_err(|e| format!("Set timeout: {}", e))?;

        Ok(Self { stream, timeout })
    }

    /// Send a predict request and get the result.
    pub fn predict(&mut self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        let id = next_id();
        let request = DistributedMessage::PredictRequest {
            id,
            input_shape: input.shape().to_vec(),
            input_data: input.data().to_vec(),
        };

        write_message(&mut self.stream, &request)?;

        let response = read_message(&mut self.stream)?;

        match response {
            DistributedMessage::PredictResponse { output_shape, output_data, .. } => {
                Tensor::<f32>::from_vec(output_data, &output_shape)
                    .map_err(|e| format!("Response tensor: {}", e))
            }
            DistributedMessage::Error { message, .. } => Err(format!("Server error: {}", message)),
            _ => Err(format!("Unexpected response: {}", response)),
        }
    }

    /// Get model information from the server.
    pub fn model_info(&mut self) -> Result<ModelInfo, String> {
        let id = next_id();
        write_message(&mut self.stream, &DistributedMessage::ModelInfoRequest { id })?;

        let response = read_message(&mut self.stream)?;

        match response {
            DistributedMessage::ModelInfoResponse { num_layers, num_parameters, layer_names, .. } => {
                Ok(ModelInfo { num_layers, num_parameters, layer_names })
            }
            DistributedMessage::Error { message, .. } => Err(format!("Server error: {}", message)),
            _ => Err(format!("Unexpected response: {}", response)),
        }
    }

    /// Ping the server to check health.
    pub fn ping(&mut self) -> Result<Duration, String> {
        let id = next_id();
        let start = std::time::Instant::now();
        write_message(&mut self.stream, &DistributedMessage::Ping(id))?;
        let response = read_message(&mut self.stream)?;

        match response {
            DistributedMessage::Pong(_) => Ok(start.elapsed()),
            _ => Err(format!("Expected pong, got: {}", response)),
        }
    }
}

// ---------------------------------------------------------------------------
// Connection pool for load-balanced clients
// ---------------------------------------------------------------------------

/// A pool of connections to multiple servers for load balancing.
pub struct ClientPool {
    clients: Vec<ModelClient>,
    next_idx: usize,
}

impl ClientPool {
    /// Create a pool from a list of server addresses.
    pub fn new(addresses: &[String]) -> Result<Self, String> {
        let mut clients = Vec::new();
        for addr in addresses {
            let client = ModelClient::connect(addr)?;
            clients.push(client);
        }
        Ok(Self { clients, next_idx: 0 })
    }

    /// Round-robin predict across servers.
    pub fn predict(&mut self, input: &Tensor<f32>) -> Result<Tensor<f32>, String> {
        if self.clients.is_empty() {
            return Err("No clients in pool".to_string());
        }
        let idx = self.next_idx % self.clients.len();
        self.next_idx += 1;
        self.clients[idx].predict(input)
    }

    /// Number of clients in the pool.
    pub fn len(&self) -> usize { self.clients.len() }
    pub fn is_empty(&self) -> bool { self.clients.is_empty() }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_pool_creation() {
        let pool = ClientPool::new(&[]).unwrap();
        assert!(pool.is_empty());
    }

    #[test]
    fn test_client_pool_empty_predict_fails() {
        let mut pool = ClientPool::new(&[]).unwrap();
        let input = Tensor::<f32>::ones(&[1, 2]);
        let result = pool.predict(&input);
        assert!(result.is_err());
    }
}
