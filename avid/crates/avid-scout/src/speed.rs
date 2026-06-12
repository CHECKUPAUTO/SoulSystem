#![allow(
    clippy::needless_collect,
    clippy::suboptimal_flops,
    clippy::manual_pattern_char_comparison,
    clippy::if_not_else,
    clippy::bool_to_int_with_if,
    clippy::cast_precision_loss,
    clippy::single_match,
    clippy::match_same_arms,
    clippy::unused_async,
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::cognitive_complexity
)]

use std::time::Duration;

/// Page speed metrics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SpeedMetrics {
    pub total_time_ms: u64,
    pub dns_time_ms: u64,
    pub connect_time_ms: u64,
    pub ttfb_ms: u64,
    pub download_time_ms: u64,
    pub size_bytes: usize,
    pub grade: char,
}

/// Grade a page load time.
fn grade_from_ms(ms: u64) -> char {
    match ms {
        0..=200 => 'A',
        201..=500 => 'B',
        501..=1000 => 'C',
        1001..=2000 => 'D',
        2001..=4000 => 'E',
        _ => 'F',
    }
}

/// Measure total request timing using a simple blocking measurement.
/// In production this would integrate with reqwest timing middleware.
#[must_use]
pub fn measure_speed(total_duration: Duration, size_bytes: usize) -> SpeedMetrics {
    let total_ms = total_duration.as_millis() as u64;
    SpeedMetrics {
        total_time_ms: total_ms,
        dns_time_ms: 0,
        connect_time_ms: 0,
        ttfb_ms: total_ms / 3,
        download_time_ms: total_ms * 2 / 3,
        size_bytes,
        grade: grade_from_ms(total_ms),
    }
}
