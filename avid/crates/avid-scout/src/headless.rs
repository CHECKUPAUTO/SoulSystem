//! Real headless browser session backed by Chromium via CDP.
//!
//! A single `HeadlessBrowser` is meant to be shared as `Arc<HeadlessBrowser>`
//! across the ScoutEngine for the lifetime of the process — launching
//! Chromium costs ~1–2 seconds, so per-request startup is prohibitive.
//! Pages are created on demand and dropped when the work is done.
//!
//! Requires `chromium` or `google-chrome` in PATH. Override with the
//! `BROWSER_PATH` environment variable.

use std::sync::Arc;
use std::time::Duration;

use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::page::{
    AddScriptToEvaluateOnNewDocumentParams, CaptureScreenshotFormat, CaptureScreenshotParams,
};
use chromiumoxide::page::Page;
use futures::StreamExt;
use tokio::sync::Mutex;
use tracing::{debug, error};

use crate::stealth;

#[derive(Debug, thiserror::Error)]
pub enum HeadlessError {
    #[error("chromium launch failed: {0}")]
    Launch(String),

    #[error("navigation failed: {0}")]
    Navigation(String),

    #[error("page operation failed: {0}")]
    Page(String),

    #[error("timeout after {0}ms")]
    Timeout(u64),

    #[error("browser closed")]
    Closed,

    #[error("chromiumoxide: {0}")]
    Inner(#[from] chromiumoxide::error::CdpError),
}

/// Shared headless browser. Spawn once per process, share via Arc.
pub struct HeadlessBrowser {
    browser: Mutex<Browser>,
    _handler: tokio::task::JoinHandle<()>,
}

impl std::fmt::Debug for HeadlessBrowser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeadlessBrowser").finish_non_exhaustive()
    }
}

impl HeadlessBrowser {
    /// Launch Chromium and start the CDP event loop.
    ///
    /// # Errors
    /// Returns `HeadlessError::Launch` if Chromium isn't installed or
    /// fails to start.
    pub async fn launch() -> Result<Self, HeadlessError> {
        let mut config_builder = BrowserConfig::builder()
            .no_sandbox()
            .window_size(1920, 1080)
            .request_timeout(Duration::from_secs(30));

        if let Ok(path) = std::env::var("BROWSER_PATH") {
            config_builder = config_builder.chrome_executable(path);
        }

        let config = config_builder.build().map_err(HeadlessError::Launch)?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .map_err(|e| HeadlessError::Launch(e.to_string()))?;

        let handler_task = tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if let Err(e) = event {
                    error!("CDP handler error: {e}");
                }
            }
            debug!("CDP handler exited");
        });

        Ok(Self {
            browser: Mutex::new(browser),
            _handler: handler_task,
        })
    }

    /// Open a new tab with a fixed viewport. Stealth evasions and a
    /// realistic User-Agent are applied before the first navigation.
    ///
    /// # Errors
    /// Returns `HeadlessError::Page` if the page can't be created.
    pub async fn new_session(
        self: &Arc<Self>,
        viewport_w: u32,
        viewport_h: u32,
    ) -> Result<HeadlessSession, HeadlessError> {
        let browser = self.browser.lock().await;
        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| HeadlessError::Page(e.to_string()))?;
        drop(browser);

        let params = chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams::builder()
            .width(i64::from(viewport_w))
            .height(i64::from(viewport_h))
            .device_scale_factor(1.0)
            .mobile(false)
            .build()
            .map_err(HeadlessError::Page)?;
        page.execute(params)
            .await
            .map_err(|e| HeadlessError::Page(e.to_string()))?;

        // Inject stealth evasions BEFORE any page navigation.
        let stealth_params = AddScriptToEvaluateOnNewDocumentParams::builder()
            .source(stealth::build_stealth_script().to_string())
            .build()
            .map_err(|e| HeadlessError::Page(format!("stealth build: {e}")))?;
        page.execute(stealth_params)
            .await
            .map_err(|e| HeadlessError::Page(format!("stealth inject: {e}")))?;

        // Override User-Agent (also touches navigator.userAgent + HTTP headers).
        let ua_params =
            chromiumoxide::cdp::browser_protocol::emulation::SetUserAgentOverrideParams::builder()
                .user_agent(stealth::realistic_user_agent())
                .build()
                .map_err(|e| HeadlessError::Page(format!("ua build: {e}")))?;
        page.execute(ua_params)
            .await
            .map_err(|e| HeadlessError::Page(format!("set_user_agent: {e}")))?;

        // Override navigator.platform.
        let platform_js = format!(
            r"Object.defineProperty(navigator, 'platform', {{ get: () => '{}' }});",
            stealth::REALISTIC_PLATFORM
        );
        let plat_params = AddScriptToEvaluateOnNewDocumentParams::builder()
            .source(platform_js)
            .build()
            .map_err(|e| HeadlessError::Page(format!("platform build: {e}")))?;
        page.execute(plat_params)
            .await
            .map_err(|e| HeadlessError::Page(format!("platform inject: {e}")))?;

        Ok(HeadlessSession { page: Some(page) })
    }
}

