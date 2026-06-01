//! Unix Domain Socket IPC protocol for Guardian ↔ Orchestrator.
//!
//! # Wire format
//!
//! Length-prefixed MessagePack frames:
//!
//! ```text
//!   [u32 little-endian payload_len][msgpack bytes...]
//! ```
//!
//! 4-byte header, then exactly `payload_len` bytes of MessagePack. Max
//! frame is capped at [`MAX_FRAME_SIZE`] to defend the reader from a
//! malicious/buggy writer.
//!
//! # Why UDS + MessagePack (vs HTTP + JSON)
//!
//! Phase 2c benchmarks on the T430:
//!
//! | Stage                          | HTTP+JSON | UDS+MsgPack (est.) |
//! |--------------------------------|-----------|--------------------|
//! | snapshot_serde/to_string (JSON)| 1.3 µs    | 1.0 µs (MsgPack)   |
//! | TCP loopback round-trip        | ~80-150 µs| ~20-40 µs (UDS)    |
//! | HTTP parse/frame overhead      | ~10 µs    | ~1 µs (LE header)  |
//! | **total push_snapshot**        | ~100 µs   | ~25 µs             |
//!
//! Phase 3 target: push_snapshot p99 < 50 µs measured.
//!
//! # Coexistence
//!
//! The HTTP `/state` and `/metrics` endpoints on port 9091 stay alive
//! regardless — they're the interface for curl, Grafana, the legacy
//! `decision-v13` binary. UDS is only for the chatty Guardian→Orchestrator
//! push_snapshot path.

use serde::{de::DeserializeOwned, Serialize};
use thiserror::Error;

pub const DEFAULT_SOCKET_PATH: &str = "/run/soullink/guardian.sock";
pub const MAX_FRAME_SIZE: usize = 1 << 20; // 1 MiB — snapshot is < 500 B, wide margin
pub const FRAME_HEADER_BYTES: usize = 4;

