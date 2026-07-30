//! Cross-platform operating-system credential storage.
//!
//! Secrets are stored by the platform backend selected by `keyring`: Keychain
//! on macOS, Credential Manager on Windows, and Secret Service on Linux.
//! Configuration files contain only stable secret names, never plaintext.

use std::fmt;

use thiserror::Error;
use zeroize::Zeroizing;

const SERVICE: &str = "dev.memorithm.soulsystem";

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("invalid secret name; use only ASCII letters, digits, '.', '-', '_' and '/'")]
    InvalidName,
    #[error("system credential store is unavailable: {0}")]
    Unavailable(String),
    #[error("secret '{0}' was not found")]
    NotFound(String),
}

/// A validated, non-secret key used to address a credential.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct SecretName(String);

impl SecretName {
    pub fn parse(value: impl Into<String>) -> Result<Self, SecretStoreError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 128
            && !value.starts_with('/')
            && !value.ends_with('/')
            && value
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/')
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(SecretStoreError::InvalidName)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SecretName").field(&self.0).finish()
    }
}

/// Thin wrapper over the native OS credential store.
pub struct SystemSecretStore;

impl SystemSecretStore {
    fn entry(name: &SecretName) -> Result<keyring::Entry, SecretStoreError> {
        keyring::Entry::new(SERVICE, name.as_str())
            .map_err(|error| SecretStoreError::Unavailable(error.to_string()))
    }

    pub fn set(name: &SecretName, value: &str) -> Result<(), SecretStoreError> {
        Self::entry(name)?
            .set_password(value)
            .map_err(|error| SecretStoreError::Unavailable(error.to_string()))
    }

    pub fn get(name: &SecretName) -> Result<Zeroizing<String>, SecretStoreError> {
        Self::entry(name)?
            .get_password()
            .map(Zeroizing::new)
            .map_err(|error| match error {
                keyring::Error::NoEntry => SecretStoreError::NotFound(name.as_str().to_owned()),
                other => SecretStoreError::Unavailable(other.to_string()),
            })
    }

    pub fn delete(name: &SecretName) -> Result<(), SecretStoreError> {
        Self::entry(name)?
            .delete_credential()
            .map_err(|error| match error {
                keyring::Error::NoEntry => SecretStoreError::NotFound(name.as_str().to_owned()),
                other => SecretStoreError::Unavailable(other.to_string()),
            })
    }
}

/// Stable credential name for an LLM provider.
pub fn llm_secret_name(provider: &str) -> Result<SecretName, SecretStoreError> {
    SecretName::parse(format!("llm/{provider}"))
}

/// Resolve an LLM credential without writing it to disk.
///
/// Explicit environment variables override the OS store, which keeps service
/// deployments deterministic while desktop installs can use the native
/// credential manager.
pub fn resolve_llm_secret(provider: &str) -> Option<Zeroizing<String>> {
    let provider_env = match provider {
        "openai" => Some("OPENAI_API_KEY"),
        "anthropic" => Some("ANTHROPIC_API_KEY"),
        _ => None,
    };
    for variable in ["SOULSYSTEM_LLM_API_KEY"].into_iter().chain(provider_env) {
        if let Ok(value) = std::env::var(variable) {
            if !value.is_empty() {
                return Some(Zeroizing::new(value));
            }
        }
    }

    let name = llm_secret_name(provider).ok()?;
    SystemSecretStore::get(&name).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_names_are_path_like_but_not_filesystem_paths() {
        assert!(SecretName::parse("llm/openai").is_ok());
        assert!(SecretName::parse("gateway/operator-1").is_ok());
        assert!(SecretName::parse("../escape").is_err());
        assert!(SecretName::parse("contains space").is_err());
        assert!(SecretName::parse("").is_err());
    }

    #[test]
    fn provider_names_are_stable() {
        assert_eq!(
            llm_secret_name("anthropic").unwrap().as_str(),
            "llm/anthropic"
        );
    }
}

// ── SecretString (INV-SEC-1 / INV-SEC-2, roadmap P1-5) ──────────────────

