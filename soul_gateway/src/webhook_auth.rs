//! Cryptographic verification of inbound provider webhooks.
//!
//! Closes the second half of HIGH-007 / INV-NET-3. The first half (PR F) made
//! the handlers fail closed when a provider secret is unset; this module makes
//! a *configured* deployment actually check the request, which is the half that
//! defends it.
//!
//! Each provider signs differently, and the roadmap's shorthand
//! ("per-provider HMAC-SHA256") is wrong for one of them:
//!
//! | Provider | Scheme | Signature header | Timestamp header |
//! |---|---|---|---|
//! | Discord  | **Ed25519**, not HMAC | `X-Signature-Ed25519` | `X-Signature-Timestamp` |
//! | Slack    | HMAC-SHA256 | `X-Slack-Signature` (`v0=…`) | `X-Slack-Request-Timestamp` |
//! | WhatsApp | HMAC-SHA256 | `X-Hub-Signature-256` (`sha256=…`) | *(none sent)* |
//!
//! Discord signs `timestamp ‖ body` with Ed25519 and `DISCORD_PUBLIC_KEY` is a
//! hex *public key*, not a shared secret. Slack signs the base string
//! `v0:{timestamp}:{body}`. Meta signs the raw body only.
//!
//! Three properties are enforced, in this order, and every failure is the same
//! opaque rejection so a caller cannot distinguish "bad signature" from
//! "stale timestamp" and probe the difference:
//!
//! 1. **Authenticity** — the signature verifies against the configured key.
//! 2. **Freshness** — the signed timestamp is within [`MAX_SKEW`]. Providers
//!    that send no timestamp (Meta) are covered by replay protection alone.
//! 3. **Uniqueness** — the exact signature has not been accepted before, so a
//!    captured request cannot be resubmitted inside the freshness window.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// How far a signed timestamp may be from local time, in either direction.
///
/// Five minutes matches Slack's and Discord's published guidance. Allowing skew
/// in both directions tolerates a clock that is slightly ahead as well as
/// behind; the replay cache is what stops reuse inside the window.
pub const MAX_SKEW: Duration = Duration::from_secs(5 * 60);

/// Why a webhook was rejected.
///
/// Deliberately *not* surfaced to the caller: the handlers return one opaque
/// status for every variant. It exists so the server can log precisely what
/// failed without telling the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookRejection {
    /// No verification key is configured for this provider.
    NotConfigured,
    /// A required signature or timestamp header is missing or malformed.
    MalformedHeader,
    /// The signature did not verify against the configured key.
    BadSignature,
    /// The signed timestamp is outside [`MAX_SKEW`].
    StaleTimestamp,
    /// This exact signature was already accepted.
    Replayed,
}

impl WebhookRejection {
    /// Short, stable label for logs and metrics.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::MalformedHeader => "malformed_header",
            Self::BadSignature => "bad_signature",
            Self::StaleTimestamp => "stale_timestamp",
            Self::Replayed => "replayed",
        }
    }
}

/// Decode a lowercase or uppercase hex string. `None` on odd length or any
/// non-hex byte.
pub fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let nibble = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        out.push((nibble(pair[0])? << 4) | nibble(pair[1])?);
    }
    Some(out)
}

/// Remembers recently-accepted signatures so a captured request cannot be
/// replayed while its timestamp is still fresh.
///
/// Entries expire after [`MAX_SKEW`] — past that the freshness check rejects
/// the request anyway, so retaining them would only grow memory. Meta sends no
/// timestamp, so for WhatsApp this cache is the *only* replay defence; its
/// entries are kept for the same bounded window.
#[derive(Debug, Default)]
pub struct ReplayCache {
    seen: Mutex<HashMap<Vec<u8>, SystemTime>>,
}

