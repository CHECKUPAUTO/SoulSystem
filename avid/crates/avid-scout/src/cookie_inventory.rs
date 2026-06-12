#![allow(
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::bool_to_int_with_if,
    clippy::collapsible_if,
    clippy::if_not_else,
    clippy::needless_range_loop,
    clippy::uninlined_format_args,
    clippy::use_self,
    clippy::redundant_clone,
    clippy::wildcard_imports,
    clippy::option_if_let_else,
    clippy::manual_split_once,
    clippy::match_wildcard_for_single_variants,
    clippy::single_char_pattern,
    clippy::range_plus_one,
    clippy::unnecessary_map_or,
    clippy::manual_pattern_char_comparison,
    clippy::suboptimal_flops,
    clippy::needless_collect,
    clippy::inefficient_to_string
)]

use regex::Regex;
use std::sync::OnceLock;

/// Cookie entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub domain: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
}

/// Cookie inventory from Set-Cookie headers or inline scripts.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CookieInventory {
    pub total_cookies: usize,
    pub first_party: usize,
    pub third_party: usize,
    pub session_cookies: usize,
    pub persistent_cookies: usize,
    pub entries: Vec<CookieEntry>,
}

fn cookie_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"document\.cookie\s*=\s*[\"\']([^\"\';]+)[\"\']"#).unwrap())
}

/// Inventory cookies from inline scripts (heuristic).
#[must_use]
pub fn inventory_cookies(html: &str, headers: &[(String, String)]) -> CookieInventory {
    let mut entries = Vec::new();
    // Parse Set-Cookie headers
    for (key, value) in headers {
        if key.to_lowercase() == "set-cookie" {
            let parts: Vec<&str> = value.split(';').collect();
            let name_part = parts.get(0).unwrap_or(&"").trim();
            let name = name_part.split('=').next().unwrap_or("").trim().to_string();
            let mut domain = None;
            let mut secure = false;
            let mut http_only = false;
            let mut same_site = None;
            for part in &parts[1..] {
                let p = part.trim().to_lowercase();
                if p.starts_with("domain=") {
                    domain = Some(p.trim_start_matches("domain=").to_string());
                }
                if p == "secure" {
                    secure = true;
                }
                if p == "httponly" {
                    http_only = true;
                }
                if p.starts_with("samesite=") {
                    same_site = Some(p.trim_start_matches("samesite=").to_string());
                }
            }
            if !name.is_empty() {
                entries.push(CookieEntry {
                    name,
                    domain,
                    secure,
                    http_only,
                    same_site,
                });
            }
        }
    }
    // Heuristic from inline scripts
    for cap in cookie_regex().captures_iter(html) {
        if let Some(m) = cap.get(1) {
            let name = m
                .as_str()
                .split('=')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !name.is_empty() && !entries.iter().any(|e| e.name == name) {
                entries.push(CookieEntry {
                    name,
                    domain: None,
                    secure: false,
                    http_only: false,
                    same_site: None,
                });
            }
        }
    }

    let total = entries.len();
    let first_party = entries.iter().filter(|e| e.domain.is_none()).count();
    let third_party = entries.iter().filter(|e| e.domain.is_some()).count();
    let session = entries
        .iter()
        .filter(|e| {
            !e.name.to_lowercase().contains("expir") && !e.name.to_lowercase().contains("max-age")
        })
        .count();
    let persistent = total.saturating_sub(session);

    CookieInventory {
        total_cookies: total,
        first_party,
        third_party,
        session_cookies: session,
        persistent_cookies: persistent,
        entries,
    }
}
