//! Authorization scopes shared by every operator listener (CRIT-007 residual,
//! INV-NET-1).
//!
//! # Why this lives here and not in each listener
//!
//! `soul_gateway` and the binary's `src/api.rs` both authenticate bearer
//! tokens, and both previously granted every authenticated caller full
//! operator power — including shell execution. Giving each listener its own
//! scope enum would just move the coherence problem: two definitions of
//! "write" that drift apart are worse than one that is occasionally awkward.
//! The binary already depends on `soul_gateway`, so the model lives here and
//! both listeners consume it.
//!
//! # The opt-in default, and what it does not buy
//!
//! A credential configured without a scope list is granted [`ScopeSet::all`].
//! That is a deliberate product decision, recorded so nobody has to infer it
//! from the code: scopes are **opt-in**, so upgrading cannot start returning
//! 403 to running automation that worked yesterday.
//!
//! The honest consequence is that this change makes least privilege
//! *expressible*, not *default*. A deployment that does not configure scopes is
//! exactly as exposed as it was before. INV-NET-1 therefore stays `PARTIAL`
//! until a deployment actually narrows its tokens — the mechanism being present
//! is not the same as the property holding.

use std::fmt;

/// A capability a credential may hold.
///
/// Ordered from least to most powerful, but **not** hierarchical apart from
/// [`Scope::Admin`]: holding `Write` does not imply `Read`. Implication is easy
/// to get subtly wrong and hard to audit, so a credential that should read and
/// write is configured with both, visibly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// Observe state: status, goals, events, metrics, bridge health.
    ///
    /// Not harmless — these routes disclose what the host is doing — but they
    /// change nothing.
    Read,
    /// Create or modify state that the system will act on later: goals, plans,
    /// stored memories.
    Write,
    /// Cause the host to run something: shell commands, PTY sessions, cognitive
    /// cycles, plan execution, subagent spawning.
    ///
    /// Separate from `Write` because this is the scope that turns a leaked
    /// token into host compromise.
    Exec,
    /// Everything, including scopes added in future versions.
    Admin,
}

impl Scope {
    /// Parse one scope name, case-insensitively.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "exec" => Some(Self::Exec),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    /// The canonical lowercase name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Exec => "exec",
            Self::Admin => "admin",
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The set of scopes a credential holds.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeSet {
    scopes: Vec<Scope>,
}

impl ScopeSet {
    /// Every scope, including any added later.
    ///
    /// Represented as `Admin` rather than an enumeration of the current
    /// variants, so adding a `Scope` variant in future cannot silently narrow
    /// an existing "all" credential — or silently widen one that meant "these
    /// four specifically".
    pub fn all() -> Self {
        Self {
            scopes: vec![Scope::Admin],
        }
    }

    /// No scopes at all — holds nothing.
    pub fn none() -> Self {
        Self { scopes: Vec::new() }
    }

    /// Build from an explicit list, de-duplicated and ordered.
    pub fn from_scopes(scopes: impl IntoIterator<Item = Scope>) -> Self {
        let mut scopes: Vec<Scope> = scopes.into_iter().collect();
        scopes.sort_unstable();
        scopes.dedup();
        Self { scopes }
    }

    /// Parse a `+`-separated scope list, e.g. `read+write`.
    ///
    /// An unrecognised name is **dropped**, not treated as an error and not
    /// treated as a grant: a typo like `exce` must not silently become full
    /// access, and it must not make the whole credential unusable in a way an
    /// operator would work around by removing scopes entirely. The dropped name
    /// is returned so the caller can log it.
    pub fn parse(raw: &str) -> (Self, Vec<String>) {
        let mut scopes = Vec::new();
        let mut unknown = Vec::new();
        for part in raw.split('+') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            match Scope::parse(part) {
                Some(scope) => scopes.push(scope),
                None => unknown.push(part.to_owned()),
            }
        }
        (Self::from_scopes(scopes), unknown)
    }

    /// Whether this set satisfies a requirement for `required`.
    pub fn allows(&self, required: Scope) -> bool {
        self.scopes
            .iter()
            .any(|held| *held == Scope::Admin || *held == required)
    }

    /// Whether this set holds nothing.
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }

    /// Whether this set is the unrestricted one.
    pub fn is_all(&self) -> bool {
        self.scopes.contains(&Scope::Admin)
    }

    /// The scopes held, for logging and diagnostics.
    pub fn scopes(&self) -> &[Scope] {
        &self.scopes
    }
}

impl fmt::Display for ScopeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scopes.is_empty() {
            return f.write_str("(none)");
        }
        let names: Vec<&str> = self.scopes.iter().map(Scope::as_str).collect();
        f.write_str(&names.join("+"))
    }
}

