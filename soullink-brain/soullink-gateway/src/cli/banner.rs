//! ASCII art banner + version/tagline display.
use crate::cli::theme::{is_rich_tty, style_bold, style_header, style_info, style_muted, Theme};

static BANNER_EMITTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Lobster ASCII art (50 columns wide), adapted from SoulSystem banner.ts.
pub const LOBSTER_ASCII: &[&str] = &[
    "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄",
    "██░▄▄▄░██░▄▄░██░▄▄▄██░▀██░██░▄▄▀██░████░▄▄▀██░███░██",
    "██░███░██░▀▀░██░▄▄▄██░█░█░██░█████░████░▀▀░██░█░█░██",
    "██░▀▀▀░██░█████░▀▀▀██░██▄░██░▀▀▄██░▀▀░█░██░██▄▀▄▀▄██",
    "▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀",
    "           🦞 SOULLINK GATEWAY RS 🦞           ",
    " ",
];

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMMIT_HASH: &str = match option_env!("GIT_HASH") {
    Some(h) => h,
    None => "unknown",
};

const TAGLINES: &[&str] = &[
    "Your terminal just grew claws — let the bot pinch the busywork.",
    "Gateway RS: More integrations than your therapist's intake form.",
    "Built by lobsters, for humans. Don't question the hierarchy.",
    "RS suffix added because everything is better with Rust.",
    "Message routing with exoskeletal precision.",
    "Your messages are in good claws — we've got a shell for that.",
    "SoulLink RS: bridging Telegram today, world domination tomorrow.",
    "The gateway that goes snap when things go wrong.",
    "Now with 100% more Rust, 0% more Node_modules.",
    "RS: Because one gateway wasn't enough.",
    "Rolling updates, no shedding — just crustacean determination.",
    "From shell to shell, delivering messages with vigor.",
    "We put the 'gate' in gateway and the 'RS' in Rust.",
    "Switching to RS — fewer lobsters, more claws.",
];

/// Pick a tagline that rotates daily.
pub fn pick_tagline() -> &'static str {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0);
    let idx = (seed as usize) % TAGLINES.len();
    TAGLINES[idx]
}

/// Format the banner line: version + commit + tagline.
pub fn format_banner_line() -> String {
    let tagline = pick_tagline();
    let columns = terminal_width();
    let plain = format!(
        "SoulLink Gateway RS {} ({}) — {}",
        VERSION, COMMIT_HASH, tagline
    );
    let _one_line = format!("{} {} ({})", VERSION, COMMIT_HASH, tagline);

    if is_rich_tty() {
        let version_info = format!(
            "{}{} {}{} ({}){}",
            Theme::BOLD,
            style_header("SoulLink Gateway RS"),
            style_info(VERSION),
            Theme::RESET,
            style_muted(COMMIT_HASH),
            Theme::RESET,
        );
        if columns >= 50 {
            let tagline_colored = style_muted(&format!("— {}", tagline));
            format!("{} {}", version_info, tagline_colored)
        } else {
            format!(
                "{}\n{}",
                version_info,
                style_muted(&format!("  {}", tagline))
            )
        }
    } else if columns >= plain.len().max(50) {
        plain
    } else {
        let base = format!("SoulLink Gateway RS {} ({})", VERSION, COMMIT_HASH);
        format!("{}\n  {}", base, tagline)
    }
}

/// Emit the full ASCII banner once per process. No-op on non-TTY or when `--json` is passed.
pub fn emit_banner() {
    if BANNER_EMITTED.set(true).is_err() {
        return;
    }
    if !is_rich_tty() {
        // Non-TTY: just log version
        eprintln!("SoulLink Gateway RS {} ({})", VERSION, COMMIT_HASH);
        return;
    }
    let rich = is_rich_tty();
    let columns = terminal_width();

    // Skip full ASCII art if terminal is too narrow
    if columns >= 50 {
        for line in LOBSTER_ASCII {
            if rich {
                // Apply lobster-color gradient to block chars
                if line.starts_with('▄') || line.starts_with('▀') {
                    println!("{}", style_muted(line));
                } else if line.starts_with("██") {
                    println!("{}", style_header(line));
                } else if line.contains("SOULLINK") {
                    println!("{}", style_bold(line));
                } else {
                    println!("{}{}{}", Theme::ACCENT_DIM, line, Theme::RESET);
                }
            } else {
                println!("{}", line);
            }
        }
    } else {
        // Narrow terminal: just print a compact header
        println!("{}", style_header("🦞 SoulLink Gateway RS 🦞"));
    }
    println!("{}", format_banner_line());
    println!();
}

fn terminal_width() -> usize {
    // Try COLUMNS env var first (set by many terminals/shells)
    if let Ok(cols) = std::env::var("COLUMNS") {
        if let Ok(n) = cols.parse::<usize>() {
            if n > 0 {
                return n;
            }
        }
    }
    // Fallback: `stty size` on Unix
    #[cfg(unix)]
    {
        if let Ok(out) = std::process::Command::new("stty")
            .args(["size"])
            .stdin(std::process::Stdio::null())
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(cols) = s.split_whitespace().nth(1) {
                if let Ok(n) = cols.trim().parse::<usize>() {
                    if n > 0 {
                        return n;
                    }
                }
            }
        }
    }
    120
}
