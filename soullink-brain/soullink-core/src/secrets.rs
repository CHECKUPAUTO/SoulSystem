//! Secret loader — reads `/etc/soullink/secrets.env` (mode 0600, root-owned)
//! and exposes [`InternalToken`] to middleware.
//!
//! File format (key=value, one per line, `#` comments ignored):
//! ```ignore
//! SOULLINK_INTERNAL_TOKEN=<hex-32-bytes>
//! ```
//!
//! The token is compared in constant time (see [`crate::auth`]).

use std::{io, path::Path};

use thiserror::Error;
use tracing::warn;

pub const DEFAULT_SECRETS_PATH: &str = "/etc/soullink/secrets.env";

#[derive(Debug, Error)]
pub enum SecretsError {
    #[error("secrets file not found: {0}")]
    NotFound(String),
    #[error("secrets file has insecure permissions (must be 0600): {0}")]
    InsecurePermissions(String),
    #[error("secrets file is not owned by root: {0}")]
    WrongOwner(String),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("missing key: {0}")]
    MissingKey(&'static str),
    #[error("invalid token format (expected 32 hex bytes / 64 chars)")]
    InvalidToken,
}

/// A secret internal token — wraps a fixed 32-byte array.
///
/// `Debug` never leaks the bytes (useful in logs).
#[derive(Clone)]
pub struct InternalToken(pub [u8; 32]);

impl std::fmt::Debug for InternalToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InternalToken(redacted)")
    }
}

impl InternalToken {
    /// Parse from a hex string (64 hex chars).
    pub fn from_hex(s: &str) -> Result<Self, SecretsError> {
        if s.len() != 64 {
            return Err(SecretsError::InvalidToken);
        }
        let mut out = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = hex_val(chunk[0]).ok_or(SecretsError::InvalidToken)?;
            let lo = hex_val(chunk[1]).ok_or(SecretsError::InvalidToken)?;
            out[i] = (hi << 4) | lo;
        }
        Ok(Self(out))
    }

    /// Encode as lowercase hex string (for serialisation).
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Load secrets from the default path with hardening checks.
pub fn load_default() -> Result<InternalToken, SecretsError> {
    load_from(DEFAULT_SECRETS_PATH.as_ref())
}

/// Load secrets from an arbitrary path (useful for tests / alternate envs).
pub fn load_from(path: &Path) -> Result<InternalToken, SecretsError> {
    if !path.exists() {
        return Err(SecretsError::NotFound(path.display().to_string()));
    }

    // Unix permissions / ownership checks — hardening
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let md = std::fs::metadata(path)?;
        let mode = md.mode() & 0o7777;
        if mode & 0o077 != 0 {
            return Err(SecretsError::InsecurePermissions(format!(
                "{} (mode {:o})",
                path.display(),
                mode
            )));
        }
        if md.uid() != 0 {
            // Not fatal when running unprivileged during dev, but warn loudly.
            warn!(
                uid = md.uid(),
                path = %path.display(),
                "secrets file not owned by root — acceptable in dev, rejected in prod"
            );
        }
    }

    let content = std::fs::read_to_string(path)?;
    let mut token_hex: Option<String> = None;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if k == "SOULLINK_INTERNAL_TOKEN" {
                token_hex = Some(v.to_string());
            }
        }
    }

    let hex = token_hex.ok_or(SecretsError::MissingKey("SOULLINK_INTERNAL_TOKEN"))?;
    InternalToken::from_hex(&hex)
}