/// A secret held in memory, which cannot be printed by accident.
///
/// # The problem this solves
///
/// Gateway and LLM provider configuration structs held tokens in ordinary
/// `String` fields while deriving `Debug`. Every `tracing::debug!(?config)`,
/// every `unwrap()` panic message that formats a config, and every
/// `#[derive(Debug)]` on an enclosing struct then prints the credential. Nobody
/// writes `println!("{}", token)` on purpose; it leaks through the derive.
///
/// [`SecretString`] makes that impossible rather than discouraged: its `Debug`
/// and `Display` render a fixed redaction, so an enclosing `#[derive(Debug)]`
/// stays safe without its author having to think about it. Reading the value
/// requires calling [`SecretString::expose`], which is greppable in review.
///
/// # What it does not do
///
/// It is not a defence against an attacker who can read process memory, and it
/// does not stop someone writing `expose()` into a log line. It removes the
/// *accidental* path, which is the one that actually happens.
#[derive(Clone, Default, PartialEq, Eq, zeroize::ZeroizeOnDrop)]
/// Interpolating a `SecretString` with `{}` must not compile (MED-017):
///
/// ```compile_fail
/// let t = soulsystem_common::secrets::SecretString::new("tok");
/// let _ = format!("Bearer {t}");
/// ```
pub struct SecretString(String);

impl SecretString {
    /// Wrap a value. Prefer reading straight from the environment or the
    /// credential store into this type rather than through a `String` local.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Read the secret. Deliberately verbose: this is the call a reviewer
    /// greps for.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether a usable secret is present.
    ///
    /// A whitespace-only value counts as absent — a blank entry in a config
    /// file or an unset-but-exported environment variable must not become a
    /// valid credential, which is the same rule the listeners apply.
    pub fn is_present(&self) -> bool {
        !self.0.trim().is_empty()
    }

    /// Length of the underlying secret, for diagnostics that need to say
    /// "configured, 40 characters" without saying which 40.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Read from an environment variable, treating unset and blank alike.
    pub fn from_env(var: &str) -> Option<Self> {
        std::env::var(var)
            .ok()
            .map(Self::new)
            .filter(Self::is_present)
    }
}

/// The text rendered in place of a secret.
///
/// A fixed string, not a length-derived mask: `"***"` for a 4-character token
/// and `"****************"` for a 16-character one would disclose the length,
/// which narrows a brute-force search.
pub const REDACTED: &str = "<redacted>";

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Deliberately not `debug_tuple("SecretString").field(&self.0)`: that
        // is exactly the mistake this type exists to prevent.
        f.write_str(REDACTED)
    }
}

// `Display` is deliberately NOT implemented for `SecretString`.
//
// It used to print `[REDACTED]`, which is leak-proof but fails *silently
// where it matters*: `format!("Bearer {token}")` compiled cleanly and sent
// the literal string "Bearer [REDACTED]" as the credential. Three providers
// (soul_llm's OpenAI and Ollama, avid-scout's Bearer auth) shipped exactly
// that bug — authentication broken at runtime with no compile-time signal
// (MED-017). Without `Display`, interpolating a secret is a compile error,
// and the author must choose: `.expose()` to attach the real value, or
// `{:?}` for the redacted form.

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SecretString {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl<'de> serde::Deserialize<'de> for SecretString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self)
    }
}

/// Serialization is implemented but writes the **redaction**, not the secret.
///
/// A config struct is routinely serialized back out — to a debug endpoint, a
/// state dump, a snapshot on disk. Round-tripping the plaintext through those
/// paths is the same leak in a different coat. A caller that genuinely needs to
/// persist a credential must reach for [`SecretString::expose`] and say so.
impl serde::Serialize for SecretString {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(REDACTED)
    }
}

