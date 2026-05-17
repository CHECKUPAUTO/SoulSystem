//! Clawd — Bot Telegram integrant tous les modules SoulSystem.
//!
//! Commandes:
//!   /veille <sujets>  — Demander a AVID de surveiller des sujets
//!   /skill <nom> <args> — Executer un skill local
//!   /run <commande>   — Executer une commande systeme (whitelist)
//!   /terminal         — Ouvrir un terminal PTY persistant
//!   /exit             — Fermer le terminal PTY
//!
//! Detection automatique: blocs shell et lignes prefixees `$` dans les
//! reponses du LLM declenchent le streaming automatique vers Telegram.

use crate::ansi_converter::ansi_to_telegram;
// use crate::audit_log::AuditLog;
use crate::bound_system::BoundSystem;
use crate::local_skills::BuiltinSkills;
use crate::model_router::ModelRouter;
use crate::soul_memory::SoulMemory;
use crate::terminal_stream::{ExecutionId, ExecutionInfo, TerminalStream};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
// use teloxide::dispatching::dialogue::InMemStorage;
// use teloxide::dispatching::UpdateHandler;
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide::types::{ParseMode, Recipient};
use tokio::sync::Mutex;

// ── Feedback ────────────────────────────────────────────────────────────

/// Entree de feedback utilisateur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackEntry {
    pub timestamp: i64,
    pub user_id: String,
    pub query: String,
    pub response: String,
    pub score: i32, // +1 = like, -1 = dislike
}

/// Stockage des feedbacks (sled).
pub struct FeedbackStore {
    db: sled::Db,
}

impl FeedbackStore {
    pub fn open(path: &str) -> Result<Self> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }

    pub fn new_test() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
        Ok(Self { db })
    }

    /// Enregistre un feedback.
    pub fn record(&self, user_id: &str, query: &str, response: &str, score: i32) -> Result<()> {
        let entry = FeedbackEntry {
            timestamp: chrono::Utc::now().timestamp(),
            user_id: user_id.to_string(),
            query: query.to_string(),
            response: response.to_string(),
            score,
        };

        let key = format!("fb_{}_{}", entry.timestamp, uuid_v4());
        self.db
            .insert(key.as_bytes(), serde_json::to_vec(&entry)?)?;
        Ok(())
    }

    /// Recupere les derniers feedbacks.
    pub fn get_recent(&self, limit: usize) -> Result<Vec<FeedbackEntry>> {
        let mut entries: Vec<FeedbackEntry> = Vec::new();
        for item in self.db.iter().rev() {
            let (_, value) = item?;
            if let Ok(entry) = serde_json::from_slice(&value) {
                entries.push(entry);
            }
            if entries.len() >= limit {
                break;
            }
        }
        Ok(entries)
    }
}

/// UUID v4 simplifie.
fn uuid_v4() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

// ── AVID Watch ──────────────────────────────────────────────────────────

/// Sujet de veille AVID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchTopic {
    pub topic: String,
    pub added_by: String,
    pub added_at: i64,
    pub last_check: Option<i64>,
    pub results_count: usize,
}

/// Gestionnaire de veille AVID.
pub struct AvidWatcher {
    topics: Vec<WatchTopic>,
    avid_endpoint: String,
}

impl AvidWatcher {
    pub fn new(avid_endpoint: String) -> Self {
        Self {
            topics: Vec::new(),
            avid_endpoint,
        }
    }

    /// Ajoute un sujet de veille.
    pub fn add_topic(&mut self, topic: &str, user: &str) {
        if self.topics.iter().any(|t| t.topic == topic) {
            return;
        }
        self.topics.push(WatchTopic {
            topic: topic.to_string(),
            added_by: user.to_string(),
            added_at: chrono::Utc::now().timestamp(),
            last_check: None,
            results_count: 0,
        });
    }

    /// Liste les sujets.
    pub fn list_topics(&self) -> &[WatchTopic] {
        &self.topics
    }

