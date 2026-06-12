//! Service discovery for distributed inference.
//!
//! # Rendezvous Discovery
//!
//! Nodes register with a rendezvous point, which maintains a list of
//! available servers. Clients query the rendezvous to get server addresses.
//!
//! ```text
//! Server A ──register──► Rendezvous ◄──query── Client X
//! Server B ──register──► Rendezvous ◄──query── Client Y
//! ```
//!
//! Each server sends a heartbeat every N seconds. The rendezvous
//! removes servers that miss their heartbeat.

use crate::protocol::{read_message, write_message, DistributedMessage};
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A server registration with health tracking.
#[derive(Debug, Clone)]
pub struct ServerRegistration {
    pub address: String,
    pub num_layers: usize,
    pub num_parameters: usize,
    pub last_heartbeat: Instant,
}

/// Rendezvous server that tracks available inference servers.
pub struct RendezvousServer {
    listener: TcpListener,
    servers: Arc<Mutex<HashMap<String, ServerRegistration>>>,
    heartbeat_timeout: Duration,
    running: bool,
}

impl RendezvousServer {
    /// Create a new rendezvous server on the given address.
    pub fn new(bind_addr: &str) -> Result<Self, String> {
        let listener = TcpListener::bind(bind_addr)
            .map_err(|e| format!("Rendezvous bind: {}", e))?;

        Ok(Self {
            listener,
            servers: Arc::new(Mutex::new(HashMap::new())),
            heartbeat_timeout: Duration::from_secs(30),
            running: false,
        })
    }

    /// Start the rendezvous server (non-blocking).
    pub fn start(&mut self) -> Result<(), String> {
        if self.running { return Ok(()); }
        self.running = true;

        let servers = self.servers.clone();
        let listener = self.listener.try_clone()
            .map_err(|e| format!("Clone: {}", e))?;
        let timeout = self.heartbeat_timeout;

        thread::spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        let sv = servers.clone();
                        let to = timeout;
                        thread::spawn(move || {
                            handle_rendezvous_client(&mut stream, sv, to);
                        });
                    }
                    Err(_) => break,
                }
            }
        });

        // Background thread to clean stale servers
        let servers_cleanup = self.servers.clone();
        let timeout_cleanup = self.heartbeat_timeout;
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(10));
                let mut sv = servers_cleanup.lock().unwrap();
                sv.retain(|_, reg| reg.last_heartbeat.elapsed() < timeout_cleanup);
            }
        });

        Ok(())
    }

    /// Get a list of available servers.
    pub fn get_servers(&self) -> Vec<ServerRegistration> {
        let sv = self.servers.lock().unwrap();
        sv.values().cloned().collect()
    }

    /// Get the listener address.
    pub fn addr(&self) -> String {
        self.listener.local_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    }
}

fn handle_rendezvous_client(
    stream: &mut TcpStream,
    servers: Arc<Mutex<HashMap<String, ServerRegistration>>>,
    heartbeat_timeout: Duration,
) {
    loop {
        let msg = match read_message(stream) {
            Ok(m) => m,
            Err(_) => break,
        };

        let response = match msg {
            DistributedMessage::ModelInfoResponse { id, num_layers, num_parameters, .. } => {
                // Server registration/heartbeat
                let addr = stream.peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                let mut sv = servers.lock().unwrap();
                sv.insert(addr.clone(), ServerRegistration {
                    address: addr,
                    num_layers,
                    num_parameters,
                    last_heartbeat: Instant::now(),
                });
                DistributedMessage::Pong(id)
            }
            DistributedMessage::ModelInfoRequest { id } => {
                // Client query: return server list
                let sv = servers.lock().unwrap();
                let server_list: Vec<String> = sv.keys().cloned().collect();
                drop(sv);
                let msg_content = format!("[{}]", server_list.join(","));
                DistributedMessage::ModelInfoResponse {
                    id,
                    num_layers: server_list.len(),
                    num_parameters: 0,
                    layer_names: server_list,
                }
            }
            other => DistributedMessage::Error {
                id: other.id(),
                message: "Rendezvous: unexpected message".to_string(),
            },
        };

        if write_message(stream, &response).is_err() {
            break;
        }
    }
}

/// Register a server with a rendezvous point.
pub fn register_with_rendezvous(
    rendezvous_addr: &str,
    my_addr: &str,
    num_layers: usize,
    num_parameters: usize,
) -> Result<(), String> {
    let mut stream = TcpStream::connect(rendezvous_addr)
        .map_err(|e| format!("Connect to rendezvous: {}", e))?;

    // Send registration
    let reg = DistributedMessage::ModelInfoResponse {
        id: 1,
        num_layers,
        num_parameters,
        layer_names: vec![my_addr.to_string()],
    };
    write_message(&mut stream, &reg)?;

    // Receive acknowledgment
    let _ack = read_message(&mut stream)?;
    Ok(())
}

/// Query a rendezvous point for available servers.
pub fn query_rendezvous(rendezvous_addr: &str) -> Result<Vec<String>, String> {
    let mut stream = TcpStream::connect(rendezvous_addr)
        .map_err(|e| format!("Connect to rendezvous: {}", e))?;

    let query = DistributedMessage::ModelInfoRequest { id: 1 };
    write_message(&mut stream, &query)?;

    let response = read_message(&mut stream)?;
    match response {
        DistributedMessage::ModelInfoResponse { layer_names, .. } => Ok(layer_names),
        _ => Err("Unexpected rendezvous response".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires network sandbox permissions"]
    fn test_rendezvous_start_stop() {
        let mut rv = RendezvousServer::new("127.0.0.1:0").unwrap();
        rv.start().unwrap();
        let addr = rv.addr();
        assert!(!addr.is_empty());
    }

    #[test]
    fn test_server_registration() {
        let reg = ServerRegistration {
            address: "127.0.0.1:8080".to_string(),
            num_layers: 3,
            num_parameters: 1000,
            last_heartbeat: Instant::now(),
        };
        assert_eq!(reg.address, "127.0.0.1:8080");
        assert_eq!(reg.num_layers, 3);
    }
}