/// Split a credential's principal part into `(principal, scopes)`.
///
/// The configured form is `principal:scope1+scope2=token`. Everything before
/// the first `:` is the principal; everything after is the scope list. A
/// principal with **no** `:` is the legacy form and is granted
/// [`ScopeSet::all`] — see this module's header for why that default was
/// chosen over failing closed.
pub fn split_principal_and_scopes(raw: &str) -> (String, ScopeSet, Vec<String>) {
    match raw.split_once(':') {
        Some((principal, scope_list)) => {
            let (scopes, unknown) = ScopeSet::parse(scope_list);
            // `alice:` with nothing after the colon is a configuration mistake,
            // not a request for zero access. Treating it as "no scopes" would
            // silently produce a credential that authenticates but can do
            // nothing, which reads as a broken deployment rather than a policy.
            // Treating it as "all" would silently over-grant. Neither is good,
            // so it is reported as unknown and grants nothing — the failure is
            // visible in the logs and at the first request.
            (principal.trim().to_owned(), scopes, unknown)
        }
        None => (raw.trim().to_owned(), ScopeSet::all(), Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_names_round_trip() {
        for scope in [Scope::Read, Scope::Write, Scope::Exec, Scope::Admin] {
            assert_eq!(Scope::parse(scope.as_str()), Some(scope));
            assert_eq!(scope.to_string(), scope.as_str());
        }
        assert_eq!(Scope::parse("  ReAd "), Some(Scope::Read));
        assert_eq!(Scope::parse("root"), None);
        assert_eq!(Scope::parse(""), None);
    }

    #[test]
    fn admin_satisfies_every_requirement() {
        let admin = ScopeSet::from_scopes([Scope::Admin]);
        for required in [Scope::Read, Scope::Write, Scope::Exec, Scope::Admin] {
            assert!(admin.allows(required), "admin must allow {required}");
        }
    }

    #[test]
    fn scopes_do_not_imply_one_another() {
        // Deliberate: implication is easy to get subtly wrong and hard to
        // audit. A credential that should read and write says so.
        let write_only = ScopeSet::from_scopes([Scope::Write]);
        assert!(write_only.allows(Scope::Write));
        assert!(!write_only.allows(Scope::Read));
        assert!(!write_only.allows(Scope::Exec));
        assert!(!write_only.allows(Scope::Admin));

        let read_write = ScopeSet::from_scopes([Scope::Read, Scope::Write]);
        assert!(read_write.allows(Scope::Read));
        assert!(read_write.allows(Scope::Write));
        assert!(
            !read_write.allows(Scope::Exec),
            "the scope that turns a leaked token into host compromise is not \
             implied by anything"
        );
    }

    #[test]
    fn an_empty_set_allows_nothing() {
        let none = ScopeSet::none();
        assert!(none.is_empty());
        for required in [Scope::Read, Scope::Write, Scope::Exec, Scope::Admin] {
            assert!(!none.allows(required));
        }
    }

    #[test]
    fn parsing_a_scope_list_drops_unknown_names_rather_than_granting_them() {
        let (set, unknown) = ScopeSet::parse("read+exce+write");
        assert!(set.allows(Scope::Read));
        assert!(set.allows(Scope::Write));
        assert!(
            !set.allows(Scope::Exec),
            "a typo must not become the scope it resembles"
        );
        assert_eq!(unknown, vec!["exce"]);
    }

    #[test]
    fn parsing_tolerates_whitespace_and_empty_segments() {
        let (set, unknown) = ScopeSet::parse(" read + + write ");
        assert_eq!(set, ScopeSet::from_scopes([Scope::Read, Scope::Write]));
        assert!(unknown.is_empty());
    }

    #[test]
    fn duplicate_scopes_collapse() {
        assert_eq!(
            ScopeSet::parse("read+read+read").0,
            ScopeSet::from_scopes([Scope::Read])
        );
    }

    #[test]
    fn a_principal_without_a_scope_list_is_granted_everything() {
        // The recorded product decision: scopes are opt-in, so upgrading does
        // not start returning 403 to automation that worked yesterday.
        let (principal, scopes, unknown) = split_principal_and_scopes("alice");
        assert_eq!(principal, "alice");
        assert!(scopes.is_all());
        assert!(scopes.allows(Scope::Exec));
        assert!(unknown.is_empty());
    }

    #[test]
    fn a_principal_with_a_scope_list_is_narrowed_to_it() {
        let (principal, scopes, unknown) = split_principal_and_scopes("ci:read+write");
        assert_eq!(principal, "ci");
        assert!(!scopes.is_all());
        assert!(scopes.allows(Scope::Read));
        assert!(scopes.allows(Scope::Write));
        assert!(!scopes.allows(Scope::Exec));
        assert!(unknown.is_empty());
    }

    #[test]
    fn a_trailing_colon_grants_nothing_rather_than_everything() {
        // The failure mode to avoid: `alice:` looking like `alice` and
        // silently restoring full access.
        let (principal, scopes, _) = split_principal_and_scopes("alice:");
        assert_eq!(principal, "alice");
        assert!(scopes.is_empty());
        assert!(!scopes.allows(Scope::Read));
    }

    #[test]
    fn a_principal_whose_scopes_are_all_typos_grants_nothing() {
        let (_, scopes, unknown) = split_principal_and_scopes("bob:reed+wrote");
        assert!(scopes.is_empty(), "no scope is better than the wrong scope");
        assert_eq!(unknown, vec!["reed", "wrote"]);
    }

    #[test]
    fn display_is_stable_and_secret_free() {
        assert_eq!(ScopeSet::all().to_string(), "admin");
        assert_eq!(ScopeSet::none().to_string(), "(none)");
        assert_eq!(
            ScopeSet::from_scopes([Scope::Write, Scope::Read]).to_string(),
            "read+write",
            "ordered, so log lines are comparable"
        );
    }
}
