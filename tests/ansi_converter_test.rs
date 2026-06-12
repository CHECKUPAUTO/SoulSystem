//! Tests d'intégration pour le convertisseur ANSI → Telegram.

use soulsystem::ansi_converter::{ansi_to_telegram, strip_ansi};

#[test]
fn test_ansi_integration_full_output() {
    // Simuler une sortie de commande réaliste
    let input = "\x1b[32mOK\x1b[0m Fichier traité\n\x1b[31mERREUR\x1b[0m Fichier manquant\n\x1b[33mWARN\x1b[0m Dépassement mémoire";
    let result = ansi_to_telegram(input);

    assert!(result.contains("🟢"), "Should contain green emoji");
    assert!(result.contains("🔴"), "Should contain red emoji");
    assert!(result.contains("🟡"), "Should contain yellow emoji");
    assert!(result.contains("Fichier traité"));
    assert!(result.contains("Fichier manquant"));
    assert!(result.contains("Dépassement mémoire"));
}

#[test]
fn test_strip_ansi_integration_preserves_newlines() {
    let input = "Line 1\n\x1b[31mLine 2\x1b[0m\nLine 3";
    let result = strip_ansi(input);
    assert_eq!(result, "Line 1\nLine 2\nLine 3");
}

#[test]
fn test_ansi_to_telegram_integration_no_ansi_no_change() {
    let input = "Just plain text without any escape codes";
    let result = ansi_to_telegram(input);
    assert_eq!(result, input);
}

#[test]
fn test_ansi_integration_bold_multiline() {
    let input = "\x1b[1mTitle\x1b[0m\nNormal text\n\x1b[1mBold again\x1b[0m";
    let result = ansi_to_telegram(input);
    assert_eq!(result, "<b>Title</b>\nNormal text\n<b>Bold again</b>");
}
