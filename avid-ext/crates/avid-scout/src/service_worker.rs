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

/// PWA / Service Worker detection.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ServiceWorker {
    pub has_service_worker: bool,
    pub has_web_app_manifest: bool,
    pub has_theme_color: bool,
    pub has_apple_touch_icon: bool,
    pub is_pwa_ready: bool,
    pub manifest_url: Option<String>,
}

/// Detect PWA and service worker signals.
#[must_use]
pub fn detect_service_worker(html: &str) -> ServiceWorker {
    let lower = html.to_lowercase();
    let manifest = lower.lines().find_map(|l| {
        if l.contains("manifest") && l.contains("rel=") {
            if let Some(start) = l.find("href=") {
                let rest = &l[start + 5..];
                let delim = rest.chars().next().unwrap_or('\u{0026}');
                if delim == '\"' || delim == '\u{0026}' {
                    if let Some(end) = rest[1..].find(delim) {
                        return Some(rest[1..end + 1].to_string());
                    }
                }
            }
        }
        None
    });
    let has_manifest = manifest.is_some() || lower.contains("manifest.json");
    let has_sw = lower.contains("navigator.serviceWorker") || lower.contains("serviceworker");
    let theme = lower.contains("theme-color") || lower.contains("theme_color");
    let touch = lower.contains("apple-touch-icon");
    ServiceWorker {
        has_service_worker: has_sw,
        has_web_app_manifest: has_manifest,
        has_theme_color: theme,
        has_apple_touch_icon: touch,
        is_pwa_ready: has_manifest && theme && touch,
        manifest_url: manifest,
    }
}