/// A credential whose serialization is its **purpose**, not a leak.
///
/// [`SecretString`] redacts in `Serialize` because a config struct that is
/// serialized is usually being dumped somewhere it should not go. A protocol
/// message is the opposite case: the whole reason `HelloOk` exists is to put a
/// device token on the wire for the client that must present it back. Using
/// `SecretString` there does not harden anything — it sends the string
/// `"<redacted>"` to the client as its token and breaks authentication at
/// runtime, silently, with no compile error to catch it.
///
/// So this type redacts `Debug` and `Display` — the accidental paths, which are
/// just as real here: `tracing::debug!(?hello)` on a handshake response leaks
/// the token it carries — while `Serialize` and `Deserialize` are faithful.
///
/// # Choosing between the two
///
/// Ask what serialization *means* for the field:
///
/// - Written to a config file, a state dump, a debug endpoint → [`SecretString`].
///   Serialization is an escape route and redacting it is protection.
/// - Written to the peer that needs the value → [`ProtocolSecret`].
///   Redacting it is a bug.
///
/// If both are true of one struct, it is carrying a credential in a place that
/// wants splitting, and neither type will save it.
#[derive(Clone, Default, PartialEq, Eq, zeroize::ZeroizeOnDrop)]
/// Interpolating a `ProtocolSecret` with `{}` must not compile (MED-017-B):
///
/// ```compile_fail
/// let t = soulsystem_common::secrets::ProtocolSecret::new("tok");
/// let _ = format!("device {t}");
/// ```
pub struct ProtocolSecret(String);

impl ProtocolSecret {
    /// Wrap a credential that travels over a protocol.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Borrow the plaintext. Named to be visible in review, like
    /// [`SecretString::expose`].
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether a value is present.
    pub fn is_present(&self) -> bool {
        !self.0.is_empty()
    }

    /// Length of the underlying credential.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the credential is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for ProtocolSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

// `Display` is deliberately NOT implemented for `ProtocolSecret`, for the
// same reason as `SecretString` (MED-017): a redacting Display compiles
// cleanly where a real value is needed and silently substitutes
// "[REDACTED]" at runtime. For a type whose entire purpose is to carry the
// value faithfully to a peer, that failure mode is even worse — the wire
// form would be corrupted while every log line looked correct. The audit
// found no interpolation site; this removes the footgun before one appears.
// Interpolation is a compile error: use `.expose()` or `{:?}`.

impl From<String> for ProtocolSecret {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for ProtocolSecret {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// Faithful, unlike [`SecretString`]'s: the peer needs the real value.
impl serde::Serialize for ProtocolSecret {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for ProtocolSecret {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self)
    }
}

#[cfg(test)]
mod protocol_secret_tests {
    use super::*;

    const TOKEN: &str = "dev-8c1f0a4b93e27d65";

    /// Display no longer exists (MED-017-B) — that absence is pinned by the
    /// `compile_fail` doctest on the type; Debug still redacts.
    #[test]
    fn debug_still_redacts() {
        let secret = ProtocolSecret::new(TOKEN);
        assert_eq!(format!("{secret:?}"), REDACTED);
    }

    #[test]
    fn an_enclosing_derived_debug_is_safe() {
        // The accidental path is identical for a protocol message: nobody
        // prints the token, they print the handshake response holding it.
        #[derive(Debug)]
        struct HelloOk {
            session_id: String,
            device_token: ProtocolSecret,
        }
        let hello = HelloOk {
            session_id: "s-1".into(),
            device_token: ProtocolSecret::new(TOKEN),
        };
        let rendered = format!("{hello:?}");
        assert!(!rendered.contains(TOKEN), "Debug leaked the token");
        assert!(rendered.contains("s-1"), "and kept the useful fields");
    }

    #[test]
    fn serialization_is_faithful_because_the_peer_needs_the_value() {
        // The whole point of the type. If this ever redacts, every client
        // receiving a handshake gets "<redacted>" as its token and
        // authentication breaks at runtime with no compile error.
        let secret = ProtocolSecret::new(TOKEN);
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, format!("\"{TOKEN}\""));
    }

    #[test]
    fn it_round_trips_through_serde() {
        let secret = ProtocolSecret::new(TOKEN);
        let json = serde_json::to_string(&secret).unwrap();
        let back: ProtocolSecret = serde_json::from_str(&json).unwrap();
        assert_eq!(back.expose(), TOKEN);
    }

