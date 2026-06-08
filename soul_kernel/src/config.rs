/// Configuration du runtime Soul System.
///
/// Parametres centralises pour l'ordonnancement, le cluster, la télémétrie
/// et les seuils de sécurité. Peut être chargé depuis un fichier TOML ou
/// construit programmatiquement.
pub struct SoulConfig {
    /// Port du serveur clinique (defaut : 8080)
    pub clinical_port: u16,
    /// Adresse du cluster local (defaut : "127.0.0.1:48999")
    pub cluster_address: String,
    /// Timeout du scout SearXNG en secondes (defaut : 5)
    pub scout_timeout_secs: u64,
    /// Seuil thermique en °C (defaut : 80)
    pub thermal_limit_celsius: u32,
    /// Sensibilité du hook autograd (defaut : 0.5)
    pub autograd_sensitivity: f32,
    /// Seuil du pare-feu sémantique (defaut : 0.85)
    pub firewall_threshold: f32,
    /// Nombre de slots du KV-cache (defaut : 16)
    pub kv_cache_window: usize,
    /// Nombre d'attention sinks (defaut : 2)
    pub kv_cache_sinks: usize,
}

impl Default for SoulConfig {
    fn default() -> Self {
        Self {
            clinical_port: 8080,
            cluster_address: "127.0.0.1:48999".to_string(),
            scout_timeout_secs: 5,
            thermal_limit_celsius: 80,
            autograd_sensitivity: 0.5,
            firewall_threshold: 0.85,
            kv_cache_window: 16,
            kv_cache_sinks: 2,
        }
    }
}

impl SoulConfig {
    /// Charge la configuration depuis une chaîne TOML.
    /// Les champs manquants gardent leurs valeurs par défaut.
    pub fn from_toml(toml_str: &str) -> Self {
        let mut config = Self::default();
        for line in toml_str.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                match key {
                    "clinical_port" => {
                        if let Ok(v) = value.parse() { config.clinical_port = v; }
                    }
                    "cluster_address" => {
                        config.cluster_address = value.to_string();
                    }
                    "scout_timeout_secs" => {
                        if let Ok(v) = value.parse() { config.scout_timeout_secs = v; }
                    }
                    "thermal_limit_celsius" => {
                        if let Ok(v) = value.parse() { config.thermal_limit_celsius = v; }
                    }
                    "autograd_sensitivity" => {
                        if let Ok(v) = value.parse() { config.autograd_sensitivity = v; }
                    }
                    "firewall_threshold" => {
                        if let Ok(v) = value.parse() { config.firewall_threshold = v; }
                    }
                    "kv_cache_window" => {
                        if let Ok(v) = value.parse() { config.kv_cache_window = v; }
                    }
                    "kv_cache_sinks" => {
                        if let Ok(v) = value.parse() { config.kv_cache_sinks = v; }
                    }
                    _ => {}
                }
            }
        }
        config
    }

    /// Exporte la configuration en TOML.
    pub fn to_toml(&self) -> String {
        format!(
            "# Soul System Configuration\n\
             clinical_port = {}\n\
             cluster_address = \"{}\"\n\
             scout_timeout_secs = {}\n\
             thermal_limit_celsius = {}\n\
             autograd_sensitivity = {}\n\
             firewall_threshold = {}\n\
             kv_cache_window = {}\n\
             kv_cache_sinks = {}\n",
            self.clinical_port,
            self.cluster_address,
            self.scout_timeout_secs,
            self.thermal_limit_celsius,
            self.autograd_sensitivity,
            self.firewall_threshold,
            self.kv_cache_window,
            self.kv_cache_sinks,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let config = SoulConfig::default();
        assert_eq!(config.clinical_port, 8080);
        assert_eq!(config.cluster_address, "127.0.0.1:48999");
        assert_eq!(config.scout_timeout_secs, 5);
        assert_eq!(config.thermal_limit_celsius, 80);
        assert!((config.autograd_sensitivity - 0.5).abs() < f32::EPSILON);
        assert!((config.firewall_threshold - 0.85).abs() < f32::EPSILON);
        assert_eq!(config.kv_cache_window, 16);
        assert_eq!(config.kv_cache_sinks, 2);
    }

    #[test]
    fn from_toml_overrides_defaults() {
        let toml = r#"
            clinical_port = 9090
            cluster_address = "10.0.0.1:5000"
            scout_timeout_secs = 10
            autograd_sensitivity = 1.0
        "#;
        let config = SoulConfig::from_toml(toml);
        assert_eq!(config.clinical_port, 9090);
        assert_eq!(config.cluster_address, "10.0.0.1:5000");
        assert_eq!(config.scout_timeout_secs, 10);
        assert!((config.autograd_sensitivity - 1.0).abs() < f32::EPSILON);
        // Non spécifiés -> défaut
        assert_eq!(config.thermal_limit_celsius, 80);
        assert_eq!(config.kv_cache_window, 16);
    }

    #[test]
    fn to_toml_roundtrip() {
        let config = SoulConfig::default();
        let toml = config.to_toml();
        let restored = SoulConfig::from_toml(&toml);
        assert_eq!(config.clinical_port, restored.clinical_port);
        assert_eq!(config.cluster_address, restored.cluster_address);
        assert_eq!(config.scout_timeout_secs, restored.scout_timeout_secs);
    }
}
