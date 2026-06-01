//! Shared CLI utilities — port parsing, helpers.
//!
//! Non-Run subcommands and common formatting functions.

use std::time::Duration;

/// Format a duration for human display.
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 1.0 {
        format!("{:.0}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.1}s", secs)
    } else {
        let mins = (secs / 60.0).floor() as u64;
        let s = secs % 60.0;
        format!("{mins}m {s:.0}s")
    }
}

/// Parse `--bind` value into a meaningful string for display.
pub fn bind_display(bind: &str) -> &str {
    match bind {
        "loopback" => "127.0.0.1",
        "lan" => "0.0.0.0",
        _ => bind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_ms() {
        assert_eq!(format_duration(Duration::from_millis(500)), "500ms");
    }

    #[test]
    fn format_duration_secs() {
        assert_eq!(format_duration(Duration::from_secs(30)), "30.0s");
    }

    #[test]
    fn format_duration_mins() {
        let s = format_duration(Duration::from_secs(125));
        assert!(s.contains("2m"));
        assert!(s.contains("5s"));
    }

    #[test]
    fn bind_display_known() {
        assert_eq!(bind_display("loopback"), "127.0.0.1");
        assert_eq!(bind_display("lan"), "0.0.0.0");
    }

    #[test]
    fn bind_display_passthrough() {
        assert_eq!(bind_display("0.0.0.0"), "0.0.0.0");
        assert_eq!(bind_display("custom"), "custom");
    }
}
