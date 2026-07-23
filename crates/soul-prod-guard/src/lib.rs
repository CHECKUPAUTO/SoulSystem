//! Fail-closed production readiness guard for SoulSystem.
//!
//! This crate contains **pure** decision logic: given an explicit
//! [`RuntimeMode`] and a [`SecurityPosture`] describing the concrete facts
//! about how the process is about to start (which addresses it will bind,
//! whether TLS/auth are wired, whether the OS isolation backend is present,
//! and so on), [`evaluate`] decides whether startup may proceed.
//!
//! Two non-negotiable properties are enforced here and covered by tests:
//!
//! 1. **Production is never inferred.** [`RuntimeMode::from_env_value`] fails
//!    when `SOULSYSTEM_ENV` is unset or unrecognised — the caller must state
//!    the mode explicitly.
//! 2. **Production fails closed.** In [`RuntimeMode::Production`] any single
//!    unmet prerequisite turns startup into a hard error. In
//!    [`RuntimeMode::Development`] the same findings are surfaced as warnings
//!    so the operator can see them, but startup is allowed.
//!
//! No secret *values* ever appear in a [`GuardViolation`], an [`AuditEvent`],
//! or the [`StartupRejected`] error — only names, paths, and addresses. This
//! keeps the guard safe to log.
//!
//! The heavy work of *gathering* the posture (probing the OS, reading config,
//! canonicalising paths) lives in the caller (the `soulsystem` binary). Keeping
//! that separation is what makes the decision logic exhaustively unit-testable
//! without spinning up any service.

#![forbid(unsafe_code)]

use std::fmt;

/// Explicit runtime mode. Never inferred from the absence of a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeMode {
    /// Relaxed local mode: prerequisites are reported as warnings, not errors.
    Development,
    /// Hardened mode: every prerequisite is mandatory and failure is fatal.
    Production,
}

/// Error returned when the runtime mode cannot be determined explicitly.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModeError {
    /// `SOULSYSTEM_ENV` was unset or empty.
    #[error(
        "SOULSYSTEM_ENV is not set; it must be exactly \"production\" or \
         \"development\" (production is never inferred from the absence of a flag)"
    )]
    Unset,
    /// `SOULSYSTEM_ENV` held an unrecognised value.
    #[error(
        "SOULSYSTEM_ENV has unsupported value {0:?}; expected \"production\" or \"development\""
    )]
    Unsupported(String),
}

impl RuntimeMode {
    /// Parse a mode from an explicit `SOULSYSTEM_ENV` value.
    ///
    /// `None` or an empty/whitespace string is an error, as is any value other
    /// than `production` or `development`. This is the single source of truth
    /// for how the mode is decided.
    pub fn from_env_value(value: Option<&str>) -> Result<Self, ModeError> {
        match value.map(str::trim) {
            None | Some("") => Err(ModeError::Unset),
            Some("production") => Ok(Self::Production),
            Some("development") => Ok(Self::Development),
            Some(other) => Err(ModeError::Unsupported(other.to_string())),
        }
    }

    /// Parse the mode from the process environment (`SOULSYSTEM_ENV`).
    pub fn from_env() -> Result<Self, ModeError> {
        Self::from_env_value(std::env::var("SOULSYSTEM_ENV").ok().as_deref())
    }

    /// Whether this is [`RuntimeMode::Production`].
    pub fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }

    /// Lower-case label used in audit events and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }
}

/// A network listener the process intends to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerPosture {
    /// Human-readable listener name (e.g. `"gateway"`, `"api"`).
    pub name: String,
    /// The address the listener will bind, e.g. `"127.0.0.1:7878"`.
    pub bind_addr: String,
    /// Whether TLS is active on this listener's serving path.
    pub tls_enabled: bool,
    /// Whether an authentication layer gates this listener.
    pub authenticated: bool,
}

impl ListenerPosture {
    /// Convenience constructor.
    pub fn new(
        name: impl Into<String>,
        bind_addr: impl Into<String>,
        tls_enabled: bool,
        authenticated: bool,
    ) -> Self {
        Self {
            name: name.into(),
            bind_addr: bind_addr.into(),
            tls_enabled,
            authenticated,
        }
    }
}

