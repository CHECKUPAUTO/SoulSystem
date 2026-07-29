//! Fail-closed cross-origin policy, shared by every SoulSystem HTTP listener
//! (INV-NET-4).
//!
//! # What `CorsLayer::permissive()` actually costs
//!
//! It sends `Access-Control-Allow-Origin: *`. That header does not bypass
//! authentication by itself — but it removes the browser's same-origin barrier,
//! which is what normally stops any page the operator happens to have open from
//! reading a listener's responses. On a service that can enumerate system
//! state, dispatch work, or run commands, that is the wrong default, and
//! binding to `127.0.0.1` does not fix it: a page loaded from anywhere can
//! still issue requests to `http://127.0.0.1:<port>` and, because of that
//! header, *read* what comes back.
//!
//! # Fail closed, and no wildcard
//!
//! An unset or blank allowlist yields [`CorsPolicy::Disabled`], which permits
//! no cross-origin access at all. That is the opposite of the usual default and
//! the point of the exercise: a service that has not been configured must not
//! be the most permissive one on the network.
//!
//! A bare `*` is **filtered out rather than honoured**. Restoring permissive
//! behaviour should take a deliberate enumeration of origins, not one character
//! in a config file that looks like a placeholder.
//!
//! # `Disabled` is not "no CORS layer"
//!
//! A layer that emits *no* `Access-Control-Allow-Origin` is what makes a
//! browser refuse the response. Removing the layer entirely would leave the
//! response readable by any origin that already has a reason to be reading it,
//! so [`CorsPolicy::Disabled`] still installs a layer — one with an empty
//! origin list.
//!
//! # Why this crate exists rather than a third copy
//!
//! Two listeners already carried hand-written versions of this policy. The
//! third through seventh would have been mirror-and-drift: the two existing
//! copies had already diverged on allowed methods and `max_age`, and a policy
//! that differs per service for no stated reason is one nobody can audit at a
//! glance. Allowed methods stay a per-service parameter, because they genuinely
//! differ — a read-only dashboard has no business advertising `POST`.
//!
//! It deliberately does not depend on `axum`, so services on axum 0.7 and 0.8
//! can share one policy.

use std::time::Duration;

use http::{header, HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};

/// How long a browser may cache a preflight response.
pub const DEFAULT_PREFLIGHT_MAX_AGE: Duration = Duration::from_secs(600);

/// Methods for a read-only listener — a status page or metrics endpoint that
/// exposes no mutating route.
pub const READ_ONLY_METHODS: &[Method] = &[Method::GET];

/// Methods for a listener that accepts submissions as well as reads.
pub const READ_WRITE_METHODS: &[Method] = &[Method::GET, Method::POST, Method::OPTIONS];

/// The configured cross-origin policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorsPolicy {
    /// No cross-origin browser access. The default whenever the allowlist is
    /// unset, blank, or names no usable origin.
    Disabled,
    /// Exactly these origins are permitted.
    Allowlist(Vec<String>),
}

impl CorsPolicy {
    /// Read the allowlist from the environment variable named `var`.
    ///
    /// Each service names its own variable: they are separately deployed and
    /// have no reason to share an origin list, and one shared variable would
    /// mean configuring the most exposed service also configures the least.
    pub fn from_env(var: &str) -> Self {
        match std::env::var(var) {
            Ok(raw) => Self::parse(&raw),
            Err(_) => Self::Disabled,
        }
    }

