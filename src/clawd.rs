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

use crate::audit_log::AuditLog;
use crate::bound_system::BoundSystem;
use crate::local_skills::BuiltinSkills;
use crate::model_router::ModelRouter;
use crate::soul_memory::SoulMemory;
use crate::terminal_stream::TerminalStream;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
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
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await?;
                let results = body["results"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(results)
            }
            _ => Ok(vec![
                format!("[MOCK] Resultat arXiv pour '{}': Paper 1", topic),
                format!("[MOCK] Resultat arXiv pour '{}': Paper 2", topic),
                format!("[MOCK] Resultat arXiv pour '{}': Paper 3", topic),
            ]),
        }
    }
}

// ── Clawd Config ────────────────────────────────────────────────────────

/// Configuration de Clawd.
#[derive(Debug, Clone)]
pub struct ClawdConfig {
    pub bot_token: String,
    pub allowed_users: Vec<String>,
    pub avid_endpoint: String,
}

// ── PTY Terminal Mode ───────────────────────────────────────────────────

/// Mode terminal actif par chat.
#[derive(Debug)]
pub(crate) struct PtySession {
    /// Write side of the channel to send input to the PTY.
    input_tx: tokio::sync::mpsc::UnboundedSender<String>,
    /// Handle to the PTY reader task (abort on /exit).
    reader_handle: tokio::task::AbortHandle,
    /// Last Telegram message ID for output display.
    last_msg_id: Option<teloxide::types::MessageId>,
    /// Timestamp of last activity.
    last_activity: std::time::Instant,
}

// ── Clawd Core ──────────────────────────────────────────────────────────

/// Contexte partage de Clawd.
pub struct ClawdContext {
    pub memory: Arc<SoulMemory>,
    pub audit: Arc<Mutex<AuditLog>>,
    pub feedback: Arc<FeedbackStore>,
    pub bound_system: Arc<BoundSystem>,
    pub builtin_skills: Arc<BuiltinSkills>,
    pub model_router: Arc<ModelRouter>,
    pub avid_watcher: Arc<Mutex<AvidWatcher>>,
    pub config: ClawdConfig,
    /// Cache des dernieres interactions (query -> response) pour le feedback.
    pub last_interactions: Arc<Mutex<HashMap<String, (String, String)>>>,
    /// Sessions PTY actives par chat_id.
    pub(crate) pty_sessions: Arc<Mutex<HashMap<i64, PtySession>>>,
}

