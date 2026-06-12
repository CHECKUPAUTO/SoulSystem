//! Wire protocol for distributed inference messages.
//!
//! Messages are serialized as newline-delimited JSON (one line per message).
//! Each message starts with a 4-byte big-endian length prefix for framing.

use scirust_inference::tensor::Tensor;
use std::io::{Read, Write};

// ---------------------------------------------------------------------------
// Message types
// ---------------------------------------------------------------------------

/// Messages exchanged between client and server.
#[derive(Debug, Clone, PartialEq)]
pub enum DistributedMessage {
    /// Client → Server: predict request
    PredictRequest {
        id: u64,
        input_shape: Vec<usize>,
        input_data: Vec<f32>,
    },
    /// Server → Client: predict response
    PredictResponse {
        id: u64,
        output_shape: Vec<usize>,
        output_data: Vec<f32>,
        timing_ms: f64,
    },
    /// Client → Server: model info request
    ModelInfoRequest { id: u64 },
    /// Server → Client: model info response
    ModelInfoResponse {
        id: u64,
        num_layers: usize,
        num_parameters: usize,
        layer_names: Vec<String>,
    },
    /// Either side: error
    Error { id: u64, message: String },
    /// Either side: ping/pong for health checks
    Ping(u64),
    Pong(u64),
}

impl DistributedMessage {
    /// Serialize to bytes (length-prefixed JSON).
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let json = self.to_json()?;
        let bytes = json.into_bytes();
        let len = bytes.len() as u32;

