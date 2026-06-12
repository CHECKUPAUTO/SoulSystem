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

use reqwest::Client;

/// Archive.org availability.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ArchiveInfo {
    pub available: bool,
    pub snapshot_count: Option<usize>,
    pub earliest_snapshot: Option<String>,
    pub latest_snapshot: Option<String>,
}

/// Query archive.org CDX API for snapshot count.
pub async fn check_archive(url: &str) -> ArchiveInfo {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("AVID-Scout/1.0")
        .build()
        .unwrap_or_else(|_| Client::new());
    let encoded: String = url::form_urlencoded::byte_serialize(url.as_bytes()).collect();
    let api_url = format!(
        "https://web.archive.org/cdx/search/cdx?url={}&output=json&limit=1",
        encoded
    );
    match client.get(&api_url).send().await {
        Ok(resp) => {
            if let Ok(json) = resp.json::<Vec<Vec<String>>>().await {
                if json.len() > 1 {
                    return ArchiveInfo {
                        available: true,
                        snapshot_count: Some(json.len().saturating_sub(1)),
                        earliest_snapshot: json.get(1).and_then(|r| r.get(1)).cloned(),
                        latest_snapshot: json.last().and_then(|r| r.get(1)).cloned(),
                    };
                }
            }
        }
        Err(_) => {}
    }
    ArchiveInfo {
        available: false,
        ..Default::default()
    }
}
