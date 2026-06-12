//! SciRust Web — WASM bindings and web UI for browser-based inference.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────┐   JSON    ┌──────────┐   WASM    ┌──────────────┐
//! │  Browser │◄─────────►│  HTTP    │◄─────────►│  SciRust     │
//! │  (HTML)  │   fetch   │  Server  │  bindings │  Inference   │
//! └──────────┘           └──────────┘           └──────────────┘
//! ```
//!
//! # Building for WASM
//!
//! ```bash
//! # Requires wasm-pack (install: cargo install wasm-pack)
//! wasm-pack build --target web -- --features wasm
//! ```
//!
//! # Usage in browser
//!
//! ```javascript
//! import init, { predict, load_model } from './scirust_web.js';
//! await init();
//! const model = load_model('/models/mnist.scirust');
//! const result = predict(model, new Float32Array([0.1, 0.2, ...]));
//! console.log('Predictions:', result);
//! ```

#![allow(unused)]

pub mod bindings;
pub mod server;
pub mod client;
