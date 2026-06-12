use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;

pub enum PlayerBackend { CDP, WebDriver }

/// Controls browser playback of actions.
pub struct BrowserPlayer { backend: PlayerBackend }

impl BrowserPlayer {
    pub fn new(backend: PlayerBackend) -> Self { Self { backend } }
    pub async fn play_url(&self, url: &str) -> Result<(), anyhow::Error> {
        match self.backend {
            PlayerBackend::CDP => {
                let (mut browser, mut handler) = Browser::launch(BrowserConfig::builder().build().map_err(|e| anyhow::anyhow!(e))?).await?;
                tokio::task::spawn(async move { while let Some(h) = handler.next().await { if h.is_err() { break; } } });
                let page = browser.new_page(url).await?;
                page.wait_for_navigation().await?;
                browser.close().await?;
            }
            PlayerBackend::WebDriver => {}
        }
        Ok(())
    }
}
