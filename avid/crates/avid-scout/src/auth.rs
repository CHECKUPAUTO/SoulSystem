#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
use reqwest::header::HeaderValue;
use soulsystem_common::secrets::SecretString;

/// Authentication method for protected pages.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    Basic {
        username: String,
        password: SecretString,
    },
    Bearer {
        token: SecretString,
    },
}

/// Build an Authorization header value from an auth method.
impl AuthMethod {
    pub fn to_header_value(&self) -> HeaderValue {
        match self {
            Self::Basic { username, password } => {
                let creds = base64::encode(format!("{username}:{}", password.expose()));
                HeaderValue::from_str(&format!("Basic {creds}")).expect("valid header")
            }
            Self::Bearer { token } => {
                // Was `{token}` via the redacting Display: sent "Bearer [REDACTED]"
                // and authentication silently failed (MED-017).
                HeaderValue::from_str(&format!("Bearer {}", token.expose())).expect("valid header")
            }
        }
    }
}

/// Simple base64 encoder (no external crate needed for basic auth).
mod base64 {
    pub fn encode(input: String) -> String {
        let bytes = input.into_bytes();
        base64_impl(&bytes)
    }

    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn base64_impl(bytes: &[u8]) -> String {
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = match chunk.len() {
                3 => [chunk[0], chunk[1], chunk[2]],
                2 => [chunk[0], chunk[1], 0],
                1 => [chunk[0], 0, 0],
                _ => break,
            };
            let n = (usize::from(b[0]) << 16) | (usize::from(b[1]) << 8) | usize::from(b[2]);
            out.push(CHARS[(n >> 18) & 0x3F] as char);
            out.push(CHARS[(n >> 12) & 0x3F] as char);
            out.push(if chunk.len() > 1 {
                CHARS[(n >> 6) & 0x3F]
            } else {
                b'='
            } as char);
            out.push(if chunk.len() > 2 {
                CHARS[n & 0x3F]
            } else {
                b'='
            } as char);
        }
        out
    }
}
