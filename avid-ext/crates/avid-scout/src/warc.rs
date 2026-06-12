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
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// Write a single WARC record (simplified ISO 28500 format).
pub fn write_warc_record(
    writer: &mut impl Write,
    target_uri: &str,
    content_type: &str,
    payload: &[u8],
) -> std::io::Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let date = format_date(now);
    let record_id = format!("<urn:uuid:{}-{}", now, payload.len());

    let header = format!(
        "WARC/1.0\r\nWARC-Type: response\r\nWARC-Target-URI: {}\r\nWARC-Date: {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nWARC-Record-ID: {}\r\n\r\n",
        target_uri,
        date,
        content_type,
        payload.len(),
        record_id,
    );

    writer.write_all(header.as_bytes())?;
    writer.write_all(payload)?;
    writer.write_all(b"\r\n\r\n")?;
    Ok(())
}

fn format_date(epoch: u64) -> String {
    let days = epoch / 86400;
    let rem = epoch % 86400;
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        1970 + days / 365,
        (days % 365) / 30 + 1,
        days % 30 + 1,
        hh,
        mm,
        ss
    )
}