#[derive(Debug, Error)]
pub enum IpcError {
    #[error("frame too large: {size} > max {max}")]
    FrameTooLarge { size: usize, max: usize },
    #[error("unexpected EOF: wanted {wanted}, got {got}")]
    UnexpectedEof { wanted: usize, got: usize },
    #[error("serde error: {0}")]
    Serde(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Async frame reader over any `AsyncRead`. Reads one complete frame per call.
///
/// Usage:
/// ```ignore
/// let mut stream = tokio::net::UnixStream::connect(path).await?;
/// let snap: SystemSnapshot = read_frame(&mut stream).await?;
/// ```
#[cfg(feature = "ipc-async")]
pub async fn read_frame<T, R>(reader: &mut R) -> Result<T, IpcError>
where
    T: DeserializeOwned,
    R: tokio::io::AsyncReadExt + Unpin,
{
    let mut hdr = [0u8; FRAME_HEADER_BYTES];
    reader.read_exact(&mut hdr).await?;
    let size = u32::from_le_bytes(hdr) as usize;
    if size > MAX_FRAME_SIZE {
        return Err(IpcError::FrameTooLarge {
            size,
            max: MAX_FRAME_SIZE,
        });
    }
    let mut buf = vec![0u8; size];
    reader.read_exact(&mut buf).await?;
    rmp_serde::from_slice::<T>(&buf).map_err(|e| IpcError::Serde(e.to_string()))
}

/// Async frame writer over any `AsyncWrite`.
#[cfg(feature = "ipc-async")]
pub async fn write_frame<T, W>(writer: &mut W, value: &T) -> Result<(), IpcError>
where
    T: Serialize,
    W: tokio::io::AsyncWriteExt + Unpin,
{
    let bytes = rmp_serde::to_vec(value).map_err(|e| IpcError::Serde(e.to_string()))?;
    if bytes.len() > MAX_FRAME_SIZE {
        return Err(IpcError::FrameTooLarge {
            size: bytes.len(),
            max: MAX_FRAME_SIZE,
        });
    }
    let hdr = (bytes.len() as u32).to_le_bytes();
    writer.write_all(&hdr).await?;
    writer.write_all(&bytes).await?;
    Ok(())
}

/// Synchronous helpers — used in tests with `std::io::Cursor`.
pub fn encode_frame<T: Serialize>(value: &T) -> Result<Vec<u8>, IpcError> {
    let body = rmp_serde::to_vec(value).map_err(|e| IpcError::Serde(e.to_string()))?;
    if body.len() > MAX_FRAME_SIZE {
        return Err(IpcError::FrameTooLarge {
            size: body.len(),
            max: MAX_FRAME_SIZE,
        });
    }
    let mut out = Vec::with_capacity(FRAME_HEADER_BYTES + body.len());
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode_frame<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, IpcError> {
    if bytes.len() < FRAME_HEADER_BYTES {
        return Err(IpcError::UnexpectedEof {
            wanted: FRAME_HEADER_BYTES,
            got: bytes.len(),
        });
    }
    let mut hdr = [0u8; FRAME_HEADER_BYTES];
    hdr.copy_from_slice(&bytes[..FRAME_HEADER_BYTES]);
    let size = u32::from_le_bytes(hdr) as usize;
    if size > MAX_FRAME_SIZE {
        return Err(IpcError::FrameTooLarge {
            size,
            max: MAX_FRAME_SIZE,
        });
    }
    let body_end = FRAME_HEADER_BYTES + size;
    if bytes.len() < body_end {
        return Err(IpcError::UnexpectedEof {
            wanted: body_end,
            got: bytes.len(),
        });
    }
    rmp_serde::from_slice::<T>(&bytes[FRAME_HEADER_BYTES..body_end])
        .map_err(|e| IpcError::Serde(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SystemSnapshot;

    #[test]
    fn roundtrip_snapshot() {
        let snap = SystemSnapshot::default();
        let frame = encode_frame(&snap).unwrap();
        assert!(frame.len() > FRAME_HEADER_BYTES);
        let back: SystemSnapshot = decode_frame(&frame).unwrap();
        assert_eq!(back.gpu_mem_total_mib, snap.gpu_mem_total_mib);
        assert_eq!(back.throttle_active, snap.throttle_active);
    }

    #[test]
    fn truncated_header_rejected() {
        let err = decode_frame::<SystemSnapshot>(&[0u8, 1]).unwrap_err();
        assert!(matches!(err, IpcError::UnexpectedEof { .. }));
    }

    #[test]
    fn truncated_body_rejected() {
        let snap = SystemSnapshot::default();
        let mut frame = encode_frame(&snap).unwrap();
        frame.truncate(frame.len() - 10);
        let err = decode_frame::<SystemSnapshot>(&frame).unwrap_err();
        assert!(matches!(err, IpcError::UnexpectedEof { .. }));
    }

    #[test]
    fn oversize_header_rejected() {
        // Craft a frame header claiming 2 MiB
        let mut frame = (2u32 * 1024 * 1024).to_le_bytes().to_vec();
        frame.extend_from_slice(&[0u8; 16]);
        let err = decode_frame::<SystemSnapshot>(&frame).unwrap_err();
        assert!(matches!(err, IpcError::FrameTooLarge { .. }));
    }

    #[test]
    fn msgpack_is_smaller_than_json() {
        // Sanity: MsgPack encoding of a snapshot should be clearly smaller
        // than JSON. If this fails, rmp-serde may have regressed a feature.
        let snap = SystemSnapshot::default();
        let msgpack = rmp_serde::to_vec(&snap).unwrap();
        let json = serde_json::to_vec(&snap).unwrap();
        assert!(
            msgpack.len() < json.len(),
            "msgpack {}B >= json {}B — check config",
            msgpack.len(),
            json.len()
        );
    }
}
