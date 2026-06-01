//! Terminal Stream — Streaming automatique des commandes shell vers Telegram.
//!
//! Enrobe BoundSystem + Bot Telegram pour afficher la sortie
//! des commandes en direct dans un message qui se met a jour.

use ansi_converter::ansi_to_telegram;
use anyhow::Result;
use bound_system::{BoundSystem, StreamMessage};
use spinner::Spinner;
use std::collections::HashMap;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup, Recipient};
use tokio::sync::Mutex;

/// Nombre max de lignes affichees dans le message Telegram.
const MAX_VISIBLE_LINES: usize = 30;
/// Limite de caracteres Telegram.
const TG_CHAR_LIMIT: usize = 3800;

/// Identifiant unique d'une execution en cours.
pub type ExecutionId = String;

/// Information sur une execution streamée (pour le bouton stop).
#[derive(Clone)]
pub struct ExecutionInfo {
    pub pid: Option<u32>,
    pub cancel_tx: tokio::sync::mpsc::Sender<()>,
}

/// Gestionnaire de streaming Telegram.
pub struct TerminalStream {
    bot: Bot,
    chat_id: i64,
    bound_system: std::sync::Arc<BoundSystem>,
    /// Exécutions actives (pour les callbacks stop).
    active_executions: Arc<Mutex<HashMap<ExecutionId, ExecutionInfo>>>,
}

impl TerminalStream {
    pub fn new(bot: Bot, chat_id: i64, bound_system: std::sync::Arc<BoundSystem>) -> Self {
        Self {
            bot,
            chat_id,
            bound_system,
            active_executions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Accès au registre d'exécutions actives (pour le handler callback clawd).
    pub fn active_executions(&self) -> Arc<Mutex<HashMap<ExecutionId, ExecutionInfo>>> {
        self.active_executions.clone()
    }

    /// Execute une commande et streame la sortie dans un message Telegram.
    ///
    /// Affiche un spinner animé pendant l'exécution, et permet l'arrêt
    /// via un bouton inline "⏹️ Stop".
    pub async fn execute_and_stream(&self, command: &str, description: &str) -> Result<String> {
        let recipient = Recipient::Id(teloxide::types::ChatId(self.chat_id));
        let exec_id = format!("exec_{}_{}", self.chat_id, uuid_v4_short());

        let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::channel::<()>(1);

        // Message initial avec spinner
        let mut spinner = Spinner::new();

        // Bouton stop
        let stop_button = InlineKeyboardButton::callback("⏹️ Stop", format!("stop:{}", exec_id));
        let keyboard = InlineKeyboardMarkup::new(vec![vec![stop_button]]);

        let initial_text = format!("{} 🖥️ {}...", spinner.next_frame(), description);
        let sent = self
            .bot
            .send_message(recipient.clone(), &initial_text)
            .reply_markup(keyboard.clone())
            .await?;
        let msg_id = sent.id;

        // Lancer le streaming
        let streaming_result = self.bound_system.execute_streaming(command).await;
        let exec = match streaming_result {
            Ok(exec) => exec,
            Err(e) => {
                let err_text = format!("❌ Erreur: {}\n🖥️ {}", e, description);
                let _ = self
                    .bot
                    .edit_message_text(recipient, msg_id, &err_text)
                    .await;
                return Err(e);
            }
        };

        // Enregistrer l'exécution AVEC le PID pour le kill
        {
            let mut execs = self.active_executions.lock().await;
            execs.insert(
                exec_id.clone(),
                ExecutionInfo {
                    pid: exec.pid,
                    cancel_tx,
                },
            );
        }

        let mut rx = exec.rx;
        let mut all_lines: Vec<String> = Vec::new();
        let mut last_text = String::new();
        let spinner_interval = tokio::time::Duration::from_millis(200);
        let mut finished = false;
        let mut final_status: Option<(i32, bool)> = None;
        let mut error_msg: Option<String> = None;

        loop {
            if finished {
                break;
            }

            tokio::select! {
                biased;

                // Arrêt demandé par callback
                _ = cancel_rx.recv() => {
                    finished = true;
                    all_lines.push("⏹️ Arrêté par l'utilisateur.".into());
                    final_status = None;
                }

                msg = rx.recv() => {
                    match msg {
                        Some(StreamMessage::Line(line)) => {
                            let prefix = if line.is_stderr {
                                "⚠️ "
                            } else {
                                ""
                            };
                            let converted = ansi_to_telegram(&line.content);
                            all_lines.push(format!("{}{}", prefix, converted));
                        }
                        Some(StreamMessage::End(end)) => {
                            finished = true;
                            final_status = Some((end.exit_code, end.timed_out));
                        }
                        Some(StreamMessage::Error(e)) => {
                            finished = true;
                            error_msg = Some(e);
                        }
                        None => {
                            finished = true;
                            final_status = None;
                        }
                    }
                }

                _ = tokio::time::sleep(spinner_interval) => {
                    // Mettre à jour le spinner
                }
            }

            // Construire et envoyer la mise à jour
            let current_text = build_display_text(
                description,
                &all_lines,
                final_status.map(|(c, _)| c),
                final_status.map(|(_, t)| t).unwrap_or(false),
                if finished {
                    None
                } else {
                    Some(spinner.next_frame())
                },
            );

            if current_text != last_text {
                let edit_result = if finished {
                    self.bot
                        .edit_message_text(recipient.clone(), msg_id, &current_text)
                        .await
                } else {
                    self.bot
                        .edit_message_text(recipient.clone(), msg_id, &current_text)
                        .reply_markup(keyboard.clone())
                        .await
                };

                if edit_result.is_ok() {
                    last_text = current_text;
                }
            }
        }

        // Nettoyer l'enregistrement
        {
            let mut execs = self.active_executions.lock().await;
            execs.remove(&exec_id);
        }

        // Message final sans bouton
        let final_text = build_display_text(
            description,
            &all_lines,
            final_status.map(|(c, _)| c),
            final_status.map(|(_, t)| t).unwrap_or(false),
            None,
        );
        if final_text != last_text {
            let _ = self
                .bot
                .edit_message_text(recipient, msg_id, &final_text)
                .await;
        }

        match (final_status, error_msg) {
            (Some((code, timed_out)), _) => Ok(format!(
                "Commande terminee (exit={}, timeout={})",
                code, timed_out
            )),
            (None, Some(e)) => Err(anyhow::anyhow!("Stream error: {}", e)),
            (None, None) => Ok("Stream stopped".into()),
        }
    }

    /// Annule une exécution active par son ID (envoie SIGTERM puis cancel).
    pub async fn cancel_execution(&self, exec_id: &str) -> Result<()> {
        let mut execs = self.active_executions.lock().await;
        if let Some(info) = execs.remove(exec_id) {
            // Tuer le processus avec SIGTERM si on a le PID
            if let Some(pid) = info.pid {
                let _ = BoundSystem::kill_process(pid, 15);
                // Planifier SIGKILL après 3 secondes si le processus survit
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    let _ = BoundSystem::kill_process(pid, 9);
                });
            }
            // Annuler le stream
            let _ = info.cancel_tx.try_send(());
        }
        Ok(())
    }
}

