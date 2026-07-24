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

/// Read the gateway auth token from the environment, if any non-empty.
fn gateway_token() -> Option<String> {
    std::env::var("SOULSYSTEM_GATEWAY_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())
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
) -> SecurityPosture {
    let token = gateway_token();
    let auth_material_present = token
        .as_deref()
        .map(|t| !is_default_secret(t))
        .unwrap_or(false);

    let mut default_or_example_secret_names = Vec::new();
    if let Some(t) = &token {
        if is_default_secret(t) {
            default_or_example_secret_names.push("SOULSYSTEM_GATEWAY_TOKEN".to_string());
        }
    }

    // The gateway's operator routes are protected by the GatewayAuth bearer
    // middleware. The API and local WS bridge below do not use that middleware
    // and must remain reported as unauthenticated.
    let tls_enabled = false;

    let listeners = vec![
        ListenerPosture::new("gateway", gateway_addr, tls_enabled, gateway_authenticated),
        ListenerPosture::new("api", "127.0.0.1:9023", tls_enabled, false),
        ListenerPosture::new("ws_bridge", "127.0.0.1:9022", tls_enabled, false),
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
) -> anyhow::Result<GuardReport> {
    let mode = resolve_mode()?;
    let posture = assemble_posture(gateway_addr, settings, gateway_auth.is_configured());

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

        let posture = assemble_posture("127.0.0.1:7878", &settings, auth.is_configured());

        assert!(listener(&posture, "gateway").authenticated);
        assert!(!listener(&posture, "api").authenticated);
        assert!(!listener(&posture, "ws_bridge").authenticated);
    }

    #[test]
    fn gateway_is_unauthenticated_when_the_consumed_authenticator_has_no_token() {
        let settings = Settings::default();
        let auth = GatewayAuth::unconfigured();

        let posture = assemble_posture("127.0.0.1:7878", &settings, auth.is_configured());

        assert!(!listener(&posture, "gateway").authenticated);
        assert!(!listener(&posture, "api").authenticated);
        assert!(!listener(&posture, "ws_bridge").authenticated);
    }
}
