#![allow(
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unnested_or_patterns,
    clippy::range_plus_one,
    clippy::single_char_pattern,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::significant_drop_tightening
)]
use lopdf::Document;

/// Extract plain text from PDF bytes.
pub fn extract_pdf_text(bytes: &[u8]) -> Result<String, PdfError> {
    let doc = Document::load_from(bytes).map_err(|e| PdfError::Load(e.to_string()))?;
    let mut text = String::new();
    for (_page_num, page_id) in doc.get_pages() {
        if let Ok(page_text) = doc.extract_text(&[page_id.0]) {
            text.push_str(&page_text);
            text.push('\n');
        }
    }
    Ok(text)
}

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("PDF load error: {0}")]
    Load(String),
}