    #[test]
    fn it_differs_from_secret_string_in_exactly_one_respect() {
        // Pins the distinction the two types exist to draw, so a future edit
        // cannot quietly collapse them into each other.
        let protocol = ProtocolSecret::new(TOKEN);
        let config = SecretString::new(TOKEN);

        assert_eq!(
            format!("{protocol:?}"),
            format!("{config:?}"),
            "Debug: same"
        );
        assert_ne!(
            serde_json::to_string(&protocol).unwrap(),
            serde_json::to_string(&config).unwrap(),
            "Serialize: the one deliberate difference"
        );
    }
}

#[cfg(test)]
mod secret_string_tests {
    use super::*;

    const TOKEN: &str = "sk-live-4f9a2c7e1b8d6350";

    #[test]
    fn debug_never_renders_the_secret() {
        let secret = SecretString::new(TOKEN);
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains(TOKEN), "Debug leaked the secret");
        assert!(
            !rendered.contains("4f9a"),
            "Debug leaked part of the secret"
        );
        assert_eq!(rendered, REDACTED);
    }

    #[test]
    fn an_enclosing_derived_debug_is_safe_without_its_author_thinking_about_it() {
        // The actual failure mode: nobody prints the token deliberately, they
        // print the struct that holds it.
        #[derive(Debug)]
        struct ProviderConfig {
            endpoint: String,
            api_key: SecretString,
        }

        let config = ProviderConfig {
            endpoint: "https://api.example.com".into(),
            api_key: SecretString::new(TOKEN),
        };
        let rendered = format!("{config:?}");
        assert!(!rendered.contains(TOKEN));
        assert!(
            rendered.contains("api.example.com"),
            "only the secret is redacted; the rest stays useful for debugging"
        );
    }

    #[test]
    fn the_redaction_does_not_disclose_the_length() {
        // `***` vs `****************` would narrow a brute-force search.
        let short = SecretString::new("ab");
        let long = SecretString::new("a".repeat(512));
        assert_eq!(format!("{short:?}"), format!("{long:?}"));
    }

    #[test]
    fn expose_returns_the_real_value() {
        assert_eq!(SecretString::new(TOKEN).expose(), TOKEN);
    }

    #[test]
    fn a_blank_value_is_treated_as_absent() {
        // Same rule the listeners apply: a blank config entry or an
        // exported-but-empty variable must not become a valid credential.
        assert!(!SecretString::new("").is_present());
        assert!(!SecretString::new("   \t\n").is_present());
        assert!(SecretString::new("x").is_present());
    }

    #[test]
    fn serialization_writes_the_redaction_not_the_secret() {
        // A config struct gets serialized to debug endpoints and state dumps.
        // Round-tripping plaintext through those is the same leak in a
        // different coat.
        let json = serde_json::to_string(&SecretString::new(TOKEN)).expect("serializes");
        assert!(!json.contains(TOKEN));
        assert!(json.contains(REDACTED));
    }

    #[test]
    fn deserialization_accepts_a_plain_string() {
        let secret: SecretString =
            serde_json::from_str(&format!("\"{TOKEN}\"")).expect("deserializes");
        assert_eq!(secret.expose(), TOKEN);
    }

    #[test]
    fn a_struct_holding_one_does_not_leak_through_serde() {
        #[derive(serde::Serialize)]
        struct Config {
            name: String,
            token: SecretString,
        }
        let json = serde_json::to_string(&Config {
            name: "prod".into(),
            token: SecretString::new(TOKEN),
        })
        .expect("serializes");
        assert!(!json.contains(TOKEN));
        assert!(json.contains("prod"), "non-secret fields survive");
    }

    #[test]
    fn from_env_treats_unset_and_blank_alike() {
        let var = "SOULSYSTEM_TEST_SECRET_P1_5";
        std::env::remove_var(var);
        assert!(SecretString::from_env(var).is_none());

        std::env::set_var(var, "   ");
        assert!(
            SecretString::from_env(var).is_none(),
            "an exported-but-blank variable is not a credential"
        );

        std::env::set_var(var, TOKEN);
        assert_eq!(
            SecretString::from_env(var).map(|s| s.expose().to_owned()),
            Some(TOKEN.to_owned())
        );
        std::env::remove_var(var);
    }
}
