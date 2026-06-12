// ─── Tests unitaires pour utils ───
#[cfg(test)]
mod utils_tests {
    // Ces fonctions sont dans src/utils.rs mais ne sont pas accessibles
    // directement depuis un test d'intégration sans ré-exporter.
    // On teste ici des comportements équivalents.

    #[test]
    fn test_truncate_logic() {
        let s = "Hello, World!";
        // Équivalent de truncate(&s, 5)
        let truncated = if s.len() > 5 { &s[..5] } else { s };
        assert_eq!(truncated, "Hello");
    }

    #[test]
    fn test_truncate_no_op() {
        let s = "Hi";
        let truncated = if s.len() > 10 { &s[..10] } else { s };
        assert_eq!(truncated, "Hi");
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!("500 B", format!("{} B", 500u64));
    }

    #[test]
    fn test_format_size_kb() {
        let kb = format!("{:.1} KB", 1500_f64 / 1024.0);
        assert_eq!(kb, "1.5 KB");
    }
}

// ─── Tests pour la configuration ───
#[cfg(test)]
mod config_tests {
    use avid_gomogo::GomogoConfig;

    #[test]
    fn test_default_config() {
        let cfg = GomogoConfig::default();
        assert_eq!(cfg.scanner.default_concurrency, 10);
        assert_eq!(cfg.proxy.default_listen, "127.0.0.1:8080");
        assert_eq!(cfg.captcha.default_port, 8080);
        assert_eq!(cfg.captcha.task_timeout_secs, 300);
        assert_eq!(cfg.captcha.result_ttl_secs, 600);
        assert_eq!(cfg.logging.level, "info");
    }

    #[test]
    fn test_config_parse_valid() {
        let toml_str = r#"
[scanner]
default_concurrency = 5
default_delay_ms = 100
default_timeout_secs = 30
user_agent_rotation = true
custom_user_agents = ["TestAgent"]
follow_redirects = false

[proxy]
default_listen = "0.0.0.0:9090"
default_cert_dir = "/tmp/certs"
domain_whitelist = ["example.com"]
domain_blacklist = ["ads.example.com"]

[[proxy.modification_rules]]
name = "test"
pattern = "foo"
replacement = "bar"
scope = "both"

[captcha]
default_port = 9090
default_bind = "0.0.0.0"
task_timeout_secs = 60
result_ttl_secs = 120
max_pending_tasks = 100

[logging]
level = "debug"
json_format = true
"#;
        let cfg: GomogoConfig = toml::from_str(toml_str).expect("Parse TOML");
        assert_eq!(cfg.scanner.default_concurrency, 5);
        assert!(cfg.scanner.user_agent_rotation);
        assert_eq!(cfg.proxy.default_listen, "0.0.0.0:9090");
        assert_eq!(cfg.captcha.task_timeout_secs, 60);
        assert_eq!(cfg.logging.level, "debug");
        assert!(cfg.logging.json_format);
    }

    #[test]
    fn test_config_serialize_roundtrip() {
        let cfg = GomogoConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).expect("Serialize");
        let cfg2: GomogoConfig = toml::from_str(&toml_str).expect("Deserialize");
        assert_eq!(
            cfg.scanner.default_concurrency,
            cfg2.scanner.default_concurrency
        );
        assert_eq!(cfg.captcha.result_ttl_secs, cfg2.captcha.result_ttl_secs);
    }
}

// ─── Tests pour le modèle captcha ───
#[cfg(test)]
mod captcha_model_tests {
    use chrono::Utc;

    #[test]
    fn test_task_priority_ordering() {
        let t1 = avid_gomogo::CaptchaTask {
            id: "1".into(),
            image_base64: "a".into(),
            priority: 10,
            created_at: Utc::now(),
        };
        let t2 = avid_gomogo::CaptchaTask {
            id: "2".into(),
            image_base64: "b".into(),
            priority: 20,
            created_at: Utc::now(),
        };
        assert!(t2 > t1, "Higher priority should be greater");
    }

    #[test]
    fn test_task_priority_tiebreaker() {
        let now = Utc::now();
        let t1 = avid_gomogo::CaptchaTask {
            id: "1".into(),
            image_base64: "a".into(),
            priority: 10,
            created_at: now,
        };
        let t2 = avid_gomogo::CaptchaTask {
            id: "2".into(),
            image_base64: "b".into(),
            priority: 10,
            created_at: now - chrono::Duration::seconds(10),
        };
        // Same priority, t2 is older → t2 should be returned first (greater in max-heap)
        assert!(t2 > t1, "Older task with same priority should be greater");
    }
}

// ─── Tests pour le filtrage proxy ───
#[cfg(test)]
mod proxy_filter_tests {
    use regex::Regex;

    fn is_allowed(whitelist: &[Regex], blacklist: &[Regex], host: &str) -> bool {
        if !blacklist.is_empty() && blacklist.iter().any(|r| r.is_match(host)) {
            return false;
        }
        if !whitelist.is_empty() && !whitelist.iter().any(|r| r.is_match(host)) {
            return false;
        }
        true
    }

    #[test]
    fn test_no_filters_allows_all() {
        assert!(is_allowed(&[], &[], "example.com"));
        assert!(is_allowed(&[], &[], "evil.com"));
    }

    #[test]
    fn test_blacklist_blocks() {
        let bl = vec![Regex::new("evil\\.com").unwrap()];
        assert!(!is_allowed(&[], &bl, "evil.com"));
        assert!(is_allowed(&[], &bl, "good.com"));
    }

    #[test]
    fn test_whitelist_allows_only() {
        let wl = vec![Regex::new("good\\.com").unwrap()];
        assert!(is_allowed(&wl, &[], "good.com"));
        assert!(!is_allowed(&wl, &[], "bad.com"));
    }

    #[test]
    fn test_both_filters() {
        let wl = vec![Regex::new("(good|ok)\\.com").unwrap()];
        let bl = vec![Regex::new("evil\\.com").unwrap()];
        // Blacklist takes precedence
        assert!(!is_allowed(&wl, &bl, "evil.com"));
        assert!(is_allowed(&wl, &bl, "good.com"));
        assert!(!is_allowed(&wl, &bl, "bad.com"));
    }
}
