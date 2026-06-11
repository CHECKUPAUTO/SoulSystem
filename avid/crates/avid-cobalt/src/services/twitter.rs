//! Twitter/X extracteur — vidéos, gifs, carrousels
//! Porté depuis cobalt.tools (imputnet/cobalt)

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;

pub struct TwitterExtractor {
    client: Client,
}

impl Default for TwitterExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl TwitterExtractor {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn extract(&self, tweet_id: &str) -> Result<TweetResult> {
        let api_url = format!("https://api.twitter.com/2/tweets/{tweet_id}?expansions=attachments.media_keys&media.fields=duration_ms,height,width,url,type,variants");

        // Try with guest token
        let guest_token = self.get_guest_token().await?;
        let resp = self.client.get(&api_url)
            .header("Authorization", "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA")
            .header("x-guest-token", &guest_token)
            .send()
            .await?
            .json::<Value>()
            .await?;

        let includes = resp
            .get("includes")
            .ok_or_else(|| anyhow!("No media in tweet"))?;

        let media = includes["media"]
            .as_array()
            .ok_or_else(|| anyhow!("No media array"))?;

        let mut items = Vec::new();
        for item in media {
            let media_type = item["type"].as_str().unwrap_or("photo");
            match media_type {
                "video" | "animated_gif" => {
                    if let Some(variants) = item["variants"].as_array() {
                        // Best bitrate variant
                        let best = variants
                            .iter()
                            .filter(|v| v["bitrate"].as_u64().is_some())
                            .max_by_key(|v| v["bitrate"].as_u64().unwrap_or(0))
                            .or_else(|| variants.first());
                        if let Some(variant) = best {
                            if let Some(url) = variant["url"].as_str() {
                                items.push(MediaItem {
                                    url: url.to_string(),
                                    media_type: "video".into(),
                                    width: item["width"].as_u64().unwrap_or(0) as u32,
                                    height: item["height"].as_u64().unwrap_or(0) as u32,
                                });
                            }
                        }
                    }
                }
                "photo" => {
                    if let Some(url) = item["url"].as_str() {
                        items.push(MediaItem {
                            url: url.to_string(),
                            media_type: "photo".into(),
                            width: item["width"].as_u64().unwrap_or(0) as u32,
                            height: item["height"].as_u64().unwrap_or(0) as u32,
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(TweetResult { items })
    }

    async fn get_guest_token(&self) -> Result<String> {
        let resp = self.client.post("https://api.twitter.com/1.1/guest/activate.json")
            .header("Authorization", "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA")
            .send()
            .await?
            .json::<Value>()
            .await?;

        resp["guest_token"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("Failed to get guest token"))
    }
}

#[derive(Debug)]
pub struct MediaItem {
    pub url: String,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct TweetResult {
    pub items: Vec<MediaItem>,
}
