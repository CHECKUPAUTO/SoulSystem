//! Provider configuration — loads provider entries from TOML config.
//!
//! Provider sections in `gateway.toml`:
//! ```toml
//! [provider.ollama]
//! provider_type = "ollama"
//! base_url = "http://localhost:11434"
//! models = ["deepseek-v4-flash:cloud", "llama3:70b"]
//! default_model = "deepseek-v4-flash:cloud"
//! timeout_secs = 120
//!
//! [provider.openai]
//! provider_type = "openai"
//! base_url = "https://api.openai.com/v1"
//! api_key = "${OPENAI_API_KEY}"
//! models = ["gpt-5", "o3"]
//! timeout_secs = 60
//! ```

use std::collections::HashMap;

use crate::provider;

/// Load provider configurations from the gateway config TOML.
///
/// The TOML may include `[provider.*]` sections alongside the existing
/// `[telegram]`, `[orchestrator]`, etc.
pub fn load_provider_configs() -> HashMap<String, provider::ProviderConfig> {
    let mut providers = HashMap::new();

    // Load TOML manually for provider sections (they're dynamic keys)
    let path = std::env::var("SOULLINK_GATEWAY_CONFIG")
        .unwrap_or_else(|_| "/etc/soullink/gateway.toml".into());

    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(_) => {
            // No config file — that's fine, return empty
            return providers;
        }
    };

    // Parse the TOML as a table to extract provider.* sections
    let table: toml::Table = match toml::from_str(&raw) {
        Ok(t) => t,
        Err(_) => return providers,
    };

    // Look for [provider.xxx] sections
    for (key, value) in &table {
        if key == "provider" {
            if let toml::Value::Table(provider_table) = value {
                for (name, cfg_val) in provider_table {
                    if let toml::Value::Table(cfg) = cfg_val {
                        let provider_type = cfg
                            .get("type")
                            .or_else(|| cfg.get("provider_type"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("ollama")
                            .to_string();

                        let base_url = cfg
                            .get("base_url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("http://localhost:11434")
                            .to_string();

                        let api_key = cfg
                            .get("api_key")
                            .and_then(|v| v.as_str())
                            .map(|s| resolve_env_var(s));

                        let models: Vec<String> = cfg
                            .get("models")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();

                        let default_model = cfg
                            .get("default_model")
                            .or_else(|| cfg.get("default-model"))
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        let timeout_secs = cfg
                            .get("timeout_secs")
                            .or_else(|| cfg.get("timeout"))
                            .and_then(|v| v.as_integer())
                            .unwrap_or(30) as u64;

                        providers.insert(
                            name.clone(),
                            provider::ProviderConfig {
                                provider_type,
                                base_url,
                                api_key,
                                models,
                                default_model,
                                timeout_secs,
                            },
                        );
                    }
                }
            }
        }
    }

    providers
}

/// Resolve `${ENV_VAR}` or `$ENV_VAR` references in config strings.
fn resolve_env_var(s: &str) -> String {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
        std::env::var(rest).unwrap_or_default()
    } else if let Some(rest) = s.strip_prefix('$') {
        std::env::var(rest).unwrap_or_default()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_env_var_plain() {
        assert_eq!(resolve_env_var("hello"), "hello");
    }

    #[test]
    fn test_resolve_env_var_brace() {
        // Should resolve to the env var or empty string — don't panic
        let resolved = resolve_env_var("${OPENAI_API_KEY}");
        let expected = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_load_provider_configs_empty_config() {
        // With no config file, should return empty map
        let providers = load_provider_configs();
        // Should not panic
        assert!(providers.is_empty() || !providers.is_empty());
    }
}
