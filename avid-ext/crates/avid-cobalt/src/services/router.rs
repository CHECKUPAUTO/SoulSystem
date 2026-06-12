//! Service router for avid-cobalt.
//!
//! HONEST surface: `detect_service` returns Some(..) ONLY for services
//! we actually support in Rust. Returning Some(..) for a service that
//! `detect_and_extract` will reject with `bail!` is a bug because the
//! caller cannot distinguish "unknown URL" from "known URL, no support".
//!
//! As of now, supported services:
//!   - youtube
//!   - twitter
//!
//! All other platforms (instagram, tiktok, reddit, vimeo, twitch, ...)
//! are NOT implemented. The original cobalt.tools JS modules sit
//! alongside this crate as a reference implementation; they are NOT
//! executed by Cargo. Add Rust ports under `src/services/` and update
//! this router when a new service is ready.

use crate::services::twitter::TwitterExtractor;
use crate::services::youtube::YoutubeExtractor;

/// Set of services that are actually implemented in Rust right now.
/// Order matters for the longest-prefix-first heuristic.
const SUPPORTED: &[(&str, &str)] = &[
    ("youtube.com", "youtube"),
    ("youtu.be", "youtube"),
    ("twitter.com", "twitter"),
    ("x.com", "twitter"),
];

/// Detect the service from `url`. Returns Some(service_name) only when
/// the service has a working Rust implementation. Returns None for any
/// other URL — including URLs from platforms we know but haven't ported
/// (instagram, tiktok, etc.).
#[must_use]
pub fn detect_service(url: &str) -> Option<&'static str> {
    let lower = url.to_lowercase();
    SUPPORTED
        .iter()
        .find_map(|(domain, service)| lower.contains(domain).then_some(*service))
}

/// Set of known-but-unsupported platforms — used to give a more useful
/// error message when the user submits one of these.
const KNOWN_UNSUPPORTED: &[&str] = &[
    "instagram.com",
    "tiktok.com",
    "reddit.com",
    "vimeo.com",
    "twitch.tv",
    "soundcloud.com",
    "facebook.com",
    "fb.watch",
    "bsky.app",
    "dailymotion.com",
    "pinterest.com",
    "tumblr.com",
    "snapchat.com",
    "loom.com",
    "rutube.ru",
    "streamable.com",
    "bilibili.com",
    "vk.com",
    "vk.ru",
    "ok.ru",
];

#[must_use]
pub fn is_known_unsupported(url: &str) -> Option<&'static str> {
    let lower = url.to_lowercase();
    KNOWN_UNSUPPORTED
        .iter()
        .find(|d| lower.contains(*d))
        .copied()
}

/// Extract media URL from a supported service URL.
///
/// # Errors
/// - `ServiceUnknown` if the URL doesn't match any known platform
/// - `ServiceNotPorted` if the URL matches a known but unported platform
/// - Inner extractor errors otherwise
pub async fn detect_and_extract(url: &str) -> anyhow::Result<String> {
    let Some(service) = detect_service(url) else {
        if let Some(platform) = is_known_unsupported(url) {
            anyhow::bail!(
                "Platform '{platform}' is recognized but not yet ported to Rust. \
                 Only youtube and twitter are supported in this build."
            );
        }
        anyhow::bail!("Unknown URL — no service detected for: {url}");
    };

    match service {
        "youtube" => {
            let video_id = url
                .split("v=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .or_else(|| url.split('/').next_back())
                .ok_or_else(|| anyhow::anyhow!("YouTube video ID not found in {url}"))?;
            let result = YoutubeExtractor::new().extract(video_id).await?;
            let format = result
                .formats
                .first()
                .ok_or_else(|| anyhow::anyhow!("No format found for {video_id}"))?;
            Ok(format.url.clone())
        }
        "twitter" => {
            let tweet_id = url
                .split('/')
                .next_back()
                .and_then(|s| s.split('?').next())
                .ok_or_else(|| anyhow::anyhow!("Tweet ID not found in {url}"))?;
            let result = TwitterExtractor::new().extract(tweet_id).await?;
            result
                .items
                .first()
                .map(|i| i.url.clone())
                .ok_or_else(|| anyhow::anyhow!("No media found in tweet {tweet_id}"))
        }
        other => anyhow::bail!("Inconsistent state: detect_service returned '{other}' but no extractor matched. This is a bug."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_known_supported() {
        assert_eq!(detect_service("https://www.youtube.com/watch?v=abc"), Some("youtube"));
        assert_eq!(detect_service("https://youtu.be/abc"), Some("youtube"));
        assert_eq!(detect_service("https://x.com/user/status/123"), Some("twitter"));
        assert_eq!(detect_service("https://twitter.com/user/status/123"), Some("twitter"));
    }

    #[test]
    fn detect_known_unsupported_returns_none_not_some() {
        // Critical: returning Some("tiktok") would mislead the caller into
        // thinking we support TikTok, then bail later. Return None instead.
        assert_eq!(detect_service("https://www.tiktok.com/@user/video/123"), None);
        assert_eq!(detect_service("https://instagram.com/p/abc"), None);
        assert_eq!(detect_service("https://reddit.com/r/x/comments/y"), None);
    }

    #[test]
    fn is_known_unsupported_flags_them() {
        assert_eq!(is_known_unsupported("https://www.tiktok.com/foo"), Some("tiktok.com"));
        assert_eq!(is_known_unsupported("https://example.com/foo"), None);
    }

    #[test]
    fn detect_unknown_url() {
        assert_eq!(detect_service("https://example.com"), None);
        assert_eq!(is_known_unsupported("https://example.com"), None);
    }
}
