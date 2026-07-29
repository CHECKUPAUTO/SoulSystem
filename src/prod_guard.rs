//! Wiring between the running `soulsystem` binary and the pure
//! [`soul_prod_guard`] fail-closed startup evaluator.
//!
//! This module gathers the concrete [`SecurityPosture`] from the CLI, config,
//! environment, and cheap OS probes, then asks the guard whether startup may
//! proceed. In production an unmet prerequisite aborts startup before any
//! listener opens; in development the same findings are logged as warnings.
//!
//! The decision logic itself lives in the `soul-prod-guard` crate and is
//! exhaustively unit-tested there. This module is intentionally thin: it only
//! observes real facts and forwards them.

use soul_gateway::GatewayAuth;
use soul_prod_guard::{
    evaluate, GuardReport, ListenerPosture, ModeError, RuntimeMode, SecurityPosture,
};
use soulsystem::config::Settings;
use tracing::{error, info, warn};

/// Known placeholder secret values that must never reach production.
const DEFAULT_SECRET_SENTINELS: &[&str] = &[
    "",
    "changeme",
    "change-me",
    "example",
    "example-token",
    "placeholder",
    "secret",
    "test",
    "token",
    "dev",
    "default",
];

/// Resolve the explicit runtime mode.
///
/// An unset `SOULSYSTEM_ENV` resolves to development with a loud warning — it
/// NEVER resolves to production. An unrecognised value is a hard error rather
/// than a silent guess.
fn resolve_mode() -> anyhow::Result<RuntimeMode> {
    match RuntimeMode::from_env() {
        Ok(mode) => Ok(mode),
        Err(ModeError::Unset) => {
            warn!(
                "SOULSYSTEM_ENV is not set; defaulting to development mode. Production is \
                 never selected implicitly — set SOULSYSTEM_ENV=production explicitly to enable it."
            );
            Ok(RuntimeMode::Development)
        }
        Err(e @ ModeError::Unsupported(_)) => Err(anyhow::Error::new(e)),
    }
}

