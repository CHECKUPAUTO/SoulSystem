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

use std::collections::HashSet;

/// A checked link with its HTTP status.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinkCheck {
    pub url: String,
    pub status: Option<u16>,
    pub is_broken: bool,
}

/// Check a list of URLs for broken links (simple HEAD requests).
pub async fn check_broken_links(urls: &[String]) -> Vec<LinkCheck> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("AVID-Scout/1.0")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let mut results = Vec::new();
    let mut seen = HashSet::new();
    for url in urls {
        if !seen.insert(url.clone()) {
            continue;
        }
        let status = client
            .head(url)
            .send()
            .await
            .ok()
            .map(|r| r.status().as_u16());
        let is_broken = status.map_or(true, |s| s >= 400);
        results.push(LinkCheck {
            url: url.clone(),
            status,
            is_broken,
        });
    }
    results
}
