//! Real screenshot capture via the shared headless browser.
//!
//! Previous versions of this file lied about success: they returned a
//! placeholder `ScreenshotInfo` with hardcoded dimensions and no actual
//! capture. This version returns `captured: false` when no browser is
//! available and never fabricates results.

use std::sync::Arc;

use crate::headless::{HeadlessBrowser, HeadlessError};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotInfo {
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub full_page: bool,
    /// Real PNG bytes. `None` when no capture was performed.
    #[serde(skip)]
    pub bytes: Option<Vec<u8>>,
    /// True iff `bytes` is populated by an actual capture.
    pub captured: bool,
}

impl ScreenshotInfo {
    /// Placeholder used when no browser is available or capture failed.
    #[must_use]
    pub fn unavailable(viewport_w: u32, viewport_h: u32, full_page: bool) -> Self {
        Self {
            width: viewport_w,
            height: viewport_h,
            format: "png".to_string(),
            full_page,
            bytes: None,
            captured: false,
        }
    }
}

/// Real screenshot capture.
///
/// Returns `captured: false` when no browser is available. Never lies
/// about success.
pub async fn capture_screenshot(
    browser: Option<&Arc<HeadlessBrowser>>,
    url: &str,
    viewport_w: u32,
    viewport_h: u32,
    full_page: bool,
) -> ScreenshotInfo {
    let Some(browser) = browser else {
        return ScreenshotInfo::unavailable(viewport_w, viewport_h, full_page);
    };

    match try_capture(browser, url, viewport_w, viewport_h, full_page).await {
        Ok(bytes) => {
            let (w, h) = decode_png_dimensions(&bytes).unwrap_or((viewport_w, viewport_h));
            ScreenshotInfo {
                width: w,
                height: h,
                format: "png".to_string(),
                full_page,
                bytes: Some(bytes),
                captured: true,
            }
        }
        Err(e) => {
            tracing::warn!("screenshot capture failed for {url}: {e}");
            ScreenshotInfo::unavailable(viewport_w, viewport_h, full_page)
        }
    }
}

async fn try_capture(
    browser: &Arc<HeadlessBrowser>,
    url: &str,
    viewport_w: u32,
    viewport_h: u32,
    full_page: bool,
) -> Result<Vec<u8>, HeadlessError> {
    let session = browser.new_session(viewport_w, viewport_h).await?;
    let _ = session.fetch_rendered(url, 15_000).await?;
    session.screenshot(full_page).await
}

/// Minimal PNG header parser. Returns None on malformed input.
fn decode_png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_does_not_pretend_to_capture() {
        let info = ScreenshotInfo::unavailable(1920, 1080, true);
        assert!(!info.captured);
        assert!(info.bytes.is_none());
    }

    #[test]
    fn png_dimension_decoder_matches_header() {
        // Smallest valid PNG: 1x1 transparent (well-known fixture).
        let png_1x1: [u8; 67] = [
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ];
        assert_eq!(decode_png_dimensions(&png_1x1), Some((1, 1)));
    }

    #[test]
    fn png_dimension_decoder_rejects_garbage() {
        assert_eq!(decode_png_dimensions(b"not a png at all"), None);
        assert_eq!(decode_png_dimensions(&[]), None);
    }
}
