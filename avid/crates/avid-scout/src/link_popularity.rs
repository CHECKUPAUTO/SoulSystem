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

/// Link popularity metrics.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LinkPopularity {
    pub internal_links: usize,
    pub external_links: usize,
    pub unique_domains: usize,
    pub text_to_link_ratio: f32,
}

/// Compute link popularity from raw extracted links and HTML.
#[must_use]
pub fn compute_link_popularity(links: &[impl AsRef<str>], text: &str) -> LinkPopularity {
    let mut internal = 0;
    let mut external = 0;
    let mut domains = std::collections::HashSet::new();
    let text_words = text.split_whitespace().count().max(1);

    for link in links {
        let l = link.as_ref();
        if l.starts_with("http") {
            external += 1;
            if let Ok(url) = url::Url::parse(l) {
                if let Some(host) = url.host_str() {
                    domains.insert(host.to_lowercase());
                }
            }
        } else if l.starts_with('/') || l.starts_with('#') || l.starts_with('.') {
            internal += 1;
        }
    }

    LinkPopularity {
        internal_links: internal,
        external_links: external,
        unique_domains: domains.len(),
        text_to_link_ratio: links.len() as f32 / text_words as f32,
    }
}
