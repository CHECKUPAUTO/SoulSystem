//! Dead code is denied in this crate despite the workspace-wide `-A dead_code`
//! (LOW-001). A source attribute is used rather than a `[lints]` table because
//! the table does NOT win against RUSTFLAGS — verified, not assumed.
//!
//! The suppression exists for noise. Here it would hide the thing that matters:
//! HIGH-008 was a dead `AutonomousAgent::executor` field that falsely implied a
//! second sandboxing mechanism existed. In a crate whose job IS a protection,
//! an unused item usually means the protection is not wired.
#![deny(dead_code)]
//! Shared bearer-token authentication for SoulSystem HTTP listeners.
//!
//! This crate exists because `soul_gateway` had the only working
//! implementation and `soullink-orchestrator-standalone` had none — its
//! `POST /api/mesh/spawn` route turned an unauthenticated request into a
//! process (MED-013). The obvious fix, copying the token check across, is the
//! one to avoid: two copies of a comparison that must be constant-time is how
//! one of them quietly stops being constant-time.
//!
//! ## What this crate is not
//!
//! It is not an authorization system. `soul_gateway` layers scopes on top of
//! authentication and that machinery stays where it is — a service with one
//! dangerous route does not need a scope lattice, and moving 300 lines of
//! tested scope code to serve a service that would not use it would risk
//! CRIT-007's behaviour for no gain.
//!
//! ## Per-service credentials
//!
//! Each listener reads its own environment variables rather than sharing one
//! token. Sharing would mean compromising the read-only dashboard hands you
//! the process-spawning route, which is precisely the coupling authentication
//! is supposed to prevent.

use std::sync::Arc;

pub use soul_prod_guard::RuntimeMode;

/// Compare two byte strings without leaking their contents through timing.
///
/// The single implementation in the workspace. It always walks the longer of
/// the two inputs, so it reveals nothing through an early return — including
/// nothing about the length of the expected token, which a naive length check
/// would disclose before the first byte was ever compared.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = (a.len() ^ b.len()) as u64;
    for index in 0..a.len().max(b.len()) {
        diff |= u64::from(
            a.get(index).copied().unwrap_or_default() ^ b.get(index).copied().unwrap_or_default(),
        );
    }
    diff == 0
}

/// An authenticated caller's name, for logs and audit records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Principal(Arc<str>);

impl Principal {
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Principal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone)]
struct Credential {
    principal: Arc<str>,
    token: Arc<str>,
}

/// Why a listener refused to start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthStartupError {
    /// Production, but no credentials were configured.
    #[error(
        "{service} is running with SOULSYSTEM_ENV=production but no credentials \
         are configured; set {vars} to a non-empty value. Refusing to start: \
         this listener exposes routes that act on the host, and starting \
         without authentication would expose them to anyone who can reach the \
         port."
    )]
    ProductionWithoutCredentials {
        service: &'static str,
        vars: &'static str,
    },
}

/// A set of bearer tokens, each bound to a principal name.
///
/// Cloneable and cheap to share: the credential list is behind an `Arc`.
#[derive(Clone)]
pub struct TokenAuth {
    credentials: Arc<[Credential]>,
}

/// Redacts the tokens. Printing a count is useful when diagnosing "why is
/// everything 401"; printing the tokens would put them in a log file.
impl std::fmt::Debug for TokenAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenAuth")
            .field("principal_count", &self.credentials.len())
            .finish()
    }
}

impl TokenAuth {
    /// Build from explicit `(principal, token)` pairs. Mostly for tests.
    pub fn from_pairs<I, P, T>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (P, T)>,
        P: AsRef<str>,
        T: AsRef<str>,
    {
        let credentials: Vec<Credential> = pairs
            .into_iter()
            .filter(|(p, t)| !p.as_ref().is_empty() && !t.as_ref().is_empty())
            .map(|(p, t)| Credential {
                principal: Arc::from(p.as_ref()),
                token: Arc::from(t.as_ref()),
            })
            .collect();
        Self {
            credentials: credentials.into(),
        }
    }

    /// Read credentials from the environment.
    ///
    /// `multi_var` accepts a comma-separated list of `principal=token`.
    /// `single_var` is the one-credential shorthand, recorded under the
    /// principal name `"default"`.
    ///
    /// Entries missing a name or a token are skipped rather than accepted with
    /// an empty token — an empty expected token would authenticate an empty
    /// `Authorization: Bearer ` header.
    pub fn from_env(multi_var: &str, single_var: &str) -> Self {
        let mut credentials = Vec::new();

        if let Ok(entries) = std::env::var(multi_var) {
            for entry in entries.split(',') {
                if let Some((principal, token)) = entry.split_once('=') {
                    let (principal, token) = (principal.trim(), token.trim());
                    if !principal.is_empty() && !token.is_empty() {
                        credentials.push(Credential {
                            principal: Arc::from(principal),
                            token: Arc::from(token),
                        });
                    }
                }
            }
        }

        if let Ok(token) = std::env::var(single_var) {
            let token = token.trim();
            if !token.is_empty() {
                credentials.push(Credential {
                    principal: Arc::from("default"),
                    token: Arc::from(token),
                });
            }
        }

        Self {
            credentials: credentials.into(),
        }
    }

