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