impl ClawdContext {
    pub fn new(
        memory: Arc<SoulMemory>,
        audit: Arc<Mutex<AuditLog>>,
        feedback: Arc<FeedbackStore>,
        bound_system: Arc<BoundSystem>,
        config: ClawdConfig,
    ) -> Self {
        Self {
            memory,
            audit: audit.clone(),
            feedback,
            bound_system,
            builtin_skills: Arc::new(BuiltinSkills::new()),
            model_router: Arc::new(ModelRouter::default_models()),
            avid_watcher: Arc::new(Mutex::new(AvidWatcher::new(config.avid_endpoint.clone()))),
            config,
            last_interactions: Arc::new(Mutex::new(HashMap::new())),
            pty_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Traite une commande textuelle et retourne la reponse.
    pub async fn handle_command(&self, user_id: &str, text: &str) -> String {
        let parts: Vec<&str> = text.splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let args = parts.get(1).unwrap_or(&"");

        match cmd.as_str() {
            "/veille" => self.cmd_veille(user_id, args).await,
            "/skill" => self.cmd_skill(user_id, args).await,
            "/run" => self.cmd_run(user_id, args).await,
            "/start" | "/help" => self.cmd_help().await,
            _ => self.cmd_chat(user_id, text).await,
        }
    }

    async fn cmd_help(&self) -> String {
        let skills = self.builtin_skills.list().join(", ");
        let whitelist: Vec<&str> = vec!["date", "df -h", "uptime", "free -h", "whoami"];
        format!(
            "Clawd Operator Edition v{}\n\n\
             Commandes:\n\
             /veille <sujets> — Surveiller des sujets via AVID\n\
             /skill <nom> <args> — Executer un skill ({})\n\
             /run <commande> — Executer une commande autorisee ({})\n\
             /terminal — Ouvrir un terminal shell persistant\n\
             /exit — Fermer le terminal actif\n\
             /help — Cette aide\n\
             \n\
             Texte libre — Discussion avec LLM local (Ollama)\n\
             Blocs ```shell ou `$ cmd` — Execution automatique streamée",
            env!("CARGO_PKG_VERSION"),
            skills,
            whitelist.join(", "),
        )
    }

    async fn cmd_veille(&self, user_id: &str, args: &str) -> String {
        if args.is_empty() {
            let topics = self.avid_watcher.lock().await.list_topics().to_vec();
            if topics.is_empty() {
                return "Aucun sujet de veille. Usage: /veille sujet1, sujet2".into();
            }
            return format!(
                "Sujets de veille actifs:\n{}",
                topics
                    .iter()
                    .map(|t| format!("- {} (par {})", t.topic, t.added_by))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }

        let mut watcher = self.avid_watcher.lock().await;
        for topic in args.split(',') {
            let topic = topic.trim();
            if !topic.is_empty() {
                watcher.add_topic(topic, user_id);
            }
        }
        format!("Veille activee pour: {}", args)
    }

    async fn cmd_skill(&self, _user_id: &str, args: &str) -> String {
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        let name = parts[0];
        let input = parts.get(1).unwrap_or(&"");

        if name.is_empty() {
            return format!(
                "Skills disponibles: {}\nUsage: /skill <nom> <args>",
                self.builtin_skills.list().join(", ")
            );
        }

        match self.builtin_skills.execute(name, input) {
            Ok(result) => result,
            Err(e) => format!("Erreur: {}", e),
        }
    }

    async fn cmd_run(&self, user_id: &str, args: &str) -> String {
        if args.is_empty() {
            return "Usage: /run <commande>. Commandes autorisees: date, df -h, uptime, free -h, whoami, hostname".into();
        }

        match self.bound_system.execute(args).await {
            Ok(result) => {
                let mut out = format!("Commande: {}\n", result.command);
                if !result.stdout.is_empty() {
                    out.push_str(&format!("Sortie:\n{}", result.stdout));
                }
                if !result.stderr.is_empty() {
                    out.push_str(&format!("Erreur:\n{}", result.stderr));
                }
                if result.timed_out {
                    out.push_str("\n\u{26a0} Timeout (10s)");
                }

                if let Ok(mut a) = self.audit.try_lock() {
                    let _ = a.log(
                        "clawd",
                        "command_run",
                        &format!("user={} cmd={}", user_id, args),
                    );
                }

                out
            }
            Err(e) => format!("Erreur: {}", e),
        }
    }

    async fn cmd_chat(&self, user_id: &str, text: &str) -> String {
        let model = self.model_router.route(text);
        let context = self.memory.get_context(text).await.unwrap_or_default();

        let prompt = if context.is_empty() {
            text.to_string()
        } else {
            format!("Contexte pertinent:\n{}\n\nQuestion: {}", context, text)
        };

        let response = Self::call_ollama(model, &prompt).await;

        let mut meta = HashMap::new();
        meta.insert("source".into(), "telegram".into());
        meta.insert("user".into(), user_id.to_string());
        let _ = self.memory.store(text, meta).await;

        let interaction_id = uuid_v4();
        {
            let mut cache = self.last_interactions.lock().await;
            cache.insert(interaction_id.clone(), (text.to_string(), response.clone()));
        }

        format!(
            "[{}] {}\n\n_Envoyez /help pour les commandes._",
            model, response
        )
    }

    /// Extrait les commandes shell d'une reponse LLM.
    /// Detecte: ```shell ... ```, lignes `$ cmd`, lignes `> cmd`.
    pub fn extract_shell_commands(text: &str) -> Vec<String> {
        let mut commands = Vec::new();

        // Bloc code shell: ```shell\n...\n```
        let shell_block_re = regex::Regex::new(r"(?s)```shell\s*\n(.*?)```").unwrap();
        for cap in shell_block_re.captures_iter(text) {
            for line in cap[1].lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    commands.push(trimmed.to_string());
                }
            }
        }

        // Lignes prefixees $ ou > : "$ ls -la" "> cargo build"
        let cmd_line_re = regex::Regex::new(r"^\s*[$>]\s+(.+)$").unwrap();
        for line in text.lines() {
            if let Some(cap) = cmd_line_re.captures(line) {
                let cmd = cap[1].trim().to_string();
                if !commands.contains(&cmd) {
                    commands.push(cmd);
                }
            }
        }

        commands
    }

    /// Appelle Ollama pour une inference.
    async fn call_ollama(model: &str, prompt: &str) -> String {
        let client = reqwest::Client::new();
        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "num_predict": 512,
                "temperature": 0.7
            }
        });

        match client
            .post("http://localhost:11434/api/generate")
            .json(&body)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let json: serde_json::Value = resp.json().await.unwrap_or_default();
                json["response"]
                    .as_str()
                    .unwrap_or("(reponse vide)")
                    .trim()
                    .to_string()
            }
            Ok(resp) => {
                format!("Ollama error: HTTP {}", resp.status())
            }
            Err(_) => {
                format!(
                    "[Ollama indisponible] Le modele '{}' n'a pas repondu. \
                     Verifiez que le service Ollama est actif sur le port 11434.",
                    model
                )
            }
        }
    }

    /// Enregistre un feedback pour une interaction.
    pub async fn record_feedback(
        &self,
        user_id: &str,
        interaction_id: &str,
        score: i32,
    ) -> Result<()> {
        let (query, response) = {
            let cache = self.last_interactions.lock().await;
            cache
                .get(interaction_id)
                .cloned()
                .unwrap_or_else(|| ("(inconnu)".into(), "(inconnu)".into()))
        };

        self.feedback.record(user_id, &query, &response, score)?;
        Ok(())
    }

    /// Lance une veille quotidienne (stocke dans SoulMemory).
    pub async fn run_daily_watch(&self) -> Result<usize> {
        let topics = {
            let watcher = self.avid_watcher.lock().await;
            watcher.list_topics().to_vec()
        };

        let mut stored = 0usize;
        for topic in &topics {
            let watcher = self.avid_watcher.lock().await;
            match watcher.research(&topic.topic).await {
                Ok(results) => {
                    for result in results {
                        let mut meta = HashMap::new();
                        meta.insert("source".into(), "avid_veille".into());
                        meta.insert("tag".into(), "veille".into());
                        meta.insert("topic".into(), topic.topic.clone());
                        if self.memory.store(&result, meta).await.is_ok() {
                            stored += 1;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("AVID watch failed for '{}': {}", topic.topic, e);
                }
            }
        }

        Ok(stored)
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

    // Message handler
    let handler = {
        let ctx = context.clone();

        move |msg: teloxide::types::Message, bot: Bot| {
            let ctx = ctx.clone();
            let bot = bot.clone();
            let pty_tx = pty_tx.clone();

            async move {
                let chat_id = msg.chat.id.0;
                let text = msg.text().unwrap_or("");
                let user_id = msg
                    .from
                    .as_ref()
                    .map(|u| u.id.0.to_string())
                    .unwrap_or_else(|| "unknown".into());

                // ── PTY Mode: forward input to terminal ────────────────────
                let mut pty_locked = ctx.pty_sessions.lock().await;
                if let Some(session) = pty_locked.get_mut(&chat_id) {
                    let trimmed = text.trim();

                    if trimmed == "/exit" {
                        session.reader_handle.abort();
                        let out_tx = pty_tx.clone();
                        let _ = out_tx.send((chat_id, "\u{1f5a5}\u{fe0f} Terminal ferme.".into()));
                        pty_locked.remove(&chat_id);
                        return Ok(());
                    }

                    session.last_activity = std::time::Instant::now();
                    let _ = session.input_tx.send(format!("{}\n", text));
                    return Ok(());
                }
                drop(pty_locked);

                // ── Commands ──────────────────────────────────────────────
                let parts: Vec<&str> = text.splitn(2, ' ').collect();
                let cmd = parts[0].to_lowercase();
                let args = parts.get(1).unwrap_or(&"");
                let recipient = Recipient::Id(teloxide::types::ChatId(chat_id));

                match cmd.as_str() {
                    "/terminal" => {
                        let _ = bot
                            .send_message(
                                recipient.clone(),
                                "\u{1f5a5}\u{fe0f} Terminal en cours d'ouverture...",
                            )
                            .await;

                        match start_pty_session(ctx.clone(), bot.clone(), chat_id, pty_tx.clone())
                            .await
                        {
                            Ok(msg) => {
                                let _ = bot.send_message(recipient, msg).await;
                            }
                            Err(e) => {
                                let _ = bot
                                    .send_message(
                                        recipient,
                                        format!("\u{274c} Erreur terminal: {}", e),
                                    )
                                    .await;
                            }
                        }
                        return Ok(());
                    }

                    "/run" => {
                        if args.is_empty() {
                            let _ = bot.send_message(recipient, "Usage: /run <commande>").await;
                            return Ok(());
                        }

                        // Stream via TerminalStream
                        let ts =
                            TerminalStream::new(bot.clone(), chat_id, ctx.bound_system.clone());
                        let desc = format!("Execution: {}", args);
                        let result = ts.execute_and_stream(args, &desc).await;
                        if let Err(e) = result {
                            let _ = bot.send_message(recipient, format!("Erreur: {}", e)).await;
                        }
                        return Ok(());
                    }

                    _ => {}
                }

                // ── Chat with LLM ─────────────────────────────────────────
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

                let response = ClawdContext::call_ollama(model, &prompt).await;

                // Detect and auto-execute shell commands
                let shell_cmds = ClawdContext::extract_shell_commands(&response);
                let mut exec_summary = Vec::new();

                if !shell_cmds.is_empty() {
                    let ts = TerminalStream::new(bot.clone(), chat_id, ctx.bound_system.clone());

                    for shell_cmd in &shell_cmds {
                        let desc = format!("Auto-exec: {}", shell_cmd);
                        match ts.execute_and_stream(shell_cmd, &desc).await {
                            Ok(s) => exec_summary.push(format!("  {} -> {}", shell_cmd, s)),
                            Err(e) => {
                                exec_summary.push(format!("  {} -> ERREUR: {}", shell_cmd, e))
                            }
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
                    // Send LLM response first
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
        }
    };

    // ── PTY Output Forwarder ─────────────────────────────────────────────
    let bot_pty = bot.clone();
    let ctx_pty = context.clone();
    tokio::spawn(async move {
        let bot = bot_pty;
        loop {
            match pty_rx.recv().await {
                Some((chat_id, output)) => {
                    let recipient = Recipient::Id(teloxide::types::ChatId(chat_id));

                    // Update last message or send new
                    let mut sessions = ctx_pty.pty_sessions.lock().await;
                    let updated = if let Some(session) = sessions.get_mut(&chat_id) {
                        session.last_activity = std::time::Instant::now();
                        if let Some(msg_id) = session.last_msg_id {
                            let truncated = if output.len() > 3800 {
                                let mut t = output[output.len().saturating_sub(3800)..].to_string();
                                t.insert_str(0, "... (sortie tronquee)\n");
                                t
                            } else {
                                output.clone()
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
                        let truncated = if output.len() > 3800 {
                            let mut t = output[output.len().saturating_sub(3800)..].to_string();
                            t.insert_str(0, "... (sortie tronquee)\n");
                            t
                        } else {
                            output
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
                                // Fallback: send without parse mode
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

    // ── Main Polling Loop ────────────────────────────────────────────────
    teloxide::repl(bot, handler).await;

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
    };

    ctx.pty_sessions.lock().await.insert(chat_id, session);

    Ok("\u{1f5a5}\u{fe0f} Terminal ouvert (bash via bwrap).\n\
         Tapez vos commandes directement.\n\
         /exit pour fermer.\n\
         Timeout: 30 min d'inactivite."
        .to_string())
}