        let mut result = Vec::with_capacity(4 + bytes.len());
        result.extend_from_slice(&len.to_be_bytes());
        result.extend_from_slice(&bytes);
        Ok(result)
    }

    /// Deserialize from bytes (length-prefixed).
    pub fn from_bytes(data: &[u8]) -> Result<(Self, usize), String> {
        if data.len() < 4 {
            return Err("Data too short for length prefix".to_string());
        }
        let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if data.len() < 4 + len {
            return Err(format!("Data too short: need {} bytes, got {}", 4 + len, data.len()));
        }
        let json_str = std::str::from_utf8(&data[4..4 + len])
            .map_err(|e| format!("Invalid UTF-8: {}", e))?;
        let msg = Self::from_json(json_str)?;
        Ok((msg, 4 + len))
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, String> {
        match self {
            DistributedMessage::PredictRequest { id, input_shape, input_data } => {
                let data_str = input_data.iter()
                    .map(|v| format!("{:.8}", v))
                    .collect::<Vec<_>>()
                    .join(",");
                Ok(format!("{{\"type\":\"predict\",\"id\":{},\"shape\":[{}],\"data\":[{}]}}",
                    id, input_shape.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","), data_str))
            }
            DistributedMessage::PredictResponse { id, output_shape, output_data, timing_ms } => {
                let data_str = output_data.iter()
                    .map(|v| format!("{:.8}", v))
                    .collect::<Vec<_>>()
                    .join(",");
                Ok(format!("{{\"type\":\"predict_resp\",\"id\":{},\"shape\":[{}],\"data\":[{}],\"timing_ms\":{:.4}}}",
                    id, output_shape.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","), data_str, timing_ms))
            }
            DistributedMessage::ModelInfoRequest { id } => {
                Ok(format!("{{\"type\":\"model_info\",\"id\":{}}}", id))
            }
            DistributedMessage::ModelInfoResponse { id, num_layers, num_parameters, layer_names } => {
                let names = layer_names.iter().map(|n| format!("\"{}\"", n)).collect::<Vec<_>>().join(",");
                Ok(format!("{{\"type\":\"model_info_resp\",\"id\":{},\"layers\":{},\"params\":{},\"names\":[{}]}}",
                    id, num_layers, num_parameters, names))
            }
            DistributedMessage::Error { id, message } => {
                Ok(format!("{{\"type\":\"error\",\"id\":{},\"msg\":\"{}\"}}", id, message))
            }
            DistributedMessage::Ping(id) => Ok(format!("{{\"type\":\"ping\",\"id\":{}}}", id)),
            DistributedMessage::Pong(id) => Ok(format!("{{\"type\":\"pong\",\"id\":{}}}", id)),
        }
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let trimmed = json.trim();
        if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
            return Err("Invalid JSON object".to_string());
        }

        let extract_field = |name: &str| -> Option<String> {
            let pattern = format!("\"{}\"", name);
            let start = trimmed.find(&pattern)?;
            let after_name = &trimmed[start + pattern.len()..];
            // Find the value after ':'
            let colon = after_name.find(':')?;
            let after_colon = after_name[colon + 1..].trim();

            // Handle quoted strings
            if after_colon.starts_with('"') {
                let end = after_colon[1..].find('"')?;
                Some(after_colon[1..1 + end].to_string())
            } else {
                // Handle numbers and arrays
                let end = after_colon.find(|c: char| c == ',' || c == '}' || c == ']').unwrap_or(after_colon.len());
                Some(after_colon[..end].trim().to_string())
            }
        };

        let extract_array = |name: &str| -> Option<Vec<f32>> {
            let pattern = format!("\"{}\"", name);
            let start = trimmed.find(&pattern)?;
            let after_name = &trimmed[start + pattern.len()..];
            let colon = after_name.find(':')?;
            let after_colon = after_name[colon + 1..].trim();

            if !after_colon.starts_with('[') { return None; }
            let end = after_colon.find(']')?;
            let content = after_colon[1..end].trim();
            if content.is_empty() { return Some(Vec::new()); }

            Some(content.split(',')
                .filter_map(|s| s.trim().parse::<f32>().ok())
                .collect())
        };

        let extract_usize_array = |name: &str| -> Option<Vec<usize>> {
            let pattern = format!("\"{}\"", name);
            let start = trimmed.find(&pattern)?;
            let after_name = &trimmed[start + pattern.len()..];
            let colon = after_name.find(':')?;
            let after_colon = after_name[colon + 1..].trim();

            if !after_colon.starts_with('[') { return None; }
            let end = after_colon.find(']')?;
            let content = after_colon[1..end].trim();
            if content.is_empty() { return Some(Vec::new()); }

            Some(content.split(',')
                .filter_map(|s| s.trim().parse::<usize>().ok())
                .collect())
        };

        let msg_type = extract_field("type").unwrap_or_default();

        match msg_type.as_str() {
            "predict" => {
                let id = extract_field("id").and_then(|s| s.parse().ok()).unwrap_or(0);
                let shape = extract_usize_array("shape").unwrap_or_default();
                let data = extract_array("data").unwrap_or_default();
                Ok(DistributedMessage::PredictRequest { id, input_shape: shape, input_data: data })
            }
            "predict_resp" => {
                let id = extract_field("id").and_then(|s| s.parse().ok()).unwrap_or(0);
                let shape = extract_usize_array("shape").unwrap_or_default();
                let data = extract_array("data").unwrap_or_default();
                let timing_ms = extract_field("timing_ms").and_then(|s| s.parse().ok()).unwrap_or(0.0);
                Ok(DistributedMessage::PredictResponse { id, output_shape: shape, output_data: data, timing_ms })
            }
            "model_info" => {
                let id = extract_field("id").and_then(|s| s.parse().ok()).unwrap_or(0);
                Ok(DistributedMessage::ModelInfoRequest { id })
            }
            "model_info_resp" => {
                let id = extract_field("id").and_then(|s| s.parse().ok()).unwrap_or(0);
                let layers = extract_field("layers").and_then(|s| s.parse().ok()).unwrap_or(0);
                let params = extract_field("params").and_then(|s| s.parse().ok()).unwrap_or(0);
                // Extract names array properly: find "names":[....]
                let names_pattern = "\"names\":";
                let mut names = Vec::new();
                if let Some(ns_start) = trimmed.find(names_pattern) {
                    let after = &trimmed[ns_start + names_pattern.len()..].trim();
                    if after.starts_with('[') {
                        let end = after.find(']').unwrap_or(after.len());
                        let inner = &after[1..end];
                        for part in inner.split(',') {
                            let p = part.trim().trim_matches('"');
                            if !p.is_empty() {
                                names.push(p.to_string());
                            }
                        }
                    }
                }
                Ok(DistributedMessage::ModelInfoResponse { id, num_layers: layers, num_parameters: params, layer_names: names })
            }
            "error" => {
                let id = extract_field("id").and_then(|s| s.parse().ok()).unwrap_or(0);
                let message = extract_field("msg").unwrap_or_default();
                Ok(DistributedMessage::Error { id, message })
            }
            "ping" => {
                let id = extract_field("id").and_then(|s| s.parse().ok()).unwrap_or(0);
                Ok(DistributedMessage::Ping(id))
            }
            "pong" => {
                let id = extract_field("id").and_then(|s| s.parse().ok()).unwrap_or(0);
                Ok(DistributedMessage::Pong(id))
            }
            _ => Err(format!("Unknown message type: '{}'", msg_type)),
        }
    }

    /// Get the message ID.
    pub fn id(&self) -> u64 {
        match self {
            DistributedMessage::PredictRequest { id, .. } => *id,
            DistributedMessage::PredictResponse { id, .. } => *id,
            DistributedMessage::ModelInfoRequest { id, .. } => *id,
            DistributedMessage::ModelInfoResponse { id, .. } => *id,
            DistributedMessage::Error { id, .. } => *id,
            DistributedMessage::Ping(id) => *id,
            DistributedMessage::Pong(id) => *id,
        }
    }
}

