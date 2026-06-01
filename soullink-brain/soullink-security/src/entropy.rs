//! Shannon entropy calculation for detecting high-entropy strings (secrets).

/// Calculate Shannon entropy of a byte string.
/// Returns a value between 0.0 (constant) and ~8.0 (random).
///
/// Thresholds:
/// - < 3.0: Likely natural language text
/// - 3.0-4.5: Could be encoded data
/// - > 4.5: Likely a secret (API key, token, password)
/// - > 6.0: Almost certainly a secret
pub fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut freq = [0usize; 256];
    for &byte in data {
        freq[byte as usize] += 1;
    }

    let len = data.len() as f64;
    let mut entropy = 0.0;

    for &count in &freq {
        if count == 0 {
            continue;
        }
        let p = count as f64 / len;
        entropy -= p * p.log2();
    }

    entropy
}

/// Check if a string has high enough entropy to be a potential secret.
pub fn is_high_entropy(s: &str, threshold: f64) -> bool {
    shannon_entropy(s.as_bytes()) >= threshold
}

/// Check if a string looks like a Base64-encoded secret.
pub fn is_base64_secret(s: &str) -> bool {
    // Must be at least 20 chars and high entropy
    if s.len() < 20 {
        return false;
    }
    // Check if it's valid base64 chars
    let is_b64 = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
    is_b64 && is_high_entropy(s, 4.5)
}

/// Check if a string looks like a hex-encoded secret.
pub fn is_hex_secret(s: &str) -> bool {
    if s.len() < 20 {
        return false;
    }
    let is_hex = s.chars().all(|c| c.is_ascii_hexdigit());
    is_hex && is_high_entropy(s, 3.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_low_entropy() {
        assert!(shannon_entropy(b"aaaaaaa") < 1.0);
        assert!(shannon_entropy(b"hello world") < 4.0);
    }

    #[test]
    fn test_high_entropy() {
        let secret = "AKIAIOSFODNN7EXAMPLE"; // AWS key pattern
        assert!(shannon_entropy(secret.as_bytes()) > 3.0);
        let random = "dGhpcyBpcyBhIHNlY3JldCB0b2tlbiBmb3IgdGVzdGluZw==";
        assert!(shannon_entropy(random.as_bytes()) > 4.0);
    }

    #[test]
    fn test_is_high_entropy() {
        assert!(!is_high_entropy("hello world", 4.5));
        assert!(is_high_entropy(
            "dGhpcyBpcyBhIHNlY3JldCB0b2tlbiBmb3IgdGVzdGluZw==",
            4.5
        ));
    }

    #[test]
    fn test_base64_secret() {
        assert!(is_base64_secret(
            "dGhpcyBpcyBhIHNlY3JldCB0b2tlbiBmb3IgdGVzdGluZw=="
        ));
        assert!(!is_base64_secret("short"));
        assert!(!is_base64_secret(
            "hello world this is not base64 at all really"
        ));
    }

    #[test]
    fn test_hex_secret() {
        assert!(is_hex_secret("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6"));
        assert!(!is_hex_secret("short"));
    }
}
