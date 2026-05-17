//! Terminal Stream — Streaming automatique des commandes shell vers Telegram.
//!
//! Enrobe BoundSystem + Bot Telegram pour afficher la sortie
//! des commandes en direct dans un message qui se met a jour.

use crate::bound_system::{BoundSystem, StreamMessage};
use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::Recipient;

/// Nombre max de lignes affichees dans le message Telegram.
const MAX_VISIBLE_LINES: usize = 30;
/// Limite de caracteres Telegram.
const TG_CHAR_LIMIT: usize = 3800;

/// Gestionnaire de streaming Telegram.
pub struct TerminalStream {
    bot: Bot,
    chat_id: i64,
    bound_system: std::sync::Arc<BoundSystem>,
}

impl TerminalStream {
    pub fn new(
        bot: Bot,
        chat_id: i64,
        bound_system: std::sync::Arc<BoundSystem>,
    ) -> Self {
        Self {
            bot,
            chat_id,
            bound_system,
        }
    }

    /// Execute une commande et streame la sortie dans un message Telegram.
    ///
    /// Envoie un message initial "en cours...", puis le met a jour
    /// ligne par ligne avec la sortie de la commande.
    pub async fn execute_and_stream(
        &self,
        command: &str,
        description: &str,
    ) -> Result<String> {
        let recipient = Recipient::Id(teloxide::types::ChatId(self.chat_id));

        // Message initial
        let initial_text = format!("\u{1f5a5}\u{fe0f} {}...\n\u{23f3} Execution en cours...", description);
        let sent = self
            .bot
            .send_message(recipient.clone(), &initial_text)
            .await?;
        let msg_id = sent.id;

        // Lancer le streaming
        let mut rx = self.bound_system.execute_streaming(command).await?;

        let mut all_lines: Vec<String> = Vec::new();
        let mut last_update = std::time::Instant::now();
        let update_interval = std::time::Duration::from_millis(300);
        let mut last_text = initial_text;

        loop {
            match rx.recv().await {
                Some(StreamMessage::Line(line)) => {
                    let prefix = if line.is_stderr {
                        "\u{26a0}\u{fe0f} "
                    } else {
                        ""
                    };
                    all_lines.push(format!("{}{}", prefix, line.content));
                }
                Some(StreamMessage::End(end)) => {
                    let final_text = build_display_text(
                        description,
                        &all_lines,
                        Some(end.exit_code),
                        end.timed_out,
                    );
                    if final_text != last_text {
                        let _ = self
                            .bot
                            .edit_message_text(recipient, msg_id, &final_text)
                            .await;
                    }
                    return Ok(format!(
                        "Commande terminee (exit={}, timeout={})",
                        end.exit_code, end.timed_out
                    ));
                }
                Some(StreamMessage::Error(e)) => {
                    let error_text = format!(
                        "\u{1f5a5}\u{fe0f} {}...\n\u{274c} Erreur: {}",
                        description, e
                    );
                    let _ = self
                        .bot
                        .edit_message_text(recipient, msg_id, &error_text)
                        .await;
                    return Err(anyhow::anyhow!("Stream error: {}", e));
                }
                None => {
                    // Channel closed without End message
                    let final_text = build_display_text(description, &all_lines, None, false);
                    if final_text != last_text {
                        let _ = self
                            .bot
                            .edit_message_text(recipient, msg_id, &final_text)
                            .await;
                    }
                    return Ok("Stream closed".into());
                }
            }

            // Throttle les mises a jour Telegram
            let now = std::time::Instant::now();
            if now.duration_since(last_update) >= update_interval {
                let current_text =
                    build_display_text(description, &all_lines, None, false);
                if current_text != last_text {
                    let _ = self
                        .bot
                        .edit_message_text(recipient.clone(), msg_id, &current_text)
                        .await;
                    last_text = current_text;
                }
                last_update = now;
            }
        }
    }
}

/// Construit le texte affiche dans le message Telegram.
fn build_display_text(
    description: &str,
    all_lines: &[String],
    exit_code: Option<i32>,
    timed_out: bool,
) -> String {
    let status_icon = match exit_code {
        Some(0) => "\u{2705}".to_string(),
        Some(c) => format!("\u{274c} (exit={})", c),
        None => "\u{23f3}".to_string(),
    };

    let mut text = format!("\u{1f5a5}\u{fe0f} {}... {}\n", description, status_icon);

    if all_lines.is_empty() {
        text.push_str("(aucune sortie)\n");
    } else {
        let visible = if all_lines.len() > MAX_VISIBLE_LINES {
            text.push_str(&format!(
                "... ({} lignes tronquees sur {})\n",
                all_lines.len() - MAX_VISIBLE_LINES,
                all_lines.len()
            ));
            &all_lines[all_lines.len() - MAX_VISIBLE_LINES..]
        } else {
            all_lines
        };

        for line in visible {
            text.push_str(line);
            text.push('\n');
        }
    }

    if timed_out {
        text.push_str("\n\u{23f0} Timeout — commande trop longue");
    }

    // Tronquer si depasse la limite Telegram
    if text.len() > TG_CHAR_LIMIT {
        text.truncate(TG_CHAR_LIMIT - 50);
        text.push_str("\n... (sortie tronquee, limite Telegram)");
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_display_text_empty() {
        let lines: Vec<String> = vec![];
        let text = build_display_text("Test cmd", &lines, Some(0), false);
        assert!(text.contains("Test cmd"));
        assert!(text.contains("(aucune sortie)"));
    }

    #[test]
    fn test_build_display_text_success() {
        let lines: Vec<String> = vec!["hello".into(), "world".into()];
        let text = build_display_text("echo", &lines, Some(0), false);
        assert!(text.contains("hello"));
        assert!(text.contains("world"));
        assert!(text.contains("\u{2705}"));
    }

    #[test]
    fn test_build_display_text_failure() {
        let lines: Vec<String> = vec!["error!".into()];
        let text = build_display_text("fail", &lines, Some(1), false);
        assert!(text.contains("\u{274c}"));
        assert!(text.contains("exit=1"));
    }

    #[test]
    fn test_build_display_text_timeout() {
        let lines: Vec<String> = vec!["slow...".into()];
        let text = build_display_text("slow", &lines, Some(-1), true);
        assert!(text.contains("Timeout"));
    }

    #[test]
    fn test_build_display_text_progress() {
        let lines: Vec<String> = vec!["line1".into()];
        let text = build_display_text("cmd", &lines, None, false);
        assert!(text.contains("\u{23f3}"));
    }

    #[test]
    fn test_build_display_text_truncates_many_lines() {
        let lines: Vec<String> = (0..50).map(|i| format!("line_{}", i)).collect();
        let text = build_display_text("many", &lines, Some(0), false);
        assert!(text.contains("tronquees"));
        assert!(text.contains("line_49"));
        assert!(!text.contains("line_0"));
    }
}