    /// Lance une recherche pour un sujet (mockee si AVID non dispo).
    pub async fn research(&self, topic: &str) -> Result<Vec<String>> {
        let client = reqwest::Client::new();
        let url = format!("{}/research", self.avid_endpoint);

        match client
            .post(&url)
            .json(&serde_json::json!({"query": topic, "max_results": 5}))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(resp) => {
                let json: serde_json::Value = resp.json().await?;
                Ok(json["results"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default())
            }
            Err(_) => {
                // Retour mocke (AVID n'est pas toujours disponible)
                Ok(vec![format!(
                    "[MOCK AVID] Resultat de recherche pour '{}' ({} resultats simules)",
                    topic,
                    rand::random::<u8>() % 5 + 1
                )])
            }
        }
    }
}

// ── ClawdContext & PTY ──────────────────────────────────────────────────

/// Session PTY active pour un chat.
pub(crate) struct PtySession {
    pub(crate) input_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub(crate) reader_handle: tokio::task::AbortHandle,
    pub(crate) last_msg_id: Option<teloxide::types::MessageId>,
    pub(crate) last_activity: std::time::Instant,
    #[allow(dead_code)]
    pub(crate) session_name: Option<String>, // nom de session tmux
}

/// Contexte partage de tout le bot.
pub struct ClawdContext {
    pub config: Settings,
    pub bound_system: Arc<BoundSystem>,
    pub memory: Arc<SoulMemory>,
    pub model_router: ModelRouter,
    pub avid: Arc<Mutex<AvidWatcher>>,
    pub skills: Arc<BuiltinSkills>,
    pub feedback: Arc<FeedbackStore>,
    pub(crate) pty_sessions: Arc<Mutex<HashMap<i64, PtySession>>>,
    /// Registre partage avec TerminalStream pour les callbacks stop.
    pub active_executions: Arc<Mutex<HashMap<ExecutionId, ExecutionInfo>>>,
    /// Bus interne pour la plateforme unifiee Telegram.
    pub bus: Arc<crate::bus::Bus>,
}

/// Configuration minimale de Clawd.
#[derive(Clone)]
pub struct Settings {
    pub bot_token: String,
    pub avid_endpoint: String,
}

impl ClawdContext {
    pub fn new(config: Settings, bus: Arc<crate::bus::Bus>) -> Result<Self> {
        let skills_dir = std::path::PathBuf::from("/var/lib/soulsystem/skills");
        let _ = std::fs::create_dir_all(&skills_dir);

        let skills = Arc::new(BuiltinSkills::new());

        let feedback = Arc::new(FeedbackStore::open("/var/lib/soulsystem/data/feedback.db")?);

        let avid_endpoint = config.avid_endpoint.clone();
        let avid = Arc::new(Mutex::new(AvidWatcher::new(avid_endpoint)));

        Ok(Self {
            bound_system: Arc::new(BoundSystem::new(BoundSystem::default_whitelist())),
            memory: Arc::new(SoulMemory::new()?),
            model_router: ModelRouter::default_models(),
            skills,
            feedback,
            avid,
            config,
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
            active_executions: Arc::new(Mutex::new(HashMap::new())),
            bus,
        })
    }

    /// Appel Ollama (ou simulation si pas de serveur).
    pub async fn call_ollama(model: &str, prompt: &str) -> String {
        let client = reqwest::Client::new();
        match client
            .post("http://localhost:11434/api/generate")
            .json(&serde_json::json!({
                "model": model,
                "prompt": prompt,
                "stream": false
            }))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
        {
            Ok(resp) => match resp.json::<serde_json::Value>().await {
                Ok(json) => json["response"]
                    .as_str()
                    .unwrap_or("[Ollama] Reponse vide")
                    .to_string(),
                Err(_) => format!("[Ollama mock] Reponse simulee pour: {}", prompt),
            },
            Err(_) => format!("[Ollama mock] Reponse simulee pour: {}", prompt),
        }
    }

    /// Extrait les commandes shell d'une reponse LLM.
    pub fn extract_shell_commands(response: &str) -> Vec<String> {
        let mut commands = Vec::new();

        // Pattern 1: blocs \`\`\`shell
        let re_block = regex::Regex::new(r"(?s)```(?:sh|shell|bash)?\n(.*?)```").unwrap();
        for cap in re_block.captures_iter(response) {
            if let Some(content) = cap.get(1) {
                for line in content.as_str().lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        commands.push(trimmed.to_string());
                    }
                }
            }
        }

        // Pattern 2: lignes prefixees $ ou >
        let re_line = regex::Regex::new(r"^[$>]\s+(.+)$").unwrap();
        for line in response.lines() {
            if let Some(cap) = re_line.captures(line) {
                if let Some(cmd) = cap.get(1) {
                    let trimmed = cmd.as_str().trim();
                    if !trimmed.is_empty() {
                        commands.push(trimmed.to_string());
                    }
                }
            }
        }

        commands
    }
}

