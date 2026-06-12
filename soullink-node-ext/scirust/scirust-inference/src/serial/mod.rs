//! Model serialization/deserialization.
//!
//! Supports:
//! - SafeTensors format (safe, fast)
//! - JSON format (readable, portable)
//! - Binary format (compact, fast)

use crate::tensor::Tensor;
use std::collections::HashMap;
use std::io::{Read, Write};

/// A named tensor for serialization.
#[derive(Debug, Clone)]
pub struct NamedTensor {
    pub name: String,
    pub data: Tensor<f32>,
}

/// Serialized model containing all layer parameters.
#[derive(Debug, Clone)]
pub struct ModelState {
    pub tensors: HashMap<String, Tensor<f32>>,
    pub metadata: HashMap<String, String>,
}

impl ModelState {
    pub fn new() -> Self {
        Self {
            tensors: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata key-value pair.
    pub fn set_meta(&mut self, key: &str, value: &str) {
        self.metadata.insert(key.to_string(), value.to_string());
    }

    /// Get metadata value.
    pub fn get_meta(&self, key: &str) -> Option<&String> {
        self.metadata.get(key)
    }

    /// Add a named tensor.
    pub fn add_tensor(&mut self, name: &str, tensor: Tensor<f32>) {
        self.tensors.insert(name.to_string(), tensor);
    }

    /// Get a tensor by name.
    pub fn get_tensor(&self, name: &str) -> Option<&Tensor<f32>> {
        self.tensors.get(name)
    }

    /// Number of parameters.
    pub fn num_parameters(&self) -> usize {
        self.tensors.values().map(|t| t.len()).sum()
    }

    // -----------------------------------------------------------------------
    // Binary serialization (compact, fast)
    // -----------------------------------------------------------------------

    /// Save to binary format.
    pub fn save_binary<W: Write>(&self, writer: &mut W) -> Result<(), String> {
        // Magic: "SCRUST" + version
        writer.write_all(b"SCRUST01").map_err(|e| e.to_string())?;

        // Metadata count
        let meta_count = self.metadata.len() as u32;
        writer.write_all(&meta_count.to_le_bytes()).map_err(|e| e.to_string())?;

        for (key, val) in &self.metadata {
            write_str(writer, key)?;
            write_str(writer, val)?;
        }

        // Tensor count
        let tensor_count = self.tensors.len() as u32;
        writer.write_all(&tensor_count.to_le_bytes()).map_err(|e| e.to_string())?;

        for (name, tensor) in &self.tensors {
            write_str(writer, name)?;
            // Shape: rank + dims
            let rank = tensor.shape().len() as u32;
            writer.write_all(&rank.to_le_bytes()).map_err(|e| e.to_string())?;
            for &dim in tensor.shape() {
                let d = dim as u64;
                writer.write_all(&d.to_le_bytes()).map_err(|e| e.to_string())?;
            }
            // Data as f32 bytes (little-endian)
            let data_bytes: Vec<u8> = tensor
                .data()
                .iter()
                .flat_map(|&x| x.to_le_bytes())
                .collect();
            let data_len = data_bytes.len() as u64;
            writer.write_all(&data_len.to_le_bytes()).map_err(|e| e.to_string())?;
            writer.write_all(&data_bytes).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// Load from binary format.
    pub fn load_binary<R: Read>(reader: &mut R) -> Result<Self, String> {
        let mut magic = [0u8; 8];
        reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != b"SCRUST01" {
            return Err("Invalid binary format: magic mismatch".to_string());
        }

        let mut state = Self::new();

        // Metadata
        let meta_count = read_u32(reader)?;
        for _ in 0..meta_count {
            let key = read_str(reader)?;
            let val = read_str(reader)?;
            state.metadata.insert(key, val);
        }

        // Tensors
        let tensor_count = read_u32(reader)?;
        for _ in 0..tensor_count {
            let name = read_str(reader)?;
            let rank = read_u32(reader)? as usize;
            let mut shape = Vec::with_capacity(rank);
            for _ in 0..rank {
                let dim = read_u64(reader)? as usize;
                shape.push(dim);
            }
            let data_len = read_u64(reader)? as usize;
            let data_count = data_len / 4;
            let mut data = vec![0.0f32; data_count];
            let mut bytes = vec![0u8; data_len];
            reader.read_exact(&mut bytes).map_err(|e| e.to_string())?;
            for i in 0..data_count {
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&bytes[i * 4..(i + 1) * 4]);
                data[i] = f32::from_le_bytes(arr);
            }
            let tensor = Tensor::from_vec(data, &shape)?;
            state.tensors.insert(name, tensor);
        }

        Ok(state)
    }

    // -----------------------------------------------------------------------
    // JSON serialization (readable, cross-platform)
    // -----------------------------------------------------------------------

    /// Save to JSON format (requires `serde_json` feature).
    #[cfg(feature = "serialization")]
    pub fn save_json<W: Write>(&self, writer: &mut W) -> Result<(), String> {
        #[derive(serde::Serialize)]
        struct JsonModel<'a> {
            metadata: &'a HashMap<String, String>,
            tensors: Vec<JsonTensor<'a>>,
        }

        #[derive(serde::Serialize)]
        struct JsonTensor<'a> {
            name: &'a str,
            shape: &'a [usize],
            data: Vec<f32>,
        }

        let tensors: Vec<JsonTensor> = self
            .tensors
            .iter()
            .map(|(name, t)| JsonTensor {
                name,
                shape: t.shape(),
                data: t.data().to_vec(),
            })
            .collect();

        let model = JsonModel {
            metadata: &self.metadata,
            tensors,
        };

        let json = serde_json::to_string_pretty(&model).map_err(|e| e.to_string())?;
        writer.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load from JSON format.
    #[cfg(feature = "serialization")]
    pub fn load_json<R: Read>(reader: &mut R) -> Result<Self, String> {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct JsonModel {
            metadata: HashMap<String, String>,
            tensors: Vec<JsonTensor>,
        }

        #[derive(Deserialize)]
        struct JsonTensor {
            name: String,
            shape: Vec<usize>,
            data: Vec<f32>,
        }

        let mut contents = String::new();
        reader.read_to_string(&mut contents).map_err(|e| e.to_string())?;
        let model: JsonModel = serde_json::from_str(&contents).map_err(|e| e.to_string())?;

        let mut state = ModelState::new();
        state.metadata = model.metadata;

        for t in model.tensors {
            let tensor = Tensor::from_vec(t.data, &t.shape)?;
            state.tensors.insert(t.name, tensor);
        }

        Ok(state)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_str<W: Write>(writer: &mut W, s: &str) -> Result<(), String> {
    let bytes = s.as_bytes();
    let len = bytes.len() as u32;
    writer.write_all(&len.to_le_bytes()).map_err(|e| e.to_string())?;
    writer.write_all(bytes).map_err(|e| e.to_string())?;
    Ok(())
}

fn read_str<R: Read>(reader: &mut R) -> Result<String, String> {
    let len = read_u32(reader)? as usize;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
    String::from_utf8(buf).map_err(|e| e.to_string())
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64<R: Read>(reader: &mut R) -> Result<u64, String> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(u64::from_le_bytes(buf))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_binary_roundtrip() {
        let mut state = ModelState::new();
        state.set_meta("model_type", "mlp");
        state.set_meta("version", "1.0");

        let w = Tensor::<f32>::from_vec(vec![1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
        let b = Tensor::<f32>::from_vec(vec![0.5, 0.5], &[2]).unwrap();
        state.add_tensor("fc1.weight", w);
        state.add_tensor("fc1.bias", b);

        let mut buf = Vec::new();
        state.save_binary(&mut buf).unwrap();

        let mut cursor = Cursor::new(buf);
        let loaded = ModelState::load_binary(&mut cursor).unwrap();

        assert_eq!(loaded.get_meta("model_type").unwrap(), "mlp");
        assert_eq!(loaded.tensors.len(), 2);
        assert_eq!(
            loaded.get_tensor("fc1.weight").unwrap().data(),
            &[1.0, 2.0, 3.0, 4.0]
        );
    }

    #[test]
    fn test_json_roundtrip() {
        #[cfg(feature = "serialization")]
        {
            let mut state = ModelState::new();
            state.set_meta("arch", "test");
            let w = Tensor::<f32>::ones(&[2, 3]);
            state.add_tensor("weight", w);

            let mut buf = Vec::new();
            state.save_json(&mut buf).unwrap();

            let mut cursor = Cursor::new(buf);
            let loaded = ModelState::load_json(&mut cursor).unwrap();

            assert_eq!(loaded.tensors.len(), 1);
            assert!(loaded.get_tensor("weight").is_some());
        }
    }
}
