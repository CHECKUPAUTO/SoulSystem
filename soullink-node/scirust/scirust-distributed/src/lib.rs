//! SciRust Distributed — Distributed inference engine with TCP-based model serving.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────┐     TCP      ┌───────────┐
//! │ Client 1 │─────►────────│  Model    │
//! ├──────────┤              │  Server   │
//! │ Client 2 │─────►────────│  (hosts   │
//! ├──────────┤              │   model)  │
//! │ Client N │─────►────────│           │
//! └──────────┘              └───────────┘
//! ```
//!
//! # Modules
//!
//! - `server` — TCP model server that loads a model and serves predictions
//! - `client` — TCP client that sends data and receives predictions
//! - `parallel` — Data parallelism using rayon for multi-threaded inference
//! - `protocol` — Message serialization for the distributed protocol
//!
//! # Example
//!
//! ```ignore
//! use scirust_distributed::server::ModelServer;
//! use scirust_inference::pipeline::SequentialModel;
//! use std::net::TcpListener;
//!
//! // Server side
//! let model = SequentialModel::new();
//! let mut server = ModelServer::new(model, "127.0.0.1:0").unwrap();
//!
//! // Client side
//! use scirust_distributed::client::ModelClient;
//! let mut client = ModelClient::connect(addr).unwrap();
//! let result = client.predict(&input).unwrap();
//! ```

#![allow(unused)]

pub mod protocol;
pub mod server;
pub mod client;
pub mod parallel;
pub mod discovery;

pub mod prelude {
    pub use crate::protocol::*;
    pub use crate::server::*;
    pub use crate::client::*;
    pub use crate::parallel::*;
}