// ── Bot Runner ──────────────────────────────────────────────────────────

/// Demarre le bot Telegram avec tous les handlers.
pub async fn run_bot(context: Arc<ClawdContext>) -> Result<()> {
    let bot = Bot::new(&context.config.bot_token);

    tracing::info!("Clawd Telegram bot starting...");
    let _ = bot.get_me().await.map(|me| {
        tracing::info!("Bot connecte: @{}", me.username());
    });

    // Canal pour les sorties PTY
    let (pty_tx, mut pty_rx) = tokio::sync::mpsc::unbounded_channel::<(i64, String)>();

    // Tache de fond: watchdog PTY (timeout 30 min d'inactivite)
    let ctx_watchdog = context.clone();
    let pty_tx_watchdog = pty_tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            let mut sessions = ctx_watchdog.pty_sessions.lock().await;
            let timeout = std::time::Duration::from_secs(30 * 60);
            let now = std::time::Instant::now();
            let stale: Vec<i64> = sessions
                .iter()
                .filter(|(_, s)| now.duration_since(s.last_activity) > timeout)
                .map(|(k, _)| *k)
                .collect();
            for chat_id in stale {
                if let Some(session) = sessions.remove(&chat_id) {
                    session.reader_handle.abort();
                    let _ = pty_tx_watchdog
                        .send((chat_id, "\u{23f0} Terminal ferme (inactif 30 min)".into()));
                    tracing::info!("PTY session timeout for chat {}", chat_id);
                }
            }
        }
    });

    // PTY Output Forwarder: convertit ANSI et forward au chat
    let bot_pty = bot.clone();
    let ctx_pty = context.clone();
    tokio::spawn(async move {
        let bot = bot_pty;
        loop {
            match pty_rx.recv().await {
                Some((chat_id, output)) => {
                    let recipient = Recipient::Id(teloxide::types::ChatId(chat_id));
                    // Convertir les séquences ANSI
                    let converted = ansi_to_telegram(&output);

                    let mut sessions = ctx_pty.pty_sessions.lock().await;
                    let updated = if let Some(session) = sessions.get_mut(&chat_id) {
                        session.last_activity = std::time::Instant::now();
                        if let Some(msg_id) = session.last_msg_id {
                            let truncated = if converted.len() > 3800 {
                                format!(
                                    "... (sortie tronquee)\n{}",
                                    &converted[converted.len().saturating_sub(3800)..]
                                )
                            } else {
                                converted.clone()
                            };

                            let edit_result = bot
                                .edit_message_text(recipient.clone(), msg_id, &truncated)
                                .await;
                            match edit_result {
                                Ok(m) => {
                                    let new_id = m.id;
                                    if new_id != msg_id {
                                        session.last_msg_id = Some(new_id);
                                    }
                                    true
                                }
                                Err(_) => false,
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    drop(sessions);

                    if !updated {
                        let truncated = if converted.len() > 3800 {
                            format!(
                                "... (sortie tronquee)\n{}",
                                &converted[converted.len().saturating_sub(3800)..]
                            )
                        } else {
                            converted
                        };
                        match bot
                            .send_message(recipient.clone(), &truncated)
                            .parse_mode(ParseMode::MarkdownV2)
                            .await
                        {
                            Ok(sent) => {
                                let mut sessions = ctx_pty.pty_sessions.lock().await;
                                if let Some(session) = sessions.get_mut(&chat_id) {
                                    session.last_msg_id = Some(sent.id);
                                }
                            }
                            Err(_) => {
                                if let Ok(sent) =
                                    bot.send_message(recipient.clone(), &truncated).await
                                {
                                    let mut sessions = ctx_pty.pty_sessions.lock().await;
                                    if let Some(session) = sessions.get_mut(&chat_id) {
                                        session.last_msg_id = Some(sent.id);
                                    }
                                }
                            }
                        }
                    }
                }
                None => {
                    tracing::info!("PTY output channel closed");
                    break;
                }
            }
        }
    });

    // ── Bus Forwarder: envoie les reponses bus vers Telegram ──────────────
    let bot_bus = bot.clone();
    let ctx_bus = context.clone();
    tokio::spawn(async move {
        let mut rx = ctx_bus.bus.subscribe();
        loop {
            match rx.recv().await {
                Ok(crate::bus::Message::Custom { topic, payload }) if topic == "telegram.reply" => {
                    if let Some(chat_id) = payload.get("chat_id").and_then(|v| v.as_i64()) {
                        let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        let _reply_to = payload.get("reply_to_message_id").and_then(|v| v.as_i64());
                        let recipient = Recipient::Id(teloxide::types::ChatId(chat_id));
                        let _ = bot_bus.send_message(recipient, text).await;
                    }
                }
                Ok(_) => continue,
                Err(_) => {
                    tracing::warn!("Bus forwarder: deconnecte du bus");
                    break;
                }
            }
        }
    });

    // ── Message & Callback handlers ──────────────────────────────────────
    let ctx_msg = context.clone();
    let pty_tx_msg = pty_tx.clone();
    let handler = dptree::entry()
        .branch(Update::filter_message().endpoint(handle_message))
        .branch(Update::filter_callback_query().endpoint(handle_callback));

    teloxide::dispatching::Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![ctx_msg, pty_tx_msg])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}

/// Handler pour les messages texte (PTY input + chat LLM).
async fn handle_message(
    msg: teloxide::types::Message,
    bot: Bot,
    ctx: Arc<ClawdContext>,
    pty_tx: tokio::sync::mpsc::UnboundedSender<(i64, String)>,
) -> Result<()> {
    let chat_id = msg.chat.id.0;
    let text = msg.text().unwrap_or("");
    let user_id = msg
        .from
        .as_ref()
        .map(|u| u.id.0.to_string())
        .unwrap_or_else(|| "unknown".into());
    let recipient = Recipient::Id(teloxide::types::ChatId(chat_id));

    // Handle slash commands (outside PTY mode)
    if let Some(cmd) = text.strip_prefix('/') {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let cmd_name = parts[0].to_lowercase();
        let args = parts.get(1).unwrap_or(&"");

        match cmd_name.as_str() {
            "terminal" => {
                let _ = bot
                    .send_message(
                        recipient.clone(),
                        "\u{1f5a5}\u{fe0f} Terminal en cours d'ouverture...",
                    )
                    .await;

                match start_pty_session(ctx.clone(), bot.clone(), chat_id, pty_tx.clone()).await {
                    Ok(msg_text) => {
                        let _ = bot.send_message(recipient, msg_text).await;
                    }
                    Err(e) => {
                        let _ = bot
                            .send_message(recipient, format!("\u{274c} Erreur terminal: {}", e))
                            .await;
                    }
                }
                return Ok(());
            }

            "run" => {
                if args.is_empty() {
                    let _ = bot.send_message(recipient, "Usage: /run <commande>").await;
                    return Ok(());
                }

                let ts = TerminalStream::new(bot.clone(), chat_id, ctx.bound_system.clone());
                let desc = format!("Execution: {}", args);
                let result = ts.execute_and_stream(args, &desc).await;
                if let Err(e) = result {
                    let _ = bot.send_message(recipient, format!("Erreur: {}", e)).await;
                }
                return Ok(());
            }

            "veille" => {
                if args.is_empty() {
                    let topics: Vec<String> = {
                        let avid = ctx.avid.lock().await;
                        avid.list_topics()
                            .iter()
                            .map(|t| format!("- {} (par {})", t.topic, t.added_by))
                            .collect()
                    };
                    if topics.is_empty() {
                        let _ = bot
                            .send_message(recipient, "Aucun sujet de veille. /veille <sujet>")
                            .await;
                    } else {
                        let _ = bot
                            .send_message(
                                recipient,
                                format!("Sujets de veille:\n{}", topics.join("\n")),
                            )
                            .await;
                    }
                } else {
                    for topic in args.split(',') {
                        let topic = topic.trim();
                        if !topic.is_empty() {
                            let mut avid = ctx.avid.lock().await;
                            avid.add_topic(topic, &user_id);
                        }
                    }
                    let _ = bot
                        .send_message(recipient, format!("Veille ajoutee: {}", args))
                        .await;
                }
                return Ok(());
            }

            "status" => {
                let version = env!("CARGO_PKG_VERSION");
                let bus_subs = ctx.bus.subscriber_count();
                let _ = bot
                    .send_message(
                        recipient,
                        format!("\u{1f99e} Clawd v{} | Bus: {} abonnes", version, bus_subs),
                    )
                    .await;
                return Ok(());
            }
            "help" => {
                let _ = bot
                    .send_message(
                        recipient,
                        "\u{1f916} Clawd — Agent SoulSystem\n\
                         /status — Statut systeme\n\
                         /run <cmd> — Executer une commande shell\n\
                         /terminal — Ouvrir un terminal PTY persistant\n\
                         /exit — Fermer le terminal\n\
                         /veille <sujets> — Veille AVID\n\
                         /skill <nom> — Executer un skill local\n\
                         /help — Cette aide",
                    )
                    .await;
                return Ok(());
            }

            _ => {}
        }
    }
    // PTY Mode: forward input to terminal
    let mut pty_locked = ctx.pty_sessions.lock().await;
    if let Some(session) = pty_locked.get_mut(&chat_id) {
        let trimmed = text.trim();

        if trimmed == "/exit" {
            session.reader_handle.abort();
            let _ = pty_tx.send((chat_id, "\u{1f5a5}\u{fe0f} Terminal ferme.".into()));
            pty_locked.remove(&chat_id);
            return Ok(());
        }

        session.last_activity = std::time::Instant::now();
        let _ = session.input_tx.send(format!("{}\n", text));
        return Ok(());
    }
    drop(pty_locked);

    // Chat with LLM — avec routage bus pour la plateforme unifiee
    let model = ctx.model_router.route(text);
    let context_data = ctx.memory.get_context(text).await.unwrap_or_default();

    let prompt = if context_data.is_empty() {
        text.to_string()
    } else {
        format!(
            "Contexte pertinent:\n{}\n\nQuestion: {}",
            context_data, text
        )
    };

    // Publier sur le bus pour les services externes (gateway Node.js)
    ctx.bus.publish(crate::bus::Message::Custom {
        topic: "telegram.message".to_string(),
        payload: serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "message_id": msg.id.0,
            "user_id": user_id,
        }),
    });

    // Attendre une reponse sur le bus (timeout 5s)
    let bus_reply = {
        let mut rx = ctx.bus.subscribe();
        let timeout = tokio::time::Duration::from_secs(5);
        let chat_id_f = chat_id;
        tokio::time::timeout(timeout, async {
            loop {
                match rx.recv().await {
                    Ok(crate::bus::Message::Custom { topic, payload })
                        if topic == "telegram.reply" =>
                    {
                        if payload.get("chat_id").and_then(|v| v.as_i64()) == Some(chat_id_f) {
                            return Some(payload);
                        }
                    }
                    Ok(_) => continue,
                    Err(_) => return None,
                }
            }
        })
        .await
        .ok()
        .flatten()
    };

    let response = if let Some(reply) = bus_reply {
        // Reponse du bus (gateway Node.js ou autre service)
        reply
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("[Reponse bus vide]")
            .to_string()
    } else {
        // Fallback LLM local
        ClawdContext::call_ollama(model, &prompt).await
    };

    // Detect and auto-execute shell commands
    let shell_cmds = ClawdContext::extract_shell_commands(&response);
    let mut exec_summary = Vec::new();

    if !shell_cmds.is_empty() {
        let ts = TerminalStream::new(bot.clone(), chat_id, ctx.bound_system.clone());

        for shell_cmd in &shell_cmds {
            let desc = format!("Auto-exec: {}", shell_cmd);
            match ts.execute_and_stream(shell_cmd, &desc).await {
                Ok(s) => exec_summary.push(format!("  {} -> {}", shell_cmd, s)),
                Err(e) => exec_summary.push(format!("  {} -> ERREUR: {}", shell_cmd, e)),
            }
        }
    }

    // Save to memory
    let mut meta = HashMap::new();
    meta.insert("source".into(), "telegram".into());
    meta.insert("user".into(), user_id.clone());
    let _ = ctx.memory.store(text, meta).await;

    // Build response text
    let mut reply = format!("[{}] {}", model, response);

    if !exec_summary.is_empty() {
        reply.push_str("\n\n---\n*Commandes executees:*\n");
        for s in &exec_summary {
            reply.push_str(s);
            reply.push('\n');
        }
    }

    // Handle long messages
    if reply.len() > 3800 {
        let llm_part = format!("[{}] {}", model, response);
        let mut current = String::new();
        for line in llm_part.lines() {
            if current.len() + line.len() + 1 > 3800 {
                let _ = bot.send_message(recipient.clone(), &current).await;
                current = String::new();
            }
            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(line);
        }
        if !current.is_empty() {
            let _ = bot.send_message(recipient.clone(), &current).await;
        }
        if !exec_summary.is_empty() {
            let mut exec_text = "*Commandes executees:*\n".to_string();
            for s in &exec_summary {
                exec_text.push_str(s);
                exec_text.push('\n');
            }
            let _ = bot.send_message(recipient, &exec_text).await;
        }
    } else {
        let _ = bot.send_message(recipient, &reply).await;
    }

    Ok(())
}