    /// Whether any credential is configured.
    pub fn is_configured(&self) -> bool {
        !self.credentials.is_empty()
    }

    pub fn principal_count(&self) -> usize {
        self.credentials.len()
    }

    /// Authenticate a bearer token, returning who presented it.
    ///
    /// Every credential is compared even after a match, so the work done does
    /// not depend on *which* credential matched or on how many were checked
    /// before it.
    pub fn authenticate(&self, provided: Option<&str>) -> Option<Principal> {
        let given = provided?;
        let mut matched = None;
        for credential in self.credentials.iter() {
            if constant_time_eq(credential.token.as_bytes(), given.as_bytes()) {
                matched = Some(Principal(credential.principal.clone()));
            }
        }
        matched
    }

    /// Extract the token from an `Authorization` header value and authenticate.
    ///
    /// The `Bearer ` prefix is matched case-sensitively, as RFC 6750 writes it.
    pub fn authenticate_header(&self, header: Option<&str>) -> Option<Principal> {
        self.authenticate(header.and_then(|v| v.strip_prefix("Bearer ")))
    }

    /// Fail closed: refuse to start an unauthenticated listener in production.
    ///
    /// Development is permitted to run without credentials, and says so loudly
    /// once at startup rather than per request — a warning nobody can act on
    /// mid-traffic is just noise.
    ///
    /// An unparseable `SOULSYSTEM_ENV` is treated as production, matching the
    /// provenance guard: the mode that refuses is the safe one to guess.
    pub fn enforce_startup(
        &self,
        service: &'static str,
        vars: &'static str,
    ) -> Result<(), AuthStartupError> {
        let production = RuntimeMode::from_env()
            .map(|m| m.is_production())
            .unwrap_or(true);

        if self.is_configured() {
            tracing::info!(
                service,
                principals = self.principal_count(),
                "authentication enabled"
            );
            return Ok(());
        }

        if production {
            return Err(AuthStartupError::ProductionWithoutCredentials { service, vars });
        }

        tracing::warn!(
            service,
            vars,
            "no credentials configured — every route on this listener is open \
             to anyone who can reach the port. Permitted because \
             SOULSYSTEM_ENV=development; this would refuse to start in \
             production."
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_matching_token_authenticates_and_names_the_principal() {
        let auth = TokenAuth::from_pairs([("ci", "tok-a"), ("ops", "tok-b")]);
        assert_eq!(auth.authenticate(Some("tok-b")).unwrap().name(), "ops");
    }

    #[test]
    fn a_wrong_or_absent_token_does_not_authenticate() {
        let auth = TokenAuth::from_pairs([("ci", "tok-a")]);
        assert!(auth.authenticate(Some("tok-b")).is_none());
        assert!(auth.authenticate(None).is_none());
        assert!(auth.authenticate(Some("")).is_none());
    }

    /// An empty configured token must never authenticate.
    ///
    /// `Authorization: Bearer ` with nothing after it yields `Some("")`. If an
    /// empty token were stored, `constant_time_eq(b"", b"")` is true and that
    /// header would authenticate.
    #[test]
    fn an_empty_configured_token_is_dropped_rather_than_stored() {
        let auth = TokenAuth::from_pairs([("ghost", "")]);
        assert!(!auth.is_configured());
        assert!(auth.authenticate(Some("")).is_none());
    }

    #[test]
    fn the_bearer_prefix_is_required() {
        let auth = TokenAuth::from_pairs([("ci", "tok-a")]);
        assert!(auth.authenticate_header(Some("Bearer tok-a")).is_some());
        assert!(auth.authenticate_header(Some("tok-a")).is_none());
        assert!(auth.authenticate_header(Some("bearer tok-a")).is_none());
        assert!(auth.authenticate_header(None).is_none());
    }

    #[test]
    fn constant_time_eq_agrees_with_equality() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        // Differing lengths, including the prefix case a length check would
        // short-circuit on.
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn an_unconfigured_auth_authenticates_nothing() {
        let auth = TokenAuth::from_pairs(Vec::<(String, String)>::new());
        assert!(!auth.is_configured());
        assert!(auth.authenticate(Some("anything")).is_none());
    }

    #[test]
    fn debug_does_not_print_tokens() {
        let auth = TokenAuth::from_pairs([("ci", "super-secret-value")]);
        let rendered = format!("{auth:?}");
        assert!(!rendered.contains("super-secret-value"), "{rendered}");
        assert!(rendered.contains("principal_count"));
    }
}