/// Concrete, secret-free facts about how the process is about to start.
///
/// Every field is a decided fact, not a knob: the caller assembles it from the
/// CLI, config, environment, and cheap OS probes before any listener opens.
#[derive(Debug, Clone, Default)]
pub struct SecurityPosture {
    /// Every listener the process will open.
    pub listeners: Vec<ListenerPosture>,
    /// Whether non-default authentication material (a token/secret) is present.
    pub auth_material_present: bool,
    /// Whether the workspace/data root exists and canonicalises.
    pub workspace_root_canonical: bool,
    /// Whether a sandbox executor is present as the process-execution path.
    pub sandbox_backend_present: bool,
    /// Whether the OS isolation backend (bubblewrap/namespaces) is available.
    pub isolation_backend_available: bool,
    /// Whether self-modification is enabled.
    pub self_modification_enabled: bool,
    /// Whether self-modification has a signing + approval policy configured.
    pub self_modification_signed_and_gated: bool,
    /// Webhook providers that are configured to receive callbacks.
    pub webhook_providers: Vec<String>,
    /// Subset of `webhook_providers` that have a verification secret set.
    pub webhook_providers_with_secret: Vec<String>,
    /// Config/secret file paths with group- or world-accessible permissions.
    pub insecure_permission_paths: Vec<String>,
    /// Names (never values) of secrets matching a known default/example.
    pub default_or_example_secret_names: Vec<String>,
    /// The selected runtime if it is an experimental (non-canonical) one.
    pub experimental_runtime_selected: Option<String>,
}

/// A single unmet production prerequisite.
///
/// `Display` deliberately carries only names, paths, and bind addresses — never
/// secret material — so violations are safe to log and to return to operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardViolation {
    /// No authentication material is configured.
    MissingAuthMaterial,
    /// The workspace/data root could not be canonicalised.
    WorkspaceRootNotCanonical,
    /// No sandbox executor is present for process execution.
    MissingSandboxBackend,
    /// The mandatory OS isolation backend is unavailable.
    IsolationBackendUnavailable,
    /// A listener binds a non-loopback address without TLS.
    NonLoopbackBindWithoutTls { listener: String, bind_addr: String },
    /// A listener is exposed without an authentication layer.
    UnauthenticatedListener { listener: String, bind_addr: String },
    /// Self-modification is enabled without a signing + approval policy.
    SelfModificationWithoutSigning,
    /// A configured webhook provider has no verification secret.
    WebhookProviderWithoutSecret { provider: String },
    /// A secret still holds a known default/example value.
    DefaultOrExampleSecret { name: String },
    /// A config/secret file has insecure (group/world-accessible) permissions.
    InsecureFilePermissions { path: String },
    /// An experimental runtime was selected in production.
    ExperimentalRuntimeInProduction { runtime: String },
}

impl GuardViolation {
    /// Stable machine-readable code for this violation kind.
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingAuthMaterial => "missing_auth_material",
            Self::WorkspaceRootNotCanonical => "workspace_root_not_canonical",
            Self::MissingSandboxBackend => "missing_sandbox_backend",
            Self::IsolationBackendUnavailable => "isolation_backend_unavailable",
            Self::NonLoopbackBindWithoutTls { .. } => "non_loopback_bind_without_tls",
            Self::UnauthenticatedListener { .. } => "unauthenticated_listener",
            Self::SelfModificationWithoutSigning => "self_modification_without_signing",
            Self::WebhookProviderWithoutSecret { .. } => "webhook_provider_without_secret",
            Self::DefaultOrExampleSecret { .. } => "default_or_example_secret",
            Self::InsecureFilePermissions { .. } => "insecure_file_permissions",
            Self::ExperimentalRuntimeInProduction { .. } => "experimental_runtime_in_production",
        }
    }
}

