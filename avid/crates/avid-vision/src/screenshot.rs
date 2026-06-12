use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;

/// Captures screenshots from web pages using CDP.
pub struct ScreenshotCapturer;

impl ScreenshotCapturer {
    /// Capture a full page screenshot.
    pub async fn capture_full_page(url: &str) -> Result<Vec<u8>, anyhow::Error> {
        let (mut browser, mut handler) = Browser::launch(BrowserConfig::builder().build().map_err(|e| anyhow::anyhow!(e))?).await?;
        let handle = tokio::task::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() { break; }
            }
        });
        let page = browser.new_page(url).await?;
        let screenshot = page.screenshot(chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams::builder().build()).await?;
        browser.close().await?;
        let _ = handle.await;
        Ok(screenshot)
    }
}