/// UUID court pour identifiant d'exécution.
fn uuid_v4_short() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]
    )
}

/// Construit le texte affiche dans le message Telegram.
fn build_display_text(
    description: &str,
    all_lines: &[String],
    exit_code: Option<i32>,
    timed_out: bool,
    spinner_frame: Option<&str>,
) -> String {
    let status_icon = match spinner_frame {
        Some(frame) => frame.to_string(),
        None => match exit_code {
            Some(0) => "✅".to_string(),
            Some(c) => format!("❌ (exit={})", c),
            None => "⏹️".to_string(),
        },
    };

    let mut text = format!("{} 🖥️ {}\n", status_icon, description);

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
        text.push_str("\n⏰ Timeout — commande trop longue");
    }

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
        let text = build_display_text("Test cmd", &lines, Some(0), false, None);
        assert!(text.contains("Test cmd"));
        assert!(text.contains("(aucune sortie)"));
    }

    #[test]
    fn test_build_display_text_success() {
        let lines: Vec<String> = vec!["hello".into(), "world".into()];
        let text = build_display_text("echo", &lines, Some(0), false, None);
        assert!(text.contains("hello"));
        assert!(text.contains("world"));
        assert!(text.contains("✅"));
    }

    #[test]
    fn test_build_display_text_failure() {
        let lines: Vec<String> = vec!["error!".into()];
        let text = build_display_text("fail", &lines, Some(1), false, None);
        assert!(text.contains("❌"));
        assert!(text.contains("exit=1"));
    }

    #[test]
    fn test_build_display_text_timeout() {
        let lines: Vec<String> = vec!["slow...".into()];
        let text = build_display_text("slow", &lines, Some(-1), true, None);
        assert!(text.contains("Timeout"));
    }

    #[test]
    fn test_build_display_text_spinner() {
        let lines: Vec<String> = vec!["line1".into()];
        let text = build_display_text("cmd", &lines, None, false, Some("⠋"));
        assert!(text.contains("⠋"));
        assert!(!text.contains("✅"));
    }

    #[test]
    fn test_build_display_text_truncates_many_lines() {
        let lines: Vec<String> = (0..50).map(|i| format!("line_{}", i)).collect();
        let text = build_display_text("many", &lines, Some(0), false, None);
        assert!(text.contains("tronquees"));
        assert!(text.contains("line_49"));
        assert!(!text.contains("line_0"));
    }
}