impl fmt::Display for GuardViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAuthMaterial => {
                write!(f, "no authentication material is configured")
            }
            Self::WorkspaceRootNotCanonical => {
                write!(
                    f,
                    "workspace/data root does not exist or cannot be canonicalised"
                )
            }
            Self::MissingSandboxBackend => {
                write!(f, "no sandbox executor is present for process execution")
            }
            Self::IsolationBackendUnavailable => {
                write!(
                    f,
                    "mandatory OS isolation backend (bubblewrap/namespaces) is unavailable"
                )
            }
            Self::NonLoopbackBindWithoutTls {
                listener,
                bind_addr,
            } => write!(
                f,
                "listener {listener:?} binds non-loopback address {bind_addr} without TLS"
            ),
            Self::UnauthenticatedListener {
                listener,
                bind_addr,
            } => write!(
                f,
                "listener {listener:?} on {bind_addr} has no authentication layer"
            ),
            Self::SelfModificationWithoutSigning => write!(
                f,
                "self-modification is enabled without a signing and approval policy"
            ),
            Self::WebhookProviderWithoutSecret { provider } => write!(
                f,
                "webhook provider {provider:?} has no verification secret configured"
            ),
            Self::DefaultOrExampleSecret { name } => {
                write!(f, "secret {name:?} still holds a default/example value")
            }
            Self::InsecureFilePermissions { path } => write!(
                f,
                "config/secret file {path:?} has insecure (group/world-accessible) permissions"
            ),
            Self::ExperimentalRuntimeInProduction { runtime } => write!(
                f,
                "experimental runtime {runtime:?} cannot be selected in production"
            ),
        }
    }
}

/// A structured, secret-free record of a security decision made at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// The decision family, e.g. `"startup_security_validation"`.
    pub decision: &'static str,
    /// The outcome: `"pass"`, `"warn"`, or `"rejected"`.
    pub outcome: &'static str,
    /// Human-readable detail. Never contains secret values.
    pub detail: String,
}

/// The result of a successful (or dev-mode) evaluation.
#[derive(Debug, Clone)]
pub struct GuardReport {
    /// The mode the evaluation ran under.
    pub mode: RuntimeMode,
    /// Findings. Empty on a clean production start; in development these are
    /// the prerequisites that *would* have blocked a production start.
    pub violations: Vec<GuardViolation>,
    /// Structured audit events to emit.
    pub audit_events: Vec<AuditEvent>,
}

impl GuardReport {
    /// Whether any prerequisite was unmet (relevant chiefly in development).
    pub fn has_findings(&self) -> bool {
        !self.violations.is_empty()
    }
}

/// Error returned when a production start is refused. Fails closed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "production startup rejected: {} fail-closed security violation(s): {details}",
    violations.len()
)]
pub struct StartupRejected {
    /// The concrete violations, in evaluation order.
    pub violations: Vec<GuardViolation>,
    /// A pre-rendered, secret-free summary of the violations.
    pub details: String,
    /// Audit events to emit alongside the rejection.
    pub audit_events: Vec<AuditEvent>,
}

/// Return `true` if `bind_addr`'s host part is a loopback address.
///
/// Accepts `host:port`, bare `host`, and bracketed IPv6 (`[::1]:port`).
/// `127.0.0.0/8`, `::1`, and the literal `localhost` are loopback; everything
/// else — including `0.0.0.0`, `::`, and any routable address — is not.
pub fn is_loopback_bind(bind_addr: &str) -> bool {
    let host = host_of(bind_addr);
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if host == "::1" {
        return true;
    }
    // IPv4: loopback iff first octet is 127.
    let mut octets = host.split('.');
    if let (Some(a), Some(_), Some(_), Some(_), None) = (
        octets.next(),
        octets.next(),
        octets.next(),
        octets.next(),
        octets.next(),
    ) {
        if let Ok(first) = a.parse::<u8>() {
            return first == 127;
        }
    }
    false
}

/// Extract the host portion from a bind address string.
fn host_of(bind_addr: &str) -> &str {
    let s = bind_addr.trim();
    // Bracketed IPv6, e.g. "[::1]:7878" or "[::1]".
    if let Some(rest) = s.strip_prefix('[') {
        if let Some(end) = rest.find(']') {
            return &rest[..end];
        }
    }
    // Bare IPv6 (contains multiple colons) — no port suffix we can split on.
    if s.matches(':').count() >= 2 {
        return s;
    }
    // "host:port" or bare "host".
    match s.split_once(':') {
        Some((host, _port)) => host,
        None => s,
    }
}