/// Handler pour les callbacks (bouton Stop).
async fn handle_callback(
    q: teloxide::types::CallbackQuery,
    bot: Bot,
    ctx: Arc<ClawdContext>,
) -> Result<()> {
    let data = q.data.as_deref().unwrap_or("");

    if let Some(exec_id) = data.strip_prefix("stop:") {
        let exec_id = exec_id.to_string();

        // Tuer le processus via SIGTERM + SIGKILL
        {
            let mut execs = ctx.active_executions.lock().await;
            if let Some(info) = execs.remove(&exec_id) {
                if let Some(pid) = info.pid {
                    let _ = BoundSystem::kill_process(pid, 15);
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        let _ = BoundSystem::kill_process(pid, 9);
                    });
                }
                let _ = info.cancel_tx.try_send(());
            }
        }

        // Répondre au callback (sinon le bouton reste en "loading")
        if let Some(msg) = q.message {
            let _ = bot
                .answer_callback_query(&q.id)
                .text("Commande arrêtée")
                .await;
            // Supprimer le clavier du message
            let _ = bot.edit_message_reply_markup(msg.chat().id, msg.id()).await;
        }
    }

    Ok(())
}

/// Lance une session PTY persistante pour un chat.
async fn start_pty_session(
    ctx: Arc<ClawdContext>,
    _bot: Bot,
    chat_id: i64,
    pty_tx: tokio::sync::mpsc::UnboundedSender<(i64, String)>,
) -> Result<String> {
    use crate::pty_terminal::PtyTerminal;

    let pty = PtyTerminal::new()?;
    let (input_tx, mut input_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let pty_tx_clone = pty_tx.clone();
    let pty_clone = pty.clone();

    let reader_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                input = input_rx.recv() => {
                    match input {
                        Some(text) => {
                            let _ = pty_clone.write(&text);
                        }
                        None => break,
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(500)) => {
                    match pty_clone.read() {
                        Ok(output) if !output.is_empty() => {
                            let _ = pty_tx_clone.send((chat_id, output));
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    let session = PtySession {
        input_tx,
        reader_handle: reader_handle.abort_handle(),
        last_msg_id: None,
        last_activity: std::time::Instant::now(),
        session_name: None,
    };

    ctx.pty_sessions.lock().await.insert(chat_id, session);

    Ok("\u{1f5a5}\u{fe0f} Terminal ouvert (bash via bwrap).\n\
         Tapez vos commandes directement.\n\
         /exit pour fermer.\n\
         Timeout: 30 min d'inactivite."
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_shell_block() {
        let resp = "Voici:\n```sh\nls -la\n```\nFin.";
        let cmds = ClawdContext::extract_shell_commands(resp);
        assert_eq!(cmds, vec!["ls -la"]);
    }

    #[test]
    fn test_extract_prefix_line() {
        let resp = "Execute:\n$ echo hello\n> date\nFin.";
        let cmds = ClawdContext::extract_shell_commands(resp);
        assert_eq!(cmds, vec!["echo hello", "date"]);
    }

    #[test]
    fn test_extract_no_commands() {
        let resp = "Juste du texte.";
        let cmds = ClawdContext::extract_shell_commands(resp);
        assert!(cmds.is_empty());
    }

    #[test]
    fn test_extract_multiple_blocks() {
        let resp = "```sh\nls\n```\net\n```bash\npwd\n```";
        let cmds = ClawdContext::extract_shell_commands(resp);
        assert_eq!(cmds, vec!["ls", "pwd"]);
    }
}