impl ReplayCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `signature`, returning `false` if it was already present.
    ///
    /// Evicts expired entries on the way through, so the map stays bounded by
    /// the request rate within one [`MAX_SKEW`] window rather than growing
    /// without limit.
    pub fn check_and_insert(&self, signature: &[u8], now: SystemTime) -> bool {
        let mut seen = match self.seen.lock() {
            Ok(guard) => guard,
            // A poisoned lock means a previous holder panicked. Fail closed:
            // refusing is safer than accepting an unverifiable-as-fresh request.
            Err(_) => return false,
        };
        seen.retain(|_, inserted| {
            now.duration_since(*inserted)
                .map(|age| age < MAX_SKEW)
                .unwrap_or(true)
        });
        if seen.contains_key(signature) {
            return false;
        }
        seen.insert(signature.to_vec(), now);
        true
    }

    /// Number of retained entries. Test/observability aid.
    pub fn len(&self) -> usize {
        self.seen.lock().map(|s| s.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Whether `signed_at` (Unix seconds) is within [`MAX_SKEW`] of `now`.
fn timestamp_is_fresh(signed_at: i64, now: SystemTime) -> bool {
    let Ok(now_secs) = now.duration_since(UNIX_EPOCH) else {
        return false;
    };
    let now_secs = now_secs.as_secs() as i64;
    (now_secs - signed_at).unsigned_abs() <= MAX_SKEW.as_secs()
}

/// Verify a Slack request.
///
/// Base string is `v0:{timestamp}:{body}`, HMAC-SHA256 under the signing
/// secret, and the header carries it as `v0=<hex>`.
pub fn verify_slack(
    signing_secret: &str,
    timestamp_header: &str,
    signature_header: &str,
    body: &[u8],
    now: SystemTime,
) -> Result<Vec<u8>, WebhookRejection> {
    let Some(hex) = signature_header.strip_prefix("v0=") else {
        return Err(WebhookRejection::MalformedHeader);
    };
    let Some(provided) = decode_hex(hex) else {
        return Err(WebhookRejection::MalformedHeader);
    };
    let Ok(signed_at) = timestamp_header.parse::<i64>() else {
        return Err(WebhookRejection::MalformedHeader);
    };

    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .map_err(|_| WebhookRejection::NotConfigured)?;
    mac.update(b"v0:");
    mac.update(timestamp_header.as_bytes());
    mac.update(b":");
    mac.update(body);
    let expected = mac.finalize().into_bytes();

    if expected.ct_eq(provided.as_slice()).unwrap_u8() != 1 {
        return Err(WebhookRejection::BadSignature);
    }
    // Authenticity first, then freshness: an unauthenticated caller learns
    // nothing about our clock.
    if !timestamp_is_fresh(signed_at, now) {
        return Err(WebhookRejection::StaleTimestamp);
    }
    Ok(provided)
}

/// Verify a Meta/WhatsApp request.
///
/// HMAC-SHA256 over the raw body under the app secret, header formatted as
/// `sha256=<hex>`. Meta sends no timestamp, so freshness cannot be checked and
/// replay protection carries that weight alone.
pub fn verify_meta(
    app_secret: &str,
    signature_header: &str,
    body: &[u8],
) -> Result<Vec<u8>, WebhookRejection> {
    let Some(hex) = signature_header.strip_prefix("sha256=") else {
        return Err(WebhookRejection::MalformedHeader);
    };
    let Some(provided) = decode_hex(hex) else {
        return Err(WebhookRejection::MalformedHeader);
    };

    let mut mac = HmacSha256::new_from_slice(app_secret.as_bytes())
        .map_err(|_| WebhookRejection::NotConfigured)?;
    mac.update(body);
    let expected = mac.finalize().into_bytes();

    if expected.ct_eq(provided.as_slice()).unwrap_u8() != 1 {
        return Err(WebhookRejection::BadSignature);
    }
    Ok(provided)
}

/// Verify a Discord interaction request.
///
/// Discord uses **Ed25519**, not HMAC: `DISCORD_PUBLIC_KEY` is a hex-encoded
/// public key and the signature covers `timestamp ‖ body`. Verification is
/// inherently constant-time in the signature scheme, so no extra comparison is
/// needed.
pub fn verify_discord(
    public_key_hex: &str,
    timestamp_header: &str,
    signature_header: &str,
    body: &[u8],
    now: SystemTime,
) -> Result<Vec<u8>, WebhookRejection> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let Some(key_bytes) = decode_hex(public_key_hex) else {
        return Err(WebhookRejection::NotConfigured);
    };
    let key_bytes: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| WebhookRejection::NotConfigured)?;
    let Ok(verifying_key) = VerifyingKey::from_bytes(&key_bytes) else {
        return Err(WebhookRejection::NotConfigured);
    };

    let Some(sig_bytes) = decode_hex(signature_header) else {
        return Err(WebhookRejection::MalformedHeader);
    };
    let sig_bytes: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| WebhookRejection::MalformedHeader)?;
    let signature = Signature::from_bytes(&sig_bytes);
    let Ok(signed_at) = timestamp_header.parse::<i64>() else {
        return Err(WebhookRejection::MalformedHeader);
    };

    let mut signed = Vec::with_capacity(timestamp_header.len() + body.len());
    signed.extend_from_slice(timestamp_header.as_bytes());
    signed.extend_from_slice(body);

    if verifying_key.verify(&signed, &signature).is_err() {
        return Err(WebhookRejection::BadSignature);
    }
    if !timestamp_is_fresh(signed_at, now) {
        return Err(WebhookRejection::StaleTimestamp);
    }
    Ok(sig_bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn slack_signature(secret: &str, ts: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(b"v0:");
        mac.update(ts.as_bytes());
        mac.update(b":");
        mac.update(body);
        format!("v0={}", hex_encode(&mac.finalize().into_bytes()))
    }

    fn meta_signature(secret: &str, body: &[u8]) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex_encode(&mac.finalize().into_bytes()))
    }

    fn now_secs() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    #[test]
    fn decode_hex_rejects_malformed_input() {
        assert_eq!(decode_hex("00ff"), Some(vec![0x00, 0xff]));
        assert_eq!(decode_hex("00FF"), Some(vec![0x00, 0xff]));
        assert_eq!(decode_hex(""), Some(vec![]));
        assert_eq!(decode_hex("abc"), None, "odd length");
        assert_eq!(decode_hex("zz"), None, "non-hex");
        assert_eq!(decode_hex("00 ff"), None, "whitespace");
    }

    // ── Slack ────────────────────────────────────────────────────────────

    #[test]
    fn slack_accepts_a_valid_signature() {
        let secret = "slack-signing-secret";
        let body = br#"{"type":"event_callback"}"#;
        let ts = now_secs().to_string();
        let sig = slack_signature(secret, &ts, body);

        assert!(verify_slack(secret, &ts, &sig, body, SystemTime::now()).is_ok());
    }

    #[test]
    fn slack_rejects_a_forged_signature() {
        let body = br#"{"type":"event_callback"}"#;
        let ts = now_secs().to_string();
        // Signed with a different secret: the classic forgery attempt.
        let sig = slack_signature("attacker-secret", &ts, body);

        assert_eq!(
            verify_slack("real-secret", &ts, &sig, body, SystemTime::now()),
            Err(WebhookRejection::BadSignature)
        );
    }

    #[test]
    fn slack_rejects_a_tampered_body() {
        let secret = "slack-signing-secret";
        let ts = now_secs().to_string();
        let sig = slack_signature(secret, &ts, b"original");

        assert_eq!(
            verify_slack(secret, &ts, &sig, b"tampered", SystemTime::now()),
            Err(WebhookRejection::BadSignature)
        );
    }

    #[test]
    fn slack_rejects_a_stale_timestamp() {
        let secret = "slack-signing-secret";
        let body = br#"{"type":"event_callback"}"#;
        // Correctly signed, but from well outside the freshness window.
        let ts = (now_secs() - (MAX_SKEW.as_secs() as i64) - 60).to_string();
        let sig = slack_signature(secret, &ts, body);

        assert_eq!(
            verify_slack(secret, &ts, &sig, body, SystemTime::now()),
            Err(WebhookRejection::StaleTimestamp)
        );
    }

    #[test]
    fn slack_rejects_malformed_headers() {
        let secret = "s";
        let ts = now_secs().to_string();
        // Missing the v0= prefix.
        assert_eq!(
            verify_slack(secret, &ts, "deadbeef", b"{}", SystemTime::now()),
            Err(WebhookRejection::MalformedHeader)
        );
        // Non-hex signature.
        assert_eq!(
            verify_slack(secret, &ts, "v0=nothex", b"{}", SystemTime::now()),
            Err(WebhookRejection::MalformedHeader)
        );
        // Non-numeric timestamp.
        let sig = slack_signature(secret, "not-a-number", b"{}");
        assert_eq!(
            verify_slack(secret, "not-a-number", &sig, b"{}", SystemTime::now()),
            Err(WebhookRejection::MalformedHeader)
        );
    }

    // ── Meta / WhatsApp ──────────────────────────────────────────────────

    #[test]
    fn meta_accepts_a_valid_signature_and_rejects_forgery_and_tampering() {
        let secret = "meta-app-secret";
        let body = br#"{"object":"whatsapp_business_account"}"#;

        assert!(verify_meta(secret, &meta_signature(secret, body), body).is_ok());

        assert_eq!(
            verify_meta(secret, &meta_signature("wrong", body), body),
            Err(WebhookRejection::BadSignature)
        );
        assert_eq!(
            verify_meta(secret, &meta_signature(secret, b"other"), body),
            Err(WebhookRejection::BadSignature)
        );
        assert_eq!(
            verify_meta(secret, "sha1=deadbeef", body),
            Err(WebhookRejection::MalformedHeader),
            "only the sha256= prefix is accepted"
        );
    }

    // ── Discord (Ed25519) ────────────────────────────────────────────────

    #[test]
    fn discord_accepts_a_valid_ed25519_signature_and_rejects_a_forged_one() {
        use ed25519_dalek::{Signer, SigningKey};

        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let public_hex = hex_encode(signing_key.verifying_key().as_bytes());
        let body = br#"{"type":1}"#;
        let ts = now_secs().to_string();

        let mut signed = ts.as_bytes().to_vec();
        signed.extend_from_slice(body);
        let sig_hex = hex_encode(&signing_key.sign(&signed).to_bytes());

        assert!(verify_discord(&public_hex, &ts, &sig_hex, body, SystemTime::now()).is_ok());

        // A signature from a different key must not verify.
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let forged = hex_encode(&other.sign(&signed).to_bytes());
        assert_eq!(
            verify_discord(&public_hex, &ts, &forged, body, SystemTime::now()),
            Err(WebhookRejection::BadSignature)
        );

        // Tampering with the body invalidates the signature.
        assert_eq!(
            verify_discord(
                &public_hex,
                &ts,
                &sig_hex,
                b"{\"type\":2}",
                SystemTime::now()
            ),
            Err(WebhookRejection::BadSignature)
        );
    }

    #[test]
    fn discord_rejects_a_stale_timestamp_even_with_a_valid_signature() {
        use ed25519_dalek::{Signer, SigningKey};

        let signing_key = SigningKey::from_bytes(&[7u8; 32]);
        let public_hex = hex_encode(signing_key.verifying_key().as_bytes());
        let body = br#"{"type":1}"#;
        let ts = (now_secs() - (MAX_SKEW.as_secs() as i64) - 60).to_string();

        let mut signed = ts.as_bytes().to_vec();
        signed.extend_from_slice(body);
        let sig_hex = hex_encode(&signing_key.sign(&signed).to_bytes());

        assert_eq!(
            verify_discord(&public_hex, &ts, &sig_hex, body, SystemTime::now()),
            Err(WebhookRejection::StaleTimestamp)
        );
    }

    #[test]
    fn discord_rejects_a_malformed_public_key() {
        assert_eq!(
            verify_discord("nothex", "0", "00", b"{}", SystemTime::now()),
            Err(WebhookRejection::NotConfigured)
        );
        // Right encoding, wrong length.
        assert_eq!(
            verify_discord("00ff", "0", "00", b"{}", SystemTime::now()),
            Err(WebhookRejection::NotConfigured)
        );
    }

    // ── Replay ───────────────────────────────────────────────────────────

    #[test]
    fn replay_cache_accepts_once_then_refuses() {
        let cache = ReplayCache::new();
        let now = SystemTime::now();
        let sig = b"signature-bytes";

        assert!(
            cache.check_and_insert(sig, now),
            "first use must be allowed"
        );
        assert!(
            !cache.check_and_insert(sig, now),
            "the same signature must not be accepted twice"
        );
        // A different signature is unaffected.
        assert!(cache.check_and_insert(b"another", now));
    }

    #[test]
    fn replay_cache_evicts_entries_older_than_the_freshness_window() {
        let cache = ReplayCache::new();
        let old = SystemTime::now();
        assert!(cache.check_and_insert(b"old-signature", old));
        assert_eq!(cache.len(), 1);

        // Once the entry is older than MAX_SKEW it is evicted: past that point
        // the freshness check rejects the request anyway, so retaining it would
        // only grow memory unboundedly.
        let later = old + MAX_SKEW + Duration::from_secs(1);
        assert!(cache.check_and_insert(b"new-signature", later));
        assert_eq!(
            cache.len(),
            1,
            "the expired entry must be evicted, leaving only the new one"
        );
    }

    #[test]
    fn rejection_labels_are_stable() {
        assert_eq!(WebhookRejection::BadSignature.as_str(), "bad_signature");
        assert_eq!(WebhookRejection::StaleTimestamp.as_str(), "stale_timestamp");
        assert_eq!(WebhookRejection::Replayed.as_str(), "replayed");
    }
}