/// Evaluate whether startup may proceed under `mode` given `posture`.
///
/// In [`RuntimeMode::Production`] any violation yields [`StartupRejected`]
/// (fail closed). In [`RuntimeMode::Development`] the same violations are
/// returned inside an `Ok` [`GuardReport`] as warnings.
pub fn evaluate(
    mode: RuntimeMode,
    posture: &SecurityPosture,
) -> Result<GuardReport, StartupRejected> {
    let mut violations = Vec::new();

    for listener in &posture.listeners {
        if !is_loopback_bind(&listener.bind_addr) && !listener.tls_enabled {
            violations.push(GuardViolation::NonLoopbackBindWithoutTls {
                listener: listener.name.clone(),
                bind_addr: listener.bind_addr.clone(),
            });
        }
        if !listener.authenticated {
            violations.push(GuardViolation::UnauthenticatedListener {
                listener: listener.name.clone(),
                bind_addr: listener.bind_addr.clone(),
            });
        }
    }

    if !posture.auth_material_present {
        violations.push(GuardViolation::MissingAuthMaterial);
    }
    if !posture.workspace_root_canonical {
        violations.push(GuardViolation::WorkspaceRootNotCanonical);
    }
    if !posture.sandbox_backend_present {
        violations.push(GuardViolation::MissingSandboxBackend);
    }
    if !posture.isolation_backend_available {
        violations.push(GuardViolation::IsolationBackendUnavailable);
    }
    if posture.self_modification_enabled && !posture.self_modification_signed_and_gated {
        violations.push(GuardViolation::SelfModificationWithoutSigning);
    }
    for provider in &posture.webhook_providers {
        if !posture.webhook_providers_with_secret.contains(provider) {
            violations.push(GuardViolation::WebhookProviderWithoutSecret {
                provider: provider.clone(),
            });
        }
    }
    for name in &posture.default_or_example_secret_names {
        violations.push(GuardViolation::DefaultOrExampleSecret { name: name.clone() });
    }
    for path in &posture.insecure_permission_paths {
        violations.push(GuardViolation::InsecureFilePermissions { path: path.clone() });
    }
    if let Some(runtime) = &posture.experimental_runtime_selected {
        violations.push(GuardViolation::ExperimentalRuntimeInProduction {
            runtime: runtime.clone(),
        });
    }

    let details = render_details(&violations);

    if mode.is_production() && !violations.is_empty() {
        let audit_events = vec![AuditEvent {
            decision: "startup_security_validation",
            outcome: "rejected",
            detail: format!(
                "production start refused with {} violation(s): {details}",
                violations.len()
            ),
        }];
        return Err(StartupRejected {
            violations,
            details,
            audit_events,
        });
    }

    let outcome = if violations.is_empty() {
        "pass"
    } else {
        "warn"
    };
    let detail = if violations.is_empty() {
        format!("{} mode startup security validation passed", mode.as_str())
    } else {
        format!(
            "{} mode startup: {} prerequisite(s) unmet (warning only): {details}",
            mode.as_str(),
            violations.len()
        )
    };
    Ok(GuardReport {
        mode,
        violations,
        audit_events: vec![AuditEvent {
            decision: "startup_security_validation",
            outcome,
            detail,
        }],
    })
}

