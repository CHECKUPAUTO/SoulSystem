#![allow(
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

/// Security scan results for an HTTP response.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SecurityReport {
    pub headers_present: Vec<String>,
    pub headers_missing: Vec<String>,
    pub has_https: bool,
    pub score: u8,
}

const SECURITY_HEADERS: &[(&str, &str)] = &[
    ("strict-transport-security", "HSTS"),
    ("content-security-policy", "CSP"),
    ("x-content-type-options", "X-Content-Type-Options"),
    ("x-frame-options", "X-Frame-Options"),
    ("referrer-policy", "Referrer-Policy"),
    ("permissions-policy", "Permissions-Policy"),
];

/// Scan response headers for security best-practices.
#[must_use]
pub fn scan_security_headers(
    header_map: &[(String, String)],
    url_uses_https: bool,
) -> SecurityReport {
    let mut present = Vec::new();
    let mut missing = Vec::new();
    let lower_headers: Vec<String> = header_map.iter().map(|(k, _)| k.to_lowercase()).collect();

    for (header, label) in SECURITY_HEADERS {
        if lower_headers.iter().any(|h| h == *header) {
            present.push((*label).to_string());
        } else {
            missing.push((*label).to_string());
        }
    }

    let total = SECURITY_HEADERS.len() + 1; // +1 for HTTPS
    let found = present.len() + if url_uses_https { 1 } else { 0 };
    let score = ((found * 100) / total).min(100) as u8;

    SecurityReport {
        headers_present: present,
        headers_missing: missing,
        has_https: url_uses_https,
        score,
    }
}
