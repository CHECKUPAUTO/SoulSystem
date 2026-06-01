//! ANSI color palette — Lobster theme matching OpenClaw gateway.

/// Lobster-inspired ANSI true-color palette.
pub struct Theme;
impl Theme {
    pub const ACCENT: &str = "\x1b[38;2;255;90;45m";
    pub const ACCENT_BRIGHT: &str = "\x1b[38;2;255;122;61m";
    pub const ACCENT_DIM: &str = "\x1b[38;2;209;74;34m";
    pub const INFO: &str = "\x1b[38;2;255;138;91m";
    pub const SUCCESS: &str = "\x1b[38;2;47;191;113m";
    pub const WARN: &str = "\x1b[38;2;255;176;32m";
    pub const ERROR: &str = "\x1b[38;2;226;61;45m";
    pub const MUTED: &str = "\x1b[38;2;139;127;119m";
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
}

/// Returns `true` if stdout is a TTY and `NO_COLOR` is not set.
pub fn is_rich_tty() -> bool {
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// Helper: wrap text in styling, respecting non-TTY.
macro_rules! style {
    ($prefix:expr, $text:expr) => {{
        if crate::cli::theme::is_rich_tty() {
            format!("{}{}{}", $prefix, $text, crate::cli::theme::Theme::RESET)
        } else {
            $text.to_string()
        }
    }};
}

pub fn style_header(text: &str) -> String {
    style!(Theme::ACCENT, text)
}
pub fn style_info(text: &str) -> String {
    style!(Theme::INFO, text)
}
pub fn style_success(text: &str) -> String {
    style!(Theme::SUCCESS, text)
}
pub fn style_warn(text: &str) -> String {
    style!(Theme::WARN, text)
}
pub fn style_error(text: &str) -> String {
    style!(Theme::ERROR, text)
}
pub fn style_muted(text: &str) -> String {
    style!(Theme::MUTED, text)
}
pub fn style_bold(text: &str) -> String {
    style!(Theme::BOLD, text)
}
