//! TCP model server.
//!
//! Hosts a `SequentialModel` and serves predictions to clients over TCP.
//! Supports concurrent connections via threading.

use crate::protocol::{next_id, read_message, write_message, DistributedMessage};
use scirust_inference::layers::Layer;
use scirust_inference::pipeline::SequentialModel;
use scirust_inference::tensor::Tensor;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// A TCP server that hosts an inference model.
pub struct ModelServer {
    model: Arc<SequentialModel>,
    listener: TcpListener,
    addr: String,
    running: bool,
    handles: Vec<thread::JoinHandle<()>>,
}

impl ModelServer {
    /// Create a new server bound to the given address.
    ///
    /// If the port is `0`, the OS assigns a random available port.
    pub fn new(model: SequentialModel, bind_addr: &str) -> Result<Self, String> {
        let listener = TcpListener::bind(bind_addr)
            .map_err(|e| format!("Bind error: {}", e))?;
        let addr = listener.local_addr()
            .map_err(|e| format!("Local addr error: {}", e))?
            .to_string();

        Ok(Self {
            model: Arc::new(model),
            listener,
            addr,
            running: false,
            handles: Vec::new(),
        })
    }

    /// Get the server address.
    pub fn addr(&self) -> &str { &self.addr }

    /// Start the server (non-blocking, spawns threads).
    pub fn start(&mut self) -> Result<(), String> {
        if self.running { return Ok(()); }
        self.running = true;

        let model = self.model.clone();
        let listener = self.listener.try_clone()
            .map_err(|e| format!("Clone listener: {}", e))?;

        let handle = thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let m = model.clone();
                        thread::spawn(move || {
                            handle_client(stream, m);
                        });
                    }
                    Err(e) => eprintln!("Connection error: {}", e),
                }
            }
        });

        self.handles.push(handle);
        Ok(())
    }

    /// Start and block forever (runs on the current thread).
    pub fn run_blocking(&mut self) -> Result<(), String> {
        self.running = true;
        let model = self.model.clone();

        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => {
                    let m = model.clone();
                    thread::spawn(move || {
                        handle_client(stream, m);
                    });
                }
                Err(e) => eprintln!("Connection error: {}", e),
            }
        }
        Ok(())
    }

    /// Check if server is running.
    pub fn is_running(&self) -> bool { self.running }

    /// Stop the server.
    pub fn stop(&mut self) {
        self.running = false;
    }
}

fn handle_client(mut stream: TcpStream, model: Arc<SequentialModel>) {
    loop {
        let msg = match read_message(&mut stream) {
            Ok(msg) => msg,
            Err(_) => break, // Connection closed or error
        };

        let response = match msg {
            DistributedMessage::PredictRequest { id, input_shape, input_data } => {
                let start = Instant::now();
                let tensor = match Tensor::<f32>::from_vec(input_data, &input_shape) {
                    Ok(t) => t,
                    Err(e) => {
                        let _ = write_message(&mut stream, &DistributedMessage::Error {
                            id, message: format!("Tensor error: {}", e),
                        });
                        continue;
                    }
                };

                match model.forward(&tensor) {
                    Ok(output) => {
                        let timing = start.elapsed().as_secs_f64() * 1000.0;
                        DistributedMessage::PredictResponse {
                            id,
                            output_shape: output.shape().to_vec(),
                            output_data: output.data().to_vec(),
                            timing_ms: timing,
                        }
                    }
                    Err(e) => DistributedMessage::Error {
                        id, message: format!("Forward error: {}", e),
                    }
                }
            }
            DistributedMessage::ModelInfoRequest { id } => {
                let layer_names = model.layer_names();
                DistributedMessage::ModelInfoResponse {
                    id,
                    num_layers: model.num_layers(),
                    num_parameters: model.num_parameters(),
                    layer_names,
                }
            }
            DistributedMessage::Ping(id) => DistributedMessage::Pong(id),
            _ => DistributedMessage::Error {
                id: msg.id(),
                message: "Unsupported message".to_string(),
            },
        };

        if write_message(&mut stream, &response).is_err() {
            break;
        }
    }
}

impl Drop for ModelServer {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Tests (integration, requires networking)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::ModelClient;
    use scirust_inference::layers::{Linear, ReLU};

    #[test]
    #[ignore = "Requires network sandbox permissions"]
    fn test_server_start_stop() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(4, 2, false));

        let mut server = ModelServer::new(model, "127.0.0.1:0").unwrap();
        assert!(!server.is_running());
        server.start().unwrap();
        assert!(server.is_running());
    }

    #[test]
    #[ignore = "Requires network sandbox permissions"]
    fn test_client_server_predict() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(4, 2, false));

        let mut server = ModelServer::new(model, "127.0.0.1:0").unwrap();
        let addr = server.addr().to_string();
        server.start().unwrap();

        // Give server a moment to start
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Client connects and predicts
        let mut client = ModelClient::connect(&addr).unwrap();
        let input = Tensor::<f32>::ones(&[1, 4]);
        let result = client.predict(&input).unwrap();
        assert_eq!(result.shape(), &[1, 2]);
    }

    #[test]
    #[ignore = "Requires network sandbox permissions"]
    fn test_client_server_model_info() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(10, 5, true));
        model.add(ReLU::new());

        let mut server = ModelServer::new(model, "127.0.0.1:0").unwrap();
        let addr = server.addr().to_string();
        server.start().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut client = ModelClient::connect(&addr).unwrap();
        let info = client.model_info().unwrap();
        assert_eq!(info.num_layers, 2);
        assert!(info.num_parameters > 0);
    }

    #[test]
    #[ignore = "Requires network sandbox permissions"]
    fn test_multiple_requests() {
        let mut model = SequentialModel::new();
        model.add(Linear::new(2, 1, false));

        let mut server = ModelServer::new(model, "127.0.0.1:0").unwrap();
        let addr = server.addr().to_string();
        server.start().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut client = ModelClient::connect(&addr).unwrap();

        // Multiple sequential requests
        for _ in 0..5 {
            let input = Tensor::<f32>::ones(&[1, 2]);
            let result = client.predict(&input).unwrap();
            assert_eq!(result.shape(), &[1, 1]);
        }
    }
}
