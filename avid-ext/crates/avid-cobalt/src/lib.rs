//! avid-cobalt — media downloader engine porté de cobalt.tools
//!
//! Utilise les extracteurs de service JS originaux via un runtime Node.js
//! embarqué (ou via subprocess). Chaque service peut être appelé
//! individuellement depuis Rust.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub mod services;

// ── Types partagés (schéma cobalt compatible) ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub url: String,
    #[serde(default = "default_video_quality")]
    pub video_quality: String,
    #[serde(default = "default_audio_format")]
    pub audio_format: String,
    #[serde(default = "default_audio_bitrate")]
    pub audio_bitrate: String,
    #[serde(default = "default_download_mode")]
    pub download_mode: String,
    #[serde(default = "default_filename_style")]
    pub filename_style: String,
    #[serde(default)]
    pub disable_metadata: bool,
}

fn default_video_quality() -> String { "1080".into() }
fn default_audio_format() -> String { "mp3".into() }
fn default_audio_bitrate() -> String { "128".into() }
fn default_download_mode() -> String { "auto".into() }
fn default_filename_style() -> String { "pretty".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadStatus {
    Tunnel { url: String, filename: String },
    Redirect { url: String },
    Picker { items: Vec<MediaItem> },
    Error { code: String, context: Option<serde_json::Value> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub url: String,
    #[serde(rename = "type")]
    pub media_type: String,
    pub thumb: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub cobalt: CobaltMeta,
    pub git: GitInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CobaltMeta {
    pub version: String,
    pub url: String,
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub commit: String,
    pub branch: String,
    pub remote: String,
}

// ── Engine principal ──────────────────────────────────────────────────

pub struct CobaltEngine {
    services_dir: PathBuf,
    supported_services: Vec<String>,
}

impl CobaltEngine {
    pub fn new() -> Self {
        let services_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("services");

        let supported_services = vec![
            "bilibili".into(), "bluesky".into(), "dailymotion".into(),
            "facebook".into(), "instagram".into(), "loom".into(),
            "newgrounds".into(), "ok".into(), "pinterest".into(),
            "reddit".into(), "rutube".into(), "snapchat".into(),
            "soundcloud".into(), "streamable".into(), "tiktok".into(),
            "tumblr".into(), "twitch".into(), "twitter".into(),
            "vimeo".into(), "vk".into(), "youtube".into(),
        ];

        Self { services_dir, supported_services }
    }

    /// Detect which service handles a URL
    pub fn detect_service(&self, url: &str) -> Option<String> {
        crate::services::router::detect_service(url).map(|s| s.to_string())
    }

    /// List all available services
    pub fn services(&self) -> &[String] {
        &self.supported_services
    }

    /// Get the path to the original JS extractor for a service
    pub fn extractor_path(&self, service: &str) -> PathBuf {
        self.services_dir.join(format!("{service}.js"))
    }

    /// Instance metadata (version compatible)
    pub fn instance_info(&self) -> InstanceInfo {
        InstanceInfo {
            cobalt: CobaltMeta {
                version: "1.0.0-avid".into(),
                url: "https://cobalt.tools/".into(),
                services: self.supported_services.clone(),
            },
            git: GitInfo {
                commit: env!("CARGO_PKG_VERSION").into(),
                branch: "main".into(),
                remote: "https://github.com/CHECKUPAUTO/AVID".into(),
            },
        }
    }
}

impl Default for CobaltEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_youtube() {
        let engine = CobaltEngine::new();
        assert_eq!(engine.detect_service("https://youtube.com/watch?v=test"), Some("youtube".into()));
        assert_eq!(engine.detect_service("https://youtu.be/test"), Some("youtube".into()));
    }

    #[test]
    fn test_detect_twitter() {
        let engine = CobaltEngine::new();
        assert_eq!(engine.detect_service("https://twitter.com/user/status/123"), Some("twitter".into()));
        assert_eq!(engine.detect_service("https://x.com/user/123"), Some("twitter".into()));
    }

    #[test]
    fn test_instance_info() {
        let engine = CobaltEngine::new();
        let info = engine.instance_info();
        assert_eq!(info.cobalt.services.len(), 21);
    }

    #[test]
    fn test_services_list() {
        let engine = CobaltEngine::new();
        let svc = engine.services();
        assert!(svc.contains(&"youtube".to_string()));
        assert!(svc.contains(&"instagram".to_string()));
    }
}
