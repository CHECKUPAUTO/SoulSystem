use std::time::Duration;

/// Formatte une taille en bytes vers une chaîne lisible
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Tronque une chaîne à `max` caractères
pub fn truncate(s: &str, max: usize) -> &str {
    if s.len() > max {
        &s[..max]
    } else {
        s
    }
}

/// Formate une durée en millisecondes
pub fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        format!("{:.2}s", ms as f64 / 1000.0)
    }
}

/// User-Agents pour la rotation
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_7) AppleWebKit/605.1.15 Safari/605.1.15",
    "Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148",
    "Mozilla/5.0 (Android 15; Mobile; rv:133.0) Gecko/133.0 Firefox/133.0",
];

/// Retourne un User-Agent aléatoire
pub fn random_user_agent() -> &'static str {
    use rand::Rng;
    let idx = rand::thread_rng().gen_range(0..USER_AGENTS.len());
    USER_AGENTS[idx]
}