/// Minimal PATH lookup for an executable (avoids a new dependency).
fn which_in_path(exe: &str) -> bool {
    let Ok(path) = std::env::var("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(exe).is_file())
}

/// Probe whether the OS isolation backend (bubblewrap) is available.
fn isolation_backend_available() -> bool {
    which_in_path("bwrap")
}

/// Whether a secret value looks like a default/example placeholder.
fn is_default_secret(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    DEFAULT_SECRET_SENTINELS.contains(&normalized.as_str())
}

/// Read all gateway auth material from the environment.
fn gateway_tokens() -> Vec<(String, String)> {
    let mut tokens = Vec::new();
    if let Ok(entries) = std::env::var("SOULSYSTEM_GATEWAY_TOKENS") {
        for entry in entries.split(',') {
            if let Some((principal, token)) = entry.split_once('=') {
                if !principal.trim().is_empty() && !token.trim().is_empty() {
                    tokens.push((
                        format!("SOULSYSTEM_GATEWAY_TOKENS:{}", principal.trim()),
                        token.trim().to_string(),
                    ));
                }
            }
        }
    }
    if let Ok(token) = std::env::var("SOULSYSTEM_GATEWAY_TOKEN") {
        if !token.is_empty() {
            tokens.push(("SOULSYSTEM_GATEWAY_TOKEN".into(), token));
        }
    }
    tokens
}

/// Detect group/world-accessible permissions on a config path (Unix only).
#[cfg(unix)]
fn is_world_or_group_accessible(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        // Any group or other permission bit set is considered insecure.
        Ok(meta) => meta.permissions().mode() & 0o077 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_world_or_group_accessible(_path: &std::path::Path) -> bool {
    false
}

/// Assemble the current security posture from real inputs.
///
/// Conservative by construction: capabilities whose secure backing has not yet
/// been proven in the runtime path (an authentication layer, an active TLS
/// serving path) are reported as absent so production fails closed until the
/// dedicated PRs wire them in.
fn assemble_posture(
    gateway_addr: &str,
    settings: &Settings,
    gateway_authenticated: bool,
    gateway_tls_enabled: bool,
    ws_bridge: &soulsystem::ws_bridge::WsBridgeConfig,
) -> SecurityPosture {
    let tokens = gateway_tokens();
    let auth_material_present =
        !tokens.is_empty() && tokens.iter().all(|(_, token)| !is_default_secret(token));

    let mut default_or_example_secret_names = Vec::new();
    for (name, token) in &tokens {
        if is_default_secret(token) {
            default_or_example_secret_names.push(name.clone());
        }
    }

    // The gateway's operator routes are protected by the GatewayAuth bearer
    // middleware. `api` does not use that middleware and must remain reported as
    // unauthenticated. `ws_bridge` reports its *real* posture, derived from the
    // configuration it will actually serve with, rather than a hardcoded value:
    // it authenticates exactly when a usable shared secret is configured
    // (CRIT-007 / INV-NET-1).
    let listeners = vec![
        ListenerPosture::new(
            "gateway",
            gateway_addr,
            gateway_tls_enabled,
            gateway_authenticated,
        ),
        ListenerPosture::new("api", "127.0.0.1:9023", false, false),
        ListenerPosture::new(
            "ws_bridge",
            &ws_bridge.listen,
            false,
            ws_bridge.is_authenticated(),
        ),
    ];

    let workspace_root_canonical = std::fs::canonicalize(&settings.paths.data_dir).is_ok();

    let config_file =
        std::env::var("SOULSYSTEM_CONFIG_FILE").unwrap_or_else(|_| "soulsystem.toml".to_string());
    let cfg_path = std::path::Path::new(&config_file);
    let mut insecure_permission_paths = Vec::new();
    if cfg_path.exists() && is_world_or_group_accessible(cfg_path) {
        insecure_permission_paths.push(config_file);
    }

    SecurityPosture {
        listeners,
        auth_material_present,
        workspace_root_canonical,
        // A sandbox executor (BoundSystem / soul_sandbox) is present in the
        // binary; whether it is the SOLE execution path is enforced in PR D.
        sandbox_backend_present: true,
        isolation_backend_available: isolation_backend_available(),
        // Self-modification is disabled by default; the signed, gated path is
        // implemented in the self-modification PR.
        self_modification_enabled: false,
        self_modification_signed_and_gated: false,
        webhook_providers: Vec::new(),
        webhook_providers_with_secret: Vec::new(),
        insecure_permission_paths,
        default_or_example_secret_names,
        experimental_runtime_selected: None,
    }
}

/// Enforce the fail-closed startup security policy.
///
/// Call this before opening any listener. Returns the [`GuardReport`] on a
/// permitted start (in development, possibly carrying warnings); returns an
/// error — aborting startup — when production prerequisites are unmet.
pub fn enforce_startup_security(
    gateway_addr: &str,
    settings: &Settings,
    gateway_auth: &GatewayAuth,
    gateway_tls_enabled: bool,
    ws_bridge: &soulsystem::ws_bridge::WsBridgeConfig,
) -> anyhow::Result<GuardReport> {
    let mode = resolve_mode()?;
    let posture = assemble_posture(
        gateway_addr,
        settings,
        gateway_auth.is_configured(),
        gateway_tls_enabled,
        ws_bridge,
    );

    match evaluate(mode, &posture) {
        Ok(report) => {
            for ev in &report.audit_events {
                if report.has_findings() {
                    warn!(
                        decision = ev.decision,
                        outcome = ev.outcome,
                        "{}",
                        ev.detail
                    );
                } else {
                    info!(
                        decision = ev.decision,
                        outcome = ev.outcome,
                        "{}",
                        ev.detail
                    );
                }
            }
            Ok(report)
        }
        Err(rejected) => {
            for ev in &rejected.audit_events {
                error!(
                    decision = ev.decision,
                    outcome = ev.outcome,
                    "{}",
                    ev.detail
                );
            }
            Err(anyhow::Error::new(rejected))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener<'a>(posture: &'a SecurityPosture, name: &str) -> &'a ListenerPosture {
        posture
            .listeners
            .iter()
            .find(|listener| listener.name == name)
            .expect("listener must be present in posture")
    }

    #[test]
    fn gateway_is_authenticated_when_the_consumed_authenticator_has_a_token() {
        let settings = Settings::default();
        let auth = GatewayAuth::configured("a-real-operator-token");

        let posture = assemble_posture(
            "127.0.0.1:7878",
            &settings,
            auth.is_configured(),
            false,
            &soulsystem::ws_bridge::WsBridgeConfig::default(),
        );

        assert!(listener(&posture, "gateway").authenticated);
        assert!(!listener(&posture, "api").authenticated);
        assert!(!listener(&posture, "ws_bridge").authenticated);
    }

    #[test]
    fn gateway_is_unauthenticated_when_the_consumed_authenticator_has_no_token() {
        let settings = Settings::default();
        let auth = GatewayAuth::unconfigured();

        let posture = assemble_posture(
            "127.0.0.1:7878",
            &settings,
            auth.is_configured(),
            false,
            &soulsystem::ws_bridge::WsBridgeConfig::default(),
        );

        assert!(!listener(&posture, "gateway").authenticated);
        assert!(!listener(&posture, "api").authenticated);
        assert!(!listener(&posture, "ws_bridge").authenticated);
    }

    /// The guard must read the bridge's real configuration, not a hardcoded
    /// value: a configured secret means the listener genuinely authenticates.
    #[test]
    fn ws_bridge_posture_is_derived_from_its_real_configuration() {
        let settings = Settings::default();
        let auth = GatewayAuth::unconfigured();

        let configured = soulsystem::ws_bridge::WsBridgeConfig {
            shared_secret: Some("a-real-bridge-secret".into()),
            ..Default::default()
        };
        let posture = assemble_posture(
            "127.0.0.1:7878",
            &settings,
            auth.is_configured(),
            false,
            &configured,
        );
        assert!(
            listener(&posture, "ws_bridge").authenticated,
            "a configured shared secret must be reported as authenticated"
        );

        // Opting in to unauthenticated access must NOT be reported as
        // authenticated — otherwise production could start on a bridge that
        // serves anyone.
        let opted_in = soulsystem::ws_bridge::WsBridgeConfig {
            shared_secret: None,
            unauthenticated_access: soulsystem::ws_bridge::UnauthenticatedAccess::Allow,
            ..Default::default()
        };
        let posture = assemble_posture(
            "127.0.0.1:7878",
            &settings,
            auth.is_configured(),
            false,
            &opted_in,
        );
        assert!(
            !listener(&posture, "ws_bridge").authenticated,
            "opting in to unauthenticated access is not authentication"
        );
    }

    /// Production must refuse to start while the bridge is unauthenticated.
    #[test]
    fn production_rejects_an_unauthenticated_ws_bridge() {
        let settings = Settings::default();
        let auth = GatewayAuth::configured("a-real-operator-token");

        let posture = assemble_posture(
            "127.0.0.1:7878",
            &settings,
            auth.is_configured(),
            true,
            &soulsystem::ws_bridge::WsBridgeConfig::default(),
        );

        let rejected = evaluate(RuntimeMode::Production, &posture)
            .expect_err("production must refuse to start with an unauthenticated ws_bridge");
        assert!(
            rejected.violations.iter().any(|v| matches!(
                v,
                soul_prod_guard::GuardViolation::UnauthenticatedListener { listener, .. }
                    if listener == "ws_bridge"
            )),
            "expected an UnauthenticatedListener violation naming ws_bridge; got {:?}",
            rejected.violations
        );
        assert!(
            !rejected.details.contains("a-real-operator-token"),
            "the rejection summary must stay secret-free"
        );
    }
}
