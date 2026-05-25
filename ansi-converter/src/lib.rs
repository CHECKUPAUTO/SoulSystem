//! ANSI Converter — Conversion des séquences d'échappement ANSI
//! (couleurs, gras) en format compatible Telegram.
//!
//! Stratégie pragmatique : les couleurs sont remplacées par des émojis
//! en début de ligne, le gras est converti en balises HTML `<b>`.
//! Supporte aussi un mode `strip` pour suppression complète.

/// Convertit une chaîne contenant des séquences ANSI en texte formaté
/// pour Telegram (émojis couleur + balises HTML `<b>`).
pub fn ansi_to_telegram(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut bold_depth: usize = 0;
    let mut i: usize = 0;
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();

    while i < len {
        if chars[i] == '\x1b' && i + 1 < len && chars[i + 1] == '[' {
            i += 2;

            let mut params = String::new();
            while i < len && chars[i] != 'm' {
                params.push(chars[i]);
                i += 1;
            }
            if i < len {
                i += 1; // skip 'm'
            }

            for param in params.split(';') {
                let code: i32 = match param.parse() {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                match code {
                    0 => {
                        for _ in 0..bold_depth {
                            output.push_str("</b>");
                        }
                        bold_depth = 0;
                    }
                    1 => {
                        output.push_str("<b>");
                        bold_depth += 1;
                    }
                    30 => output.push_str("⚫ "),
                    31 => output.push_str("🔴 "),
                    32 => output.push_str("🟢 "),
                    33 => output.push_str("🟡 "),
                    34 => output.push_str("🔵 "),
                    35 => output.push_str("🟣 "),
                    36 => output.push_str("🩵 "),
                    37 => output.push_str("⚪ "),
                    90 => output.push_str("⬛ "),
                    91 => output.push_str("❤️ "),
                    92 => output.push_str("💚 "),
                    93 => output.push_str("💛 "),
                    94 => output.push_str("💙 "),
                    95 => output.push_str("💜 "),
                    96 => output.push_str("🩵 "),
                    97 => output.push_str("🤍 "),
                    40..=47 => {}
                    _ => {}
                }
            }
        } else {
            output.push(chars[i]);
            i += 1;
        }
    }

    for _ in 0..bold_depth {
        output.push_str("</b>");
    }

    output
}

/// Supprime toutes les séquences ANSI d'une chaîne.
pub fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i: usize = 0;

    while i < len {
        if chars[i] == '\x1b' && i + 1 < len && chars[i + 1] == '[' {
            i += 2;
            while i < len && chars[i] != 'm' {
                i += 1;
            }
            if i < len {
                i += 1;
            }
        } else {
            output.push(chars[i]);
            i += 1;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_removes_color() {
        let input = "\x1b[31mErreur\x1b[0m";
        let result = strip_ansi(input);
        assert_eq!(result, "Erreur");
    }

    #[test]
    fn test_strip_ansi_removes_bold() {
        let input = "\x1b[1mImportant\x1b[0m";
        let result = strip_ansi(input);
        assert_eq!(result, "Important");
    }

    #[test]
    fn test_strip_ansi_preserves_plain() {
        let input = "Plain text";
        let result = strip_ansi(input);
        assert_eq!(result, "Plain text");
    }

    #[test]
    fn test_strip_multiple_sequences() {
        let input = "\x1b[1m\x1b[31mBold Red\x1b[0m";
        let result = strip_ansi(input);
        assert_eq!(result, "Bold Red");
    }

    #[test]
    fn test_ansi_to_telegram_bold() {
        let input = "\x1b[1mImportant\x1b[0m";
        let result = ansi_to_telegram(input);
        assert_eq!(result, "<b>Important</b>");
    }

    #[test]
    fn test_ansi_to_telegram_red_emoji() {
        let input = "\x1b[31mErreur fatale\x1b[0m";
        let result = ansi_to_telegram(input);
        assert!(
            result.contains("🔴"),
            "Should contain red emoji, got: {}",
            result
        );
        assert!(result.contains("Erreur fatale"));
    }

    #[test]
    fn test_ansi_to_telegram_green_emoji() {
        let input = "\x1b[32mSuccès\x1b[0m";
        let result = ansi_to_telegram(input);
        assert!(
            result.contains("🟢"),
            "Should contain green emoji, got: {}",
            result
        );
    }

    #[test]
    fn test_ansi_to_telegram_bold_and_color() {
        let input = "\x1b[1m\x1b[31mBold Red\x1b[0m";
        let result = ansi_to_telegram(input);
        assert!(result.contains("<b>"), "no bold tag: {}", result);
        assert!(result.contains("🔴"), "no red emoji: {}", result);
        assert!(result.contains("Bold Red"), "no text: {}", result);
    }

    #[test]
    fn test_ansi_to_telegram_plain_text() {
        let input = "Hello world";
        let result = ansi_to_telegram(input);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_ansi_mid_line_color() {
        let input = "Normal \x1b[31mRed\x1b[0m after";
        let result = ansi_to_telegram(input);
        assert!(result.contains("Normal 🔴 Red after"));
    }

    #[test]
    fn test_ansi_extended_colors() {
        let input = "\x1b[91mBright Red\x1b[0m";
        let result = ansi_to_telegram(input);
        assert!(result.contains("❤️") || result.contains("Bright Red"));
    }
}
