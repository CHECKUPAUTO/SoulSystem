//! YouTube extracteur — vidéos, shorts, DASH/HLS
//! Porté depuis cobalt.tools (imputnet/cobalt)

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;

pub struct YoutubeExtractor {
    client: Client,
}

impl Default for YoutubeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl YoutubeExtractor {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    /// Extrait les infos de téléchargement d'une vidéo YouTube
    pub async fn extract(&self, video_id: &str) -> Result<YoutubeResult> {
        // Phase 1: Récupérer les infos de la vidéo
        let info = self.fetch_video_info(video_id).await?;

        // Phase 2: Extraire les formats disponibles
        let formats = self.extract_formats(&info)?;

        // Phase 3: Sélectionner le meilleur format
        let best = formats
            .into_iter()
            .max_by(|a, b| {
                let a_score = (a.width * a.height) as f64 / a.filesize as f64;
                let b_score = (b.width * b.height) as f64 / b.filesize as f64;
                a_score
                    .partial_cmp(&b_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| anyhow!("No suitable format found"))?;

        Ok(YoutubeResult {
            title: info.title,
            formats: vec![best],
            duration: info.duration,
        })
    }

    async fn fetch_video_info(&self, video_id: &str) -> Result<VideoInfo> {
        let url = format!("https://www.youtube.com/watch?v={video_id}");
        let html = self
            .client
            .get(&url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .send()
            .await?
            .text()
            .await?;

        // Extract ytInitialData from the page
        let json_start = html
            .find("ytInitialData")
            .and_then(|i| html[i..].find('{'))
            .map(|i| i + html[..i].rfind("ytInitialData").unwrap_or(0));

        let json_end = html
            .rfind("ytInitialData")
            .and_then(|i| html[i..].find("};"))
            .map(|i| i + 1);

        let info = match (json_start, json_end) {
            (Some(start), Some(end)) => {
                let json_str = &html[start..start + end];
                serde_json::from_str::<Value>(json_str)?
            }
            _ => {
                // Fallback: use innertube API
                self.fetch_innertube(video_id).await?
            }
        };

        let title = info["videoDetails"]["title"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let duration = info["videoDetails"]["lengthSeconds"]
            .as_str()
            .unwrap_or("0")
            .parse::<u64>()
            .unwrap_or(0);

        Ok(VideoInfo {
            title,
            duration,
            raw: info,
        })
    }

    async fn fetch_innertube(&self, video_id: &str) -> Result<Value> {
        let url = "https://www.youtube.com/youtubei/v1/player?key=AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8";
        let body = serde_json::json!({
            "videoId": video_id,
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": "2.20240101.00.00"
                }
            }
        });

        let resp = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;

        Ok(resp)
    }

    fn extract_formats(&self, info: &VideoInfo) -> Result<Vec<VideoFormat>> {
        let mut formats = Vec::new();

        // Extract from streamingData
        if let Some(streaming) = info.raw.get("streamingData") {
            // Adaptive formats (DASH)
            if let Some(adaptive) = streaming["adaptiveFormats"].as_array() {
                for fmt in adaptive {
                    if let (Some(w), Some(h), Some(url), Some(mime)) = (
                        fmt["width"].as_u64(),
                        fmt["height"].as_u64(),
                        fmt["url"].as_str(),
                        fmt["mimeType"].as_str(),
                    ) {
                        let content_len = fmt["contentLength"]
                            .as_str()
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(0);
                        formats.push(VideoFormat {
                            itag: fmt["itag"].as_u64().unwrap_or(0) as u32,
                            width: w as u32,
                            height: h as u32,
                            url: url.to_string(),
                            mime_type: mime.to_string(),
                            filesize: content_len,
                            fps: fmt["fps"].as_u64().unwrap_or(30) as u32,
                        });
                    }
                }
            }

            // Progressive formats (combined audio+video)
            if let Some(formats_raw) = streaming["formats"].as_array() {
                for fmt in formats_raw {
                    if let (Some(w), Some(h), Some(url), Some(mime)) = (
                        fmt["width"].as_u64(),
                        fmt["height"].as_u64(),
                        fmt["url"].as_str(),
                        fmt["mimeType"].as_str(),
                    ) {
                        formats.push(VideoFormat {
                            itag: fmt["itag"].as_u64().unwrap_or(0) as u32,
                            width: w as u32,
                            height: h as u32,
                            url: url.to_string(),
                            mime_type: mime.to_string(),
                            filesize: 0,
                            fps: fmt["fps"].as_u64().unwrap_or(30) as u32,
                        });
                    }
                }
            }
        }

        formats.sort_by_key(|b| std::cmp::Reverse(b.height));
        Ok(formats)
    }
}

#[derive(Debug)]
pub struct VideoInfo {
    pub title: String,
    pub duration: u64,
    pub raw: Value,
}

#[derive(Debug, Clone)]
pub struct VideoFormat {
    pub itag: u32,
    pub width: u32,
    pub height: u32,
    pub url: String,
    pub mime_type: String,
    pub filesize: u64,
    pub fps: u32,
}

#[derive(Debug)]
pub struct YoutubeResult {
    pub title: String,
    pub formats: Vec<VideoFormat>,
    pub duration: u64,
}
