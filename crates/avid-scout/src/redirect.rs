use regex::Regex;

/// Extract meta-refresh URL from HTML if present.
///
/// Matches `<meta http-equiv="refresh" content="5;url=https://example.com">`.
#[must_use]
pub fn extract_meta_refresh(html: &str) -> Option<String> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"meta[^>]*http-equiv\s*=\s*"refresh"[^>]*content\s*=\s*"\d+\s*;\s*url\s*=\s*([^"]+)""#)
            .expect("valid regex")
    });
    re.captures(html)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}