impl std::fmt::Display for DistributedMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistributedMessage::PredictRequest { id, input_shape, .. } =>
                write!(f, "PredictRequest(id={}, shape={:?})", id, input_shape),
            DistributedMessage::PredictResponse { id, output_shape, timing_ms, .. } =>
                write!(f, "PredictResponse(id={}, shape={:?}, t={:.2}ms)", id, output_shape, timing_ms),
            DistributedMessage::ModelInfoRequest { id } =>
                write!(f, "ModelInfoRequest(id={})", id),
            DistributedMessage::ModelInfoResponse { id, num_layers, num_parameters, .. } =>
                write!(f, "ModelInfoResponse(id={}, {} layers, {} params)", id, num_layers, num_parameters),
            DistributedMessage::Error { id, message } =>
                write!(f, "Error(id={}, '{}')", id, message),
            DistributedMessage::Ping(id) => write!(f, "Ping({})", id),
            DistributedMessage::Pong(id) => write!(f, "Pong({})", id),
        }
    }
}

// ---------------------------------------------------------------------------
// Frame-level I/O
// ---------------------------------------------------------------------------

/// Read one message frame from a stream (length-prefixed).
pub fn read_message(stream: &mut impl Read) -> Result<DistributedMessage, String> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).map_err(|e| format!("Read length error: {}", e))?;
    let len = u32::from_be_bytes(len_buf) as usize;

    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).map_err(|e| format!("Read data error: {}", e))?;

    let json_str = std::str::from_utf8(&data).map_err(|e| format!("UTF-8 error: {}", e))?;
    DistributedMessage::from_json(json_str)
}

/// Write one message frame to a stream (length-prefixed JSON).
pub fn write_message(stream: &mut impl Write, msg: &DistributedMessage) -> Result<(), String> {
    let bytes = msg.to_bytes()?;
    stream.write_all(&bytes).map_err(|e| format!("Write error: {}", e))?;
    stream.flush().map_err(|e| format!("Flush error: {}", e))?;
    Ok(())
}

/// Current message ID counter (atomic for thread safety).
pub fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_roundtrip_predict() {
        let msg = DistributedMessage::PredictRequest {
            id: 42,
            input_shape: vec![1, 4],
            input_data: vec![1.0, 2.0, 3.0, 4.0],
        };
        let json = msg.to_json().unwrap();
        let parsed = DistributedMessage::from_json(&json).unwrap();
        assert_eq!(parsed.id(), 42);
        if let DistributedMessage::PredictRequest { input_shape, input_data, .. } = parsed {
            assert_eq!(input_shape, vec![1, 4]);
            assert!((input_data[0] - 1.0).abs() < 0.001);
        } else { panic!("Wrong type"); }
    }

    #[test]
    fn test_message_roundtrip_bytes() {
        let msg = DistributedMessage::Ping(7);
        let bytes = msg.to_bytes().unwrap();
        let (parsed, consumed) = DistributedMessage::from_bytes(&bytes).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(parsed.id(), 7);
        assert_eq!(parsed, DistributedMessage::Ping(7));
    }

    #[test]
    fn test_error_message() {
        let msg = DistributedMessage::Error { id: 1, message: "something broke".to_string() };
        let json = msg.to_json().unwrap();
        let parsed = DistributedMessage::from_json(&json).unwrap();
        if let DistributedMessage::Error { message, .. } = parsed {
            assert_eq!(message, "something broke");
        } else { panic!("Wrong type"); }
    }

    #[test]
    fn test_model_info() {
        let msg = DistributedMessage::ModelInfoResponse {
            id: 1, num_layers: 3, num_parameters: 100, layer_names: vec!["Linear".to_string(), "ReLU".to_string()]
        };
        let json = msg.to_json().unwrap();
        let parsed = DistributedMessage::from_json(&json).unwrap();
        if let DistributedMessage::ModelInfoResponse { num_layers, num_parameters, layer_names, .. } = parsed {
            assert_eq!(num_layers, 3);
            assert_eq!(num_parameters, 100);
            assert_eq!(layer_names.len(), 2);
        } else { panic!("Wrong type"); }
    }

    #[test]
    fn test_next_id_unique() {
        let a = next_id();
        let b = next_id();
        assert_ne!(a, b);
    }

    #[test]
    fn test_predict_response() {
        let msg = DistributedMessage::PredictResponse {
            id: 1, output_shape: vec![2], output_data: vec![0.8, 0.2], timing_ms: 1.5,
        };
        let json = msg.to_json().unwrap();
        let parsed = DistributedMessage::from_json(&json).unwrap();
        if let DistributedMessage::PredictResponse { output_shape, output_data, timing_ms, .. } = parsed {
            assert_eq!(output_shape, vec![2]);
            assert!((timing_ms - 1.5).abs() < 0.01);
        } else { panic!("Wrong type"); }
    }
}