    /// Parse a comma-separated origin list.
    ///
    /// Blank entries and a bare `*` are dropped. If nothing usable remains the
    /// result is [`CorsPolicy::Disabled`] — the fail-closed direction.
    pub fn parse(raw: &str) -> Self {
        let origins: Vec<String> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != "*")
            .map(str::to_owned)
            .collect();
        if origins.is_empty() {
            Self::Disabled
        } else {
            Self::Allowlist(origins)
        }
    }

    /// Whether `origin` is permitted.
    pub fn allows(&self, origin: &str) -> bool {
        match self {
            Self::Disabled => false,
            Self::Allowlist(origins) => origins.iter().any(|o| o == origin),
        }
    }

    /// The origins this policy permits, for logging and tests.
    pub fn origins(&self) -> &[String] {
        match self {
            Self::Disabled => &[],
            Self::Allowlist(origins) => origins,
        }
    }

    /// Build the tower-http layer, advertising `methods`.
    ///
    /// An origin that is not a valid header value is dropped rather than
    /// panicking: a typo in one entry of an allowlist should cost that entry,
    /// not the process. Since a dropped entry simply is not allowed, the
    /// failure direction stays closed.
    pub fn layer(&self, methods: &[Method]) -> CorsLayer {
        let base = CorsLayer::new()
            .allow_methods(methods.to_vec())
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
            .max_age(DEFAULT_PREFLIGHT_MAX_AGE);

        let origins: Vec<HeaderValue> = self
            .origins()
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();

        // Even when empty: an empty list emits no `Access-Control-Allow-Origin`,
        // which is what makes a browser refuse the cross-origin response.
        base.allow_origin(AllowOrigin::list(origins))
    }

    /// [`CorsPolicy::layer`] for a read-only listener.
    pub fn read_only_layer(&self) -> CorsLayer {
        self.layer(READ_ONLY_METHODS)
    }

    /// [`CorsPolicy::layer`] for a listener that also accepts submissions.
    pub fn read_write_layer(&self) -> CorsLayer {
        self.layer(READ_WRITE_METHODS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_or_blank_allowlist_permits_no_origin() {
        // The fail-closed direction: "not configured" must not mean "any
        // origin", which is what permissive() did.
        assert_eq!(CorsPolicy::parse(""), CorsPolicy::Disabled);
        assert_eq!(CorsPolicy::parse("   "), CorsPolicy::Disabled);
        assert_eq!(CorsPolicy::parse(",, ,"), CorsPolicy::Disabled);
        assert!(!CorsPolicy::Disabled.allows("https://example.com"));
        assert!(CorsPolicy::Disabled.origins().is_empty());
    }

    #[test]
    fn a_bare_wildcard_does_not_re_enable_permissive_behaviour() {
        assert_eq!(CorsPolicy::parse("*"), CorsPolicy::Disabled);
        assert_eq!(CorsPolicy::parse(" * "), CorsPolicy::Disabled);
        assert!(!CorsPolicy::parse("*").allows("https://evil.example"));
    }

    #[test]
    fn a_wildcard_mixed_with_real_origins_drops_only_the_wildcard() {
        assert_eq!(
            CorsPolicy::parse("*, https://ops.example.com"),
            CorsPolicy::Allowlist(vec!["https://ops.example.com".into()]),
        );
        let policy = CorsPolicy::parse("*, https://ops.example.com");
        assert!(policy.allows("https://ops.example.com"));
        assert!(
            !policy.allows("https://evil.example"),
            "the wildcard must not survive as a catch-all"
        );
    }

    #[test]
    fn an_allowlist_permits_exactly_what_it_names() {
        let policy = CorsPolicy::parse("https://a.example, https://b.example");
        assert!(policy.allows("https://a.example"));
        assert!(policy.allows("https://b.example"));
        assert!(!policy.allows("https://c.example"));
        // Not a prefix or substring match.
        assert!(!policy.allows("https://a.example.evil.com"));
        assert!(!policy.allows("https://a.exampl"));
    }

    #[test]
    fn entries_are_trimmed_so_a_spaced_list_still_works() {
        assert_eq!(
            CorsPolicy::parse("  https://a.example ,https://b.example  "),
            CorsPolicy::Allowlist(vec!["https://a.example".into(), "https://b.example".into()]),
        );
    }

    #[test]
    fn from_env_treats_an_unset_variable_as_disabled() {
        // A name no test sets, so this reads a genuinely absent variable.
        assert_eq!(
            CorsPolicy::from_env("SOULSYSTEM_CORS_VAR_THAT_IS_NEVER_SET"),
            CorsPolicy::Disabled,
        );
    }

    #[test]
    fn an_unparseable_origin_costs_that_entry_and_not_the_process() {
        // A newline cannot appear in a header value. The layer must still
        // build, and the bad entry must simply not be allowed.
        let policy = CorsPolicy::parse("https://ok.example, bad\nvalue");
        assert_eq!(policy.origins().len(), 2, "parse keeps both");
        let _ = policy.read_only_layer(); // must not panic
        assert!(policy.allows("https://ok.example"));
    }

    #[test]
    fn both_layer_shapes_build_for_both_policy_states() {
        // Cheap, but it pins the thing that would otherwise only be discovered
        // at runtime in whichever service happened to be started first.
        let _ = CorsPolicy::Disabled.read_only_layer();
        let _ = CorsPolicy::Disabled.read_write_layer();
        let allow = CorsPolicy::parse("https://ops.example.com");
        let _ = allow.read_only_layer();
        let _ = allow.read_write_layer();
    }

    #[test]
    fn read_only_methods_do_not_advertise_mutation() {
        assert_eq!(READ_ONLY_METHODS, &[Method::GET]);
        assert!(
            !READ_ONLY_METHODS.contains(&Method::POST),
            "a read-only listener must not advertise POST"
        );
        assert!(READ_WRITE_METHODS.contains(&Method::POST));
    }
}