/// A single page session. Cheap to create from a shared `HeadlessBrowser`.
#[derive(Debug)]
pub struct HeadlessSession {
    page: Option<Page>,
}

impl HeadlessSession {
    fn page(&self) -> Result<&Page, HeadlessError> {
        self.page.as_ref().ok_or(HeadlessError::Closed)
    }

    /// Navigate to `url`, wait for `load` event, return fully-rendered HTML.
    ///
    /// # Errors
    /// Returns `HeadlessError::Timeout` if navigation takes longer than
    /// `timeout_ms`, or `HeadlessError::Navigation` if the goto fails.
    pub async fn fetch_rendered(
        &self,
        url: &str,
        timeout_ms: u64,
    ) -> Result<String, HeadlessError> {
        let page = self.page()?;
        let timeout = Duration::from_millis(timeout_ms);

        let nav = page.goto(url);
        tokio::time::timeout(timeout, nav)
            .await
            .map_err(|_| HeadlessError::Timeout(timeout_ms))?
            .map_err(|e| HeadlessError::Navigation(e.to_string()))?;

        let wait = page.wait_for_navigation();
        let _ = tokio::time::timeout(timeout, wait).await;

        // Post-load grace for client-side hydration.
        tokio::time::sleep(Duration::from_millis(300)).await;

        page.content()
            .await
            .map_err(|e| HeadlessError::Page(e.to_string()))
    }

    /// Capture a real PNG screenshot of the current page.
    ///
    /// # Errors
    /// Returns `HeadlessError::Page` if the capture fails.
    pub async fn screenshot(&self, full_page: bool) -> Result<Vec<u8>, HeadlessError> {
        let page = self.page()?;
        let params = CaptureScreenshotParams::builder()
            .format(CaptureScreenshotFormat::Png)
            .capture_beyond_viewport(full_page)
            .build();
        page.screenshot(params)
            .await
            .map_err(|e| HeadlessError::Page(e.to_string()))
    }

    /// Evaluate a JS expression and return the result.
    ///
    /// # Errors
    /// Returns `HeadlessError::Page` if evaluation fails.
    pub async fn eval(&self, expr: &str) -> Result<serde_json::Value, HeadlessError> {
        let page = self.page()?;
        let result = page
            .evaluate(expr)
            .await
            .map_err(|e| HeadlessError::Page(e.to_string()))?;
        result
            .into_value()
            .map_err(|e| HeadlessError::Page(e.to_string()))
    }

    /// Wait for a CSS selector to appear, up to `timeout_ms`.
    ///
    /// # Errors
    /// Returns `HeadlessError::Timeout` if the selector doesn't appear.
    pub async fn wait_for(&self, selector: &str, timeout_ms: u64) -> Result<(), HeadlessError> {
        let page = self.page()?;
        let start = std::time::Instant::now();
        let deadline = Duration::from_millis(timeout_ms);
        loop {
            if page.find_element(selector).await.is_ok() {
                return Ok(());
            }
            if start.elapsed() >= deadline {
                return Err(HeadlessError::Timeout(timeout_ms));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Focus a CSS selector and type text by clicking the element then
    /// using the native chromiumoxide Element::type_str() which sends
    /// proper CDP input events that Angular detects.
    ///
    /// # Errors
    /// Returns `HeadlessError::Page` if the element isn't found.
    pub async fn type_into(&self, selector: &str, text: &str) -> Result<(), HeadlessError> {
        let page = self.page()?;

        // Clear via JS first
        let clear_js = format!(
            "const el = document.querySelector('{}'); if(el) el.value = '';",
            selector.replace('\'', "\\'"),
        );
        let _ = page.evaluate(clear_js).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Find the element via chromiumoxide's native API
        let element = page
            .find_element(selector)
            .await
            .map_err(|e| HeadlessError::Page(format!("find_element {selector}: {e}")))?;

        // Click to focus (sends real CDP mouse + focus events)
        element
            .click()
            .await
            .map_err(|e| HeadlessError::Page(format!("click {selector}: {e}")))?;

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Type via chromiumoxide's native type_str (CDP Input.dispatchKeyEvent)
        element
            .type_str(text)
            .await
            .map_err(|e| HeadlessError::Page(format!("type_str {selector}: {e}")))?;

        Ok(())
    }
}

impl Drop for HeadlessSession {
    fn drop(&mut self) {
        // Page is closed by chromiumoxide when dropped. We use Option
        // so Drop is well-defined without unsafe.
        let _ = self.page.take();
    }
}
