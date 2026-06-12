#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
use std::collections::HashSet;

/// Detected technologies on a page.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TechStack {
    pub technologies: Vec<String>,
}

/// Detect technologies from HTML body and response headers.
pub fn detect_technologies(html: &str, headers: &[(String, String)]) -> TechStack {
    let mut tech = HashSet::new();
    let lower = html.to_lowercase();

    if lower.contains("react") || lower.contains("data-reactroot") || lower.contains("__next") {
        tech.insert("React".to_string());
    }
    if lower.contains("vue.js") || lower.contains("vue-router") || lower.contains("data-v-") {
        tech.insert("Vue.js".to_string());
    }
    if lower.contains("angular") || lower.contains("ng-app") {
        tech.insert("Angular".to_string());
    }
    if lower.contains("jquery") {
        tech.insert("jQuery".to_string());
    }
    if lower.contains("bootstrap") {
        tech.insert("Bootstrap".to_string());
    }
    if lower.contains("tailwind") {
        tech.insert("Tailwind CSS".to_string());
    }
    if lower.contains("next.js") || lower.contains("__next") {
        tech.insert("Next.js".to_string());
    }
    if lower.contains("astro") || lower.contains("data-astro") {
        tech.insert("Astro".to_string());
    }
    if lower.contains("wordpress") || lower.contains("wp-content") || lower.contains("wp-includes")
    {
        tech.insert("WordPress".to_string());
    }
    if lower.contains("drupal") {
        tech.insert("Drupal".to_string());
    }
    if lower.contains("django") {
        tech.insert("Django".to_string());
    }
    if lower.contains("ruby on rails") || lower.contains("rails") {
        tech.insert("Ruby on Rails".to_string());
    }
    if lower.contains("laravel") {
        tech.insert("Laravel".to_string());
    }

    for (k, v) in headers {
        if k.eq_ignore_ascii_case("x-powered-by") {
            if v.to_lowercase().contains("php") {
                tech.insert("PHP".to_string());
            }
            if v.to_lowercase().contains("express") {
                tech.insert("Express.js".to_string());
            }
        }
        if k.eq_ignore_ascii_case("server") {
            if v.to_lowercase().contains("nginx") {
                tech.insert("Nginx".to_string());
            }
            if v.to_lowercase().contains("apache") {
                tech.insert("Apache".to_string());
            }
            if v.to_lowercase().contains("cloudflare") {
                tech.insert("Cloudflare".to_string());
            }
        }
    }

    if lower.contains("shopify") || lower.contains("myshopify") {
        tech.insert("Shopify".to_string());
    }
    if lower.contains("gatsby") {
        tech.insert("Gatsby".to_string());
    }
    if lower.contains("svelte") {
        tech.insert("Svelte".to_string());
    }

    let mut technologies: Vec<String> = tech.into_iter().collect();
    technologies.sort();
    TechStack { technologies }
}
