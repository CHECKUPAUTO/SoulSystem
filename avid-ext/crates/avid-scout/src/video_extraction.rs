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

/// Video.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Video {
    pub url: Option<String>,
    pub embed_url: Option<String>,
    pub title: Option<String>,
    pub platform: String,
    pub duration: Option<String>,
    pub thumbnail: Option<String>,
}

fn youtube_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?:youtube\.com/watch\?v=|youtu\.be/|youtube\.com/embed/)([a-zA-Z0-9_-]+)")
            .unwrap()
    })
}

fn vimeo_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"vimeo\.com/(\d+)").unwrap())
}

/// Extract video references from HTML.
#[must_use]
pub fn extract_videos(html: &str) -> Vec<Video> {
    let mut videos = Vec::new();
    for cap in youtube_regex().captures_iter(html) {
        let id = cap.get(1).map(|m| m.as_str());
        if let Some(id) = id {
            videos.push(Video {
                url: Some(format!("https://www.youtube.com/watch?v={}", id)),
                embed_url: Some(format!("https://www.youtube.com/embed/{}", id)),
                title: None,
                platform: "YouTube".to_string(),
                duration: None,
                thumbnail: Some(format!("https://img.youtube.com/vi/{}/0.jpg", id)),
            });
        }
    }
    for cap in vimeo_regex().captures_iter(html) {
        let id = cap.get(1).map(|m| m.as_str());
        if let Some(id) = id {
            videos.push(Video {
                url: Some(format!("https://vimeo.com/{}", id)),
                embed_url: Some(format!("https://player.vimeo.com/video/{}", id)),
                title: None,
                platform: "Vimeo".to_string(),
                duration: None,
                thumbnail: None,
            });
        }
    }
    videos
}