/// Render a numbered, secret-free summary of the violations.
fn render_details(violations: &[GuardViolation]) -> String {
    violations
        .iter()
        .enumerate()
        .map(|(i, v)| format!("[{}] {} ({})", i + 1, v, v.code()))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A posture that passes every production check.
    fn clean_production_posture() -> SecurityPosture {
        SecurityPosture {
            listeners: vec![
                ListenerPosture::new("gateway", "127.0.0.1:7878", false, true),
                ListenerPosture::new("api", "127.0.0.1:9023", false, true),
            ],
            auth_material_present: true,
            workspace_root_canonical: true,
            sandbox_backend_present: true,
            isolation_backend_available: true,
            self_modification_enabled: false,
            self_modification_signed_and_gated: false,
            webhook_providers: vec![],
            webhook_providers_with_secret: vec![],
            insecure_permission_paths: vec![],
            default_or_example_secret_names: vec![],
            experimental_runtime_selected: None,
        }
    }

    // ---- mode parsing ----

    #[test]
    fn mode_unset_is_error() {
        assert_eq!(RuntimeMode::from_env_value(None), Err(ModeError::Unset));
        assert_eq!(RuntimeMode::from_env_value(Some("")), Err(ModeError::Unset));
        assert_eq!(
            RuntimeMode::from_env_value(Some("   ")),
            Err(ModeError::Unset)
        );
    }

    #[test]
    fn mode_unknown_is_error() {
        assert_eq!(
            RuntimeMode::from_env_value(Some("prod")),
            Err(ModeError::Unsupported("prod".to_string()))
        );
        assert_eq!(
            RuntimeMode::from_env_value(Some("PRODUCTION")),
            Err(ModeError::Unsupported("PRODUCTION".to_string()))
        );
    }

    #[test]
    fn mode_explicit_values_parse() {
        assert_eq!(
            RuntimeMode::from_env_value(Some("production")),
            Ok(RuntimeMode::Production)
        );
        assert_eq!(
            RuntimeMode::from_env_value(Some(" development ")),
            Ok(RuntimeMode::Development)
        );
        assert!(RuntimeMode::Production.is_production());
        assert!(!RuntimeMode::Development.is_production());
    }

    // ---- loopback detection ----

    #[test]
    fn loopback_detection() {
        assert!(is_loopback_bind("127.0.0.1:7878"));
        assert!(is_loopback_bind("127.5.6.7:80"));
        assert!(is_loopback_bind("localhost:9023"));
        assert!(is_loopback_bind("[::1]:7878"));
        assert!(is_loopback_bind("::1"));
        assert!(!is_loopback_bind("0.0.0.0:7878"));
        assert!(!is_loopback_bind("10.0.0.5:7878"));
        assert!(!is_loopback_bind("192.168.1.10:443"));
        assert!(!is_loopback_bind("[::]:7878"));
        assert!(!is_loopback_bind("example.com:443"));
    }

    // ---- clean start ----

    #[test]
    fn clean_production_start_is_allowed() {
        let report = evaluate(RuntimeMode::Production, &clean_production_posture())
            .expect("clean production posture must pass");
        assert!(!report.has_findings());
        assert_eq!(report.audit_events[0].outcome, "pass");
    }

    // ---- each violation, in production, fails closed ----

    #[test]
    fn missing_auth_material_rejected() {
        let mut p = clean_production_posture();
        p.auth_material_present = false;
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert_eq!(err.violations, vec![GuardViolation::MissingAuthMaterial]);
    }

    #[test]
    fn non_canonical_workspace_root_rejected() {
        let mut p = clean_production_posture();
        p.workspace_root_canonical = false;
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err
            .violations
            .contains(&GuardViolation::WorkspaceRootNotCanonical));
    }

    #[test]
    fn missing_sandbox_backend_rejected() {
        let mut p = clean_production_posture();
        p.sandbox_backend_present = false;
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err
            .violations
            .contains(&GuardViolation::MissingSandboxBackend));
    }

    #[test]
    fn missing_isolation_backend_rejected() {
        let mut p = clean_production_posture();
        p.isolation_backend_available = false;
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err
            .violations
            .contains(&GuardViolation::IsolationBackendUnavailable));
    }

    #[test]
    fn non_loopback_without_tls_rejected() {
        let mut p = clean_production_posture();
        p.listeners = vec![ListenerPosture::new("gateway", "0.0.0.0:7878", false, true)];
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err.violations.iter().any(|v| matches!(
            v,
            GuardViolation::NonLoopbackBindWithoutTls { bind_addr, .. } if bind_addr == "0.0.0.0:7878"
        )));
    }

    #[test]
    fn non_loopback_with_tls_is_allowed() {
        let mut p = clean_production_posture();
        p.listeners = vec![ListenerPosture::new("gateway", "0.0.0.0:7878", true, true)];
        assert!(evaluate(RuntimeMode::Production, &p).is_ok());
    }

    #[test]
    fn unauthenticated_listener_rejected() {
        let mut p = clean_production_posture();
        p.listeners = vec![ListenerPosture::new(
            "gateway",
            "127.0.0.1:7878",
            false,
            false,
        )];
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err
            .violations
            .iter()
            .any(|v| matches!(v, GuardViolation::UnauthenticatedListener { .. })));
    }

    #[test]
    fn self_mod_without_signing_rejected_but_gated_ok() {
        let mut p = clean_production_posture();
        p.self_modification_enabled = true;
        p.self_modification_signed_and_gated = false;
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err
            .violations
            .contains(&GuardViolation::SelfModificationWithoutSigning));

        p.self_modification_signed_and_gated = true;
        assert!(evaluate(RuntimeMode::Production, &p).is_ok());
    }

    #[test]
    fn webhook_without_secret_rejected() {
        let mut p = clean_production_posture();
        p.webhook_providers = vec!["discord".to_string(), "slack".to_string()];
        p.webhook_providers_with_secret = vec!["discord".to_string()];
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err.violations.iter().any(|v| matches!(
            v,
            GuardViolation::WebhookProviderWithoutSecret { provider } if provider == "slack"
        )));
        // The provider WITH a secret must not be flagged.
        assert!(!err.violations.iter().any(|v| matches!(
            v,
            GuardViolation::WebhookProviderWithoutSecret { provider } if provider == "discord"
        )));
    }

    #[test]
    fn default_secret_rejected() {
        let mut p = clean_production_posture();
        p.default_or_example_secret_names = vec!["gateway_token".to_string()];
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err.violations.iter().any(|v| matches!(
            v,
            GuardViolation::DefaultOrExampleSecret { name } if name == "gateway_token"
        )));
    }

    #[test]
    fn insecure_permissions_rejected() {
        let mut p = clean_production_posture();
        p.insecure_permission_paths = vec!["/opt/soulsystem/config/secrets.toml".to_string()];
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err
            .violations
            .iter()
            .any(|v| matches!(v, GuardViolation::InsecureFilePermissions { .. })));
    }

    #[test]
    fn experimental_runtime_rejected() {
        let mut p = clean_production_posture();
        p.experimental_runtime_selected = Some("soul_entity".to_string());
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err.violations.iter().any(|v| matches!(
            v,
            GuardViolation::ExperimentalRuntimeInProduction { runtime } if runtime == "soul_entity"
        )));
    }

    // ---- development mode surfaces the same findings as warnings ----

    #[test]
    fn development_allows_but_warns() {
        let p = SecurityPosture::default(); // maximally unsafe posture
        let report =
            evaluate(RuntimeMode::Development, &p).expect("development must never fail closed");
        assert!(report.has_findings());
        assert_eq!(report.audit_events[0].outcome, "warn");
    }

    #[test]
    fn production_with_default_posture_reports_multiple_violations() {
        // The all-false default posture must trip several distinct checks.
        let p = SecurityPosture::default();
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        assert!(err.violations.len() >= 4, "expected several violations");
    }

    // ---- secrets never leak into rendered output ----

    #[test]
    fn violation_display_carries_no_secret_values() {
        // Even though a real secret value might be "hunter2", the guard only
        // ever receives the NAME. Prove the rendered strings contain names and
        // codes, never a value, for every variant.
        let v = GuardViolation::DefaultOrExampleSecret {
            name: "gateway_token".to_string(),
        };
        let rendered = format!("{v} {}", v.code());
        assert!(rendered.contains("gateway_token"));
        assert!(rendered.contains("default_or_example_secret"));
    }

    #[test]
    fn rejected_error_message_is_stable_and_secret_free() {
        let mut p = clean_production_posture();
        p.auth_material_present = false;
        p.default_or_example_secret_names = vec!["webhook_secret".to_string()];
        let err = evaluate(RuntimeMode::Production, &p).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("production startup rejected"));
        assert!(msg.contains("missing_auth_material"));
        assert!(msg.contains("webhook_secret"));
    }
}
