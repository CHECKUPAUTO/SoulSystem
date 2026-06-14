//! App logic — creation, event handling, sessions, file browser, commands.

use crate::types::*;
use crate::utils::*;
use crate::ReplState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use soul_llm::LlmClient;
use std::collections::VecDeque;
use std::fs;
use std::io::{self};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

impl App {
    pub(crate) fn new(state: &ReplState, llm_tx: mpsc::UnboundedSender<LlmEvent>) -> Self {
        let provider = ProviderOpt::from_kind(&state.llm.config().provider);
        let model_items = vec![state.llm.config().model.clone()];
        let provider_items = vec![
            ProviderOpt::Ollama,
            ProviderOpt::OpenAI,
            ProviderOpt::Anthropic,
        ];
        let provider_selected = provider_items
            .iter()
            .position(|p| *p == provider)
            .unwrap_or(0);

        let command_palette_items = vec![
            CommandPaletteItem {
                name: "ask".into(),
                description: "Poser une question au LLM".into(),
                shortcut: Some("/ask".into()),
                action: CommandAction::Ask,
            },
            CommandPaletteItem {
                name: "plan".into(),
                description: "Créer un plan".into(),
                shortcut: Some("/plan".into()),
                action: CommandAction::Plan,
            },
            CommandPaletteItem {
                name: "run".into(),
                description: "Exécuter une commande".into(),
                shortcut: Some("/run".into()),
                action: CommandAction::Run,
            },
            CommandPaletteItem {
                name: "models".into(),
                description: "Lister les modèles".into(),
                shortcut: Some("/models".into()),
                action: CommandAction::Models,
            },
            CommandPaletteItem {
                name: "status".into(),
                description: "État du système".into(),
                shortcut: Some("/status".into()),
                action: CommandAction::Status,
            },
            CommandPaletteItem {
                name: "clear".into(),
                description: "Effacer l'historique".into(),
                shortcut: None,
                action: CommandAction::Clear,
            },
            CommandPaletteItem {
                name: "save-session".into(),
                description: "Sauvegarder la session".into(),
                shortcut: Some("Ctrl+S".into()),
                action: CommandAction::SaveSession,
            },
            CommandPaletteItem {
                name: "load-session".into(),
                description: "Charger une session".into(),
                shortcut: Some("Ctrl+O".into()),
                action: CommandAction::LoadSession,
            },
            CommandPaletteItem {
                name: "export".into(),
                description: "Exporter le chat".into(),
                shortcut: None,
                action: CommandAction::ExportChat,
            },
            CommandPaletteItem {
                name: "copy".into(),
                description: "Copier la dernière réponse".into(),
                shortcut: Some("Ctrl+Y".into()),
                action: CommandAction::CopyLast,
            },
            CommandPaletteItem {
                name: "files".into(),
                description: "Navigateur de fichiers".into(),
                shortcut: Some("Ctrl+F".into()),
                action: CommandAction::FileBrowser,
            },
            CommandPaletteItem {
                name: "history-search".into(),
                description: "Rechercher dans l'historique".into(),
                shortcut: Some("Ctrl+R".into()),
                action: CommandAction::HistorySearch,
            },
            CommandPaletteItem {
                name: "help".into(),
                description: "Aide".into(),
                shortcut: Some("Ctrl+H".into()),
                action: CommandAction::Help,
            },
        ];

        let sessions = Self::load_sessions_from_disk();

        Self {
            messages: vec![Message {
                role: Role::System,
                text: "Bienvenue dans SoulSystem REPL.\nCtrl+Shift+P pour le palette de commandes.\nCtrl+R pour la recherche, Ctrl+F pour les fichiers.".into(),
                timestamp: Instant::now(),
                tool_call: None,
            }],
            input: String::new(),
            input_cursor: 0,
            input_history: VecDeque::new(),
            history_index: None,
            focus: Focus::Input,
            scroll_offset: 0,
            model_items,
            model_selected: 0,
            provider_items,
            provider_selected,
            streaming: false,
            stream_buffer: String::new(),
            llm_tx,
            toasts: Vec::new(),
            last_input_time: Instant::now(),
            total_tokens: 0,
            command_count: 0,

            file_browser_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            file_browser_entries: Vec::new(),
            file_browser_selected: 0,
            file_browser_scroll: 0,

            command_palette_items,
            command_palette_selected: 0,
            command_palette_filter: String::new(),

            history_search: None,

            sessions,
            session_selected: 0,

            diff_content: Vec::new(),
        }
    }

    // ── Session persistence ─────────────────────────────────────────

    fn sessions_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("soulsystem")
            .join("sessions")
    }

    fn load_sessions_from_disk() -> Vec<SessionEntry> {
        let dir = Self::sessions_dir();
        if !dir.exists() {
            return Vec::new();
        }
        fs::read_dir(&dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.path()
                            .extension()
                            .map(|ext| ext == "json")
                            .unwrap_or(false)
                    })
                    .filter_map(|e| {
                        let path = e.path();
                        let name = path.file_stem()?.to_str()?.to_string();
                        let content = fs::read_to_string(&path).ok()?;
                        let data: serde_json::Value = serde_json::from_str(&content).ok()?;
                        Some(SessionEntry {
                            name,
                            path,
                            message_count: data["messages"]
                                .as_array()
                                .map(|a| a.len())
                                .unwrap_or(0),
                            created_at: data["created_at"]
                                .as_str()
                                .unwrap_or("unknown")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn save_session(&self) -> Result<String, String> {
        let dir = Self::sessions_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let name = format!("session_{}", timestamp);
        let path = dir.join(format!("{}.json", name));

        let data = serde_json::json!({
            "name": name,
            "created_at": chrono::Local::now().to_rfc3339(),
            "messages": self.messages.iter().map(|m| {
                serde_json::json!({
                    "role": format!("{:?}", m.role),
                    "text": m.text,
                    "timestamp": m.timestamp.elapsed().as_secs(),
                })
            }).collect::<Vec<_>>(),
        });

        fs::write(
            &path,
            serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;

        Ok(name)
    }

    fn export_chat(&self) -> Result<String, String> {
        let dir = Self::sessions_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let path = dir.join(format!("export_{}.md", timestamp));

        let mut content = String::from("# SoulSystem Chat Export\n\n");
        for msg in &self.messages {
            let role = match msg.role {
                Role::User => "You",
                Role::Assistant => "Assistant",
                Role::System => "System",
                Role::Error => "Error",
                Role::Tool => "Tool",
            };
            content.push_str(&format!("**{}**:\n{}\n\n", role, msg.text));
        }

        fs::write(&path, &content).map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    }

    // ── File browser ────────────────────────────────────────────────

    fn refresh_file_browser(&mut self) {
        self.file_browser_entries.clear();
        self.file_browser_selected = 0;
        self.file_browser_scroll = 0;

        // Add parent directory
        if let Some(parent) = self.file_browser_path.parent() {
            self.file_browser_entries.push(FileEntry {
                name: "..".into(),
                path: parent.to_path_buf(),
                is_dir: true,
                size: 0,
            });
        }

        if let Ok(entries) = fs::read_dir(&self.file_browser_path) {
            let mut dirs: Vec<FileEntry> = Vec::new();
            let mut files: Vec<FileEntry> = Vec::new();

            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                if name.starts_with('.') {
                    continue;
                }

                let is_dir = path.is_dir();
                let size = if is_dir {
                    0
                } else {
                    fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
                };

                let entry = FileEntry {
                    name,
                    path,
                    is_dir,
                    size,
                };

                if is_dir {
                    dirs.push(entry);
                } else {
                    files.push(entry);
                }
            }

            dirs.sort_by(|a, b| a.name.cmp(&b.name));
            files.sort_by(|a, b| a.name.cmp(&b.name));

            self.file_browser_entries.extend(dirs);
            self.file_browser_entries.extend(files);
        }
    }

    fn open_file_browser(&mut self) {
        self.refresh_file_browser();
        self.focus = Focus::FileBrowser;
    }

    fn file_browser_select(&mut self) {
        if let Some(entry) = self.file_browser_entries.get(self.file_browser_selected) {
            if entry.is_dir {
                self.file_browser_path = entry.path.clone();
                self.refresh_file_browser();
            } else {
                // Insert file path into input
                let relative = self.file_browser_path.join(&entry.name);
                let path_str = relative.display().to_string();
                self.input = path_str.clone();
                self.input_cursor = self.input.len();
                self.focus = Focus::Input;
                self.add_toast(
                    ToastLevel::Info,
                    &format!("Fichier sélectionné: {}", path_str),
                );
            }
        }
    }

    // ── Command palette ─────────────────────────────────────────────

    fn open_command_palette(&mut self) {
        self.command_palette_filter.clear();
        self.command_palette_selected = 0;
        self.focus = Focus::CommandPalette;
    }

    #[allow(dead_code)]
    fn filter_command_palette(&mut self) {
        // Filtering is done in draw by checking command_palette_filter
        self.command_palette_selected = 0;
    }

    fn execute_palette_action(&mut self, action: &CommandAction) {
        match action {
            CommandAction::Ask => {
                self.focus = Focus::Input;
            }
            CommandAction::Plan => {
                self.focus = Focus::Input;
                self.input = "/plan ".into();
                self.input_cursor = self.input.len();
            }
            CommandAction::Run => {
                self.focus = Focus::Input;
                self.input = "/run ".into();
                self.input_cursor = self.input.len();
            }
            CommandAction::Models => {
                self.focus = Focus::Input;
                self.command_count += 1;
                // Will be handled in execute_command
            }
            CommandAction::Status => {
                self.focus = Focus::Input;
            }
            CommandAction::Help => {
                self.focus = Focus::HelpDialog;
            }
            CommandAction::Clear => {
                self.messages.clear();
                self.add_toast(ToastLevel::Success, "Historique effacé");
                self.focus = Focus::Input;
            }
            CommandAction::SaveSession => {
                match self.save_session() {
                    Ok(name) => self.add_toast(
                        ToastLevel::Success,
                        &format!("Session sauvegardée: {}", name),
                    ),
                    Err(e) => self.add_toast(ToastLevel::Error, &format!("Erreur: {}", e)),
                }
                self.focus = Focus::Input;
            }
            CommandAction::LoadSession => {
                self.sessions = Self::load_sessions_from_disk();
                self.session_selected = 0;
                self.focus = Focus::SessionManager;
            }
            CommandAction::ExportChat => {
                match self.export_chat() {
                    Ok(path) => self.add_toast(ToastLevel::Success, &format!("Exporté: {}", path)),
                    Err(e) => self.add_toast(ToastLevel::Error, &format!("Erreur: {}", e)),
                }
                self.focus = Focus::Input;
            }
            CommandAction::CopyLast => {
                if let Some(msg) = self
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::Assistant)
                {
                    self.copy_to_clipboard(&msg.text);
                    self.add_toast(ToastLevel::Success, "Copié dans le presse-papier");
                }
                self.focus = Focus::Input;
            }
            CommandAction::FileBrowser => {
                self.open_file_browser();
            }
            CommandAction::HistorySearch => {
                self.history_search = Some(HistorySearch {
                    query: String::new(),
                    cursor: 0,
                    results: Vec::new(),
                    selected: 0,
                });
                self.focus = Focus::HistorySearch;
            }
            CommandAction::ThemeChange => {
                self.add_toast(ToastLevel::Info, "Thème: (pas encore implémenté)");
                self.focus = Focus::Input;
            }
        }
    }

    // ── History search ──────────────────────────────────────────────

    fn update_history_search(&mut self) {
        if let Some(ref mut search) = self.history_search {
            search.results.clear();
            if !search.query.is_empty() {
                for (i, msg) in self.messages.iter().enumerate() {
                    if msg
                        .text
                        .to_lowercase()
                        .contains(&search.query.to_lowercase())
                    {
                        search.results.push(i);
                    }
                }
            }
            search.selected = search.results.len().saturating_sub(1);
        }
    }

    // ── Clipboard ───────────────────────────────────────────────────

    fn copy_to_clipboard(&self, text: &str) {
        // OSC 52 escape sequence for clipboard
        let encoded = base64_encode(text);
        print!("\x1b]52;c;{}\x07", encoded);
        let _ = io::Write::flush(&mut io::stdout());
    }

    // ── Toast system ────────────────────────────────────────────────

    fn add_toast(&mut self, level: ToastLevel, message: &str) {
        self.toasts.push(Toast {
            message: message.to_string(),
            level,
            created_at: Instant::now(),
        });
    }

    pub(crate) fn tick(&mut self) {
        // Remove old toasts (3 seconds)
        self.toasts
            .retain(|t| t.created_at.elapsed() < Duration::from_secs(3));
    }

    // ── LLM event handling ──────────────────────────────────────────

    pub(crate) fn handle_llm_event(&mut self, evt: LlmEvent) {
        match evt {
            LlmEvent::Response(text) => {
                self.streaming = false;
                self.add_message(Role::Assistant, &text);
                self.add_toast(ToastLevel::Success, "Réponse reçue");
            }
            LlmEvent::Error(e) => {
                self.streaming = false;
                self.add_message(Role::Error, &e);
                self.add_toast(ToastLevel::Error, &e);
            }
            LlmEvent::StreamChunk(chunk) => {
                self.stream_buffer.push_str(&chunk);
            }
            LlmEvent::StreamDone => {
                if !self.stream_buffer.is_empty() {
                    let text = self.stream_buffer.clone();
                    self.stream_buffer.clear();
                    self.streaming = false;
                    self.add_message(Role::Assistant, &text);
                }
            }
            LlmEvent::ToolCall { name, args } => {
                self.add_message(Role::Tool, &format!("→ {}({})", name, args));
            }
            LlmEvent::ToolResult { name, output } => {
                self.add_message(Role::Tool, &format!("← {}: {}", name, output));
            }
        }
    }

    // ── Key handling ────────────────────────────────────────────────

    pub(crate) async fn handle_key(&mut self, key: KeyEvent, llm: &LlmClient) -> bool {
        match self.focus {
            Focus::Input => self.handle_input_key(key, llm).await,
            Focus::ModelDialog => self.handle_dialog_key(key, true),
            Focus::ProviderDialog => self.handle_dialog_key(key, false),
            Focus::HelpDialog => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
                    self.focus = Focus::Input;
                }
                false
            }
            Focus::CommandPalette => self.handle_command_palette_key(key),
            Focus::FileBrowser => self.handle_file_browser_key(key),
            Focus::HistorySearch => self.handle_history_search_key(key),
            Focus::SessionManager => self.handle_session_manager_key(key),
        }
    }

    async fn handle_input_key(&mut self, key: KeyEvent, llm: &LlmClient) -> bool {
        // Global shortcuts
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => return true,
                KeyCode::Char('p') => {
                    self.focus = Focus::ProviderDialog;
                    self.provider_selected = self
                        .provider_items
                        .iter()
                        .position(|p| *p == ProviderOpt::from_kind(&llm.config().provider))
                        .unwrap_or(0);
                    return false;
                }
                KeyCode::Char('m') => {
                    self.focus = Focus::ModelDialog;
                    self.model_selected = 0;
                    return false;
                }
                KeyCode::Char('h') => {
                    self.focus = Focus::HelpDialog;
                    return false;
                }
                KeyCode::Char('f') => {
                    self.open_file_browser();
                    return false;
                }
                KeyCode::Char('r') => {
                    self.history_search = Some(HistorySearch {
                        query: String::new(),
                        cursor: 0,
                        results: Vec::new(),
                        selected: 0,
                    });
                    self.focus = Focus::HistorySearch;
                    return false;
                }
                KeyCode::Char('s') => {
                    match self.save_session() {
                        Ok(name) => self.add_toast(
                            ToastLevel::Success,
                            &format!("Session sauvegardée: {}", name),
                        ),
                        Err(e) => self.add_toast(ToastLevel::Error, &e),
                    }
                    return false;
                }
                KeyCode::Char('o') => {
                    self.sessions = Self::load_sessions_from_disk();
                    self.session_selected = 0;
                    self.focus = Focus::SessionManager;
                    return false;
                }
                KeyCode::Char('y') => {
                    if let Some(msg) = self
                        .messages
                        .iter()
                        .rev()
                        .find(|m| m.role == Role::Assistant)
                    {
                        self.copy_to_clipboard(&msg.text);
                        self.add_toast(ToastLevel::Success, "Copié dans le presse-papier");
                    }
                    return false;
                }
                KeyCode::Char('a') => {
                    self.input_cursor = 0;
                    return false;
                }
                KeyCode::Char('e') => {
                    self.input_cursor = self.input.len();
                    return false;
                }
                KeyCode::Char('u') => {
                    self.input.clear();
                    self.input_cursor = 0;
                    return false;
                }
                KeyCode::Char('w') => {
                    let before = &self.input[..self.input_cursor];
                    let new_pos = before.rfind(' ').map(|p| p + 1).unwrap_or(0);
                    self.input.drain(new_pos..self.input_cursor);
                    self.input_cursor = new_pos;
                    return false;
                }
                _ => {}
            }
        }

        // Ctrl+Shift+P for command palette
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::SHIFT)
        {
            if let KeyCode::Char('P') | KeyCode::Char('p') = key.code {
                self.open_command_palette();
                return false;
            }
        }

        match key.code {
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Multi-line: insert newline
                    self.input.insert(self.input_cursor, '\n');
                    self.input_cursor += 1;
                } else {
                    let input = self.input.trim().to_string();
                    if input.is_empty() {
                        return false;
                    }

                    // Add to history
                    self.input_history.push_back(input.clone());
                    if self.input_history.len() > 100 {
                        self.input_history.pop_front();
                    }
                    self.history_index = None;

                    self.input.clear();
                    self.input_cursor = 0;
                    if input == "exit" || input == "quit" {
                        return true;
                    }
                    self.add_message(Role::User, &input);
                    self.execute_command(&input, llm).await;
                }
            }
            KeyCode::Up => {
                if self.input_history.is_empty() {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                } else {
                    // Navigate history
                    let new_index = match self.history_index {
                        None => Some(self.input_history.len() - 1),
                        Some(i) => {
                            if i > 0 {
                                Some(i - 1)
                            } else {
                                Some(0)
                            }
                        }
                    };
                    if let Some(idx) = new_index {
                        if let Some(cmd) = self.input_history.get(idx) {
                            self.input = cmd.clone();
                            self.input_cursor = self.input.len();
                            self.history_index = Some(idx);
                        }
                    }
                }
            }
            KeyCode::Down => {
                if let Some(idx) = self.history_index {
                    let new_index = idx + 1;
                    if new_index >= self.input_history.len() {
                        self.history_index = None;
                        self.input.clear();
                        self.input_cursor = 0;
                    } else if let Some(cmd) = self.input_history.get(new_index) {
                        self.input = cmd.clone();
                        self.input_cursor = self.input.len();
                        self.history_index = Some(new_index);
                    }
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                }
            }
            KeyCode::Tab => {
                // Autocomplete from file browser or history
                self.autocomplete();
            }
            KeyCode::Char(c) => {
                self.input.insert(self.input_cursor, c);
                self.input_cursor += 1;
            }
            KeyCode::Backspace => {
                if self.input_cursor > 0 {
                    self.input_cursor -= 1;
                    self.input.remove(self.input_cursor);
                }
            }
            KeyCode::Delete => {
                if self.input_cursor < self.input.len() {
                    self.input.remove(self.input_cursor);
                }
            }
            KeyCode::Left => {
                self.input_cursor = self.input_cursor.saturating_sub(1);
            }
            KeyCode::Right => {
                if self.input_cursor < self.input.len() {
                    self.input_cursor += 1;
                }
            }
            KeyCode::Home => self.input_cursor = 0,
            KeyCode::End => self.input_cursor = self.input.len(),
            KeyCode::PageUp => self.scroll_offset = self.scroll_offset.saturating_sub(10),
            KeyCode::PageDown => self.scroll_offset = self.scroll_offset.saturating_add(10),
            _ => {}
        }
        false
    }

    fn autocomplete(&mut self) {
        let input = &self.input[..self.input_cursor];
        if input.is_empty() {
            return;
        }

        // Try to autocomplete file paths
        let last_word = input
            .rsplit(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("");
        if last_word.contains('/') || last_word.contains('.') {
            let (dir, prefix) = if let Some(pos) = last_word.rfind('/') {
                let dir = if last_word.starts_with('/') {
                    PathBuf::from(&last_word[..=pos])
                } else {
                    self.file_browser_path.join(&last_word[..=pos])
                };
                (dir, &last_word[pos + 1..])
            } else {
                (self.file_browser_path.clone(), last_word)
            };

            if let Ok(entries) = fs::read_dir(&dir) {
                let matches: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|name| name.starts_with(prefix))
                    .collect();

                if matches.len() == 1 {
                    let completion = &matches[0];
                    let new_input =
                        format!("{}{}", &input[..input.len() - last_word.len()], completion);
                    self.input = new_input;
                    self.input_cursor = self.input.len();
                } else if matches.len() > 1 {
                    self.add_toast(
                        ToastLevel::Info,
                        &format!("{} options: {}", matches.len(), matches.join(", ")),
                    );
                }
            }
        }
    }

    // ── Command palette key handling ────────────────────────────────

    fn handle_command_palette_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Input;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.command_palette_selected = self.command_palette_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self
                    .filtered_command_palette_items()
                    .len()
                    .saturating_sub(1);
                self.command_palette_selected = self.command_palette_selected.min(max);
            }
            KeyCode::Enter => {
                let items = self.filtered_command_palette_items();
                if let Some(item) = items.get(self.command_palette_selected) {
                    let action = item.action.clone();
                    self.execute_palette_action(&action);
                }
            }
            KeyCode::Char(c) => {
                self.command_palette_filter.push(c);
                self.command_palette_selected = 0;
            }
            KeyCode::Backspace => {
                self.command_palette_filter.pop();
                self.command_palette_selected = 0;
            }
            _ => {}
        }
        false
    }

    pub(crate) fn filtered_command_palette_items(&self) -> Vec<&CommandPaletteItem> {
        if self.command_palette_filter.is_empty() {
            self.command_palette_items.iter().collect()
        } else {
            self.command_palette_items
                .iter()
                .filter(|item| {
                    item.name
                        .to_lowercase()
                        .contains(&self.command_palette_filter.to_lowercase())
                        || item
                            .description
                            .to_lowercase()
                            .contains(&self.command_palette_filter.to_lowercase())
                })
                .collect()
        }
    }

    // ── File browser key handling ───────────────────────────────────

    fn handle_file_browser_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.focus = Focus::Input;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.file_browser_selected = self.file_browser_selected.saturating_sub(1);
                if self.file_browser_selected < self.file_browser_scroll {
                    self.file_browser_scroll = self.file_browser_selected;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.file_browser_entries.len().saturating_sub(1);
                self.file_browser_selected = self.file_browser_selected.min(max);
                if self.file_browser_selected >= self.file_browser_scroll + 20 {
                    self.file_browser_scroll = self.file_browser_selected - 19;
                }
            }
            KeyCode::PageUp => {
                self.file_browser_selected = self.file_browser_selected.saturating_sub(20);
                self.file_browser_scroll = self.file_browser_scroll.saturating_sub(20);
            }
            KeyCode::PageDown => {
                let max = self.file_browser_entries.len().saturating_sub(1);
                self.file_browser_selected = (self.file_browser_selected + 20).min(max);
            }
            KeyCode::Enter => {
                self.file_browser_select();
            }
            KeyCode::Backspace => {
                if let Some(parent) = self.file_browser_path.parent() {
                    self.file_browser_path = parent.to_path_buf();
                    self.refresh_file_browser();
                }
            }
            _ => {}
        }
        false
    }

    // ── History search key handling ─────────────────────────────────

    fn handle_history_search_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.history_search = None;
                self.focus = Focus::Input;
            }
            KeyCode::Enter => {
                if let Some(ref search) = self.history_search {
                    if let Some(&idx) = search.results.get(search.selected) {
                        if let Some(msg) = self.messages.get(idx) {
                            self.input = msg.text.clone();
                            self.input_cursor = self.input.len();
                        }
                    }
                }
                self.history_search = None;
                self.focus = Focus::Input;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(ref mut search) = self.history_search {
                    search.selected = search.selected.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(ref mut search) = self.history_search {
                    let max = search.results.len().saturating_sub(1);
                    search.selected = search.selected.min(max);
                }
            }
            KeyCode::Char(c) => {
                if let Some(ref mut search) = self.history_search {
                    search.query.push(c);
                    self.update_history_search();
                }
            }
            KeyCode::Backspace => {
                if let Some(ref mut search) = self.history_search {
                    search.query.pop();
                    self.update_history_search();
                }
            }
            _ => {}
        }
        false
    }

    // ── Session manager key handling ────────────────────────────────

    fn handle_session_manager_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.focus = Focus::Input;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.session_selected = self.session_selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.sessions.len().saturating_sub(1);
                self.session_selected = self.session_selected.min(max);
            }
            KeyCode::Enter => {
                if let Some(session) = self.sessions.get(self.session_selected) {
                    if let Ok(content) = fs::read_to_string(&session.path) {
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                            if let Some(msgs) = data["messages"].as_array() {
                                self.messages.clear();
                                for msg in msgs {
                                    let role = msg["role"].as_str().unwrap_or("System");
                                    let text = msg["text"].as_str().unwrap_or("");
                                    let role = match role {
                                        "User" => Role::User,
                                        "Assistant" => Role::Assistant,
                                        "Error" => Role::Error,
                                        "Tool" => Role::Tool,
                                        _ => Role::System,
                                    };
                                    self.messages.push(Message {
                                        role,
                                        text: text.to_string(),
                                        timestamp: Instant::now(),
                                        tool_call: None,
                                    });
                                }
                                self.add_toast(
                                    ToastLevel::Success,
                                    &format!(
                                        "Session '{}' chargée ({} messages)",
                                        session.name,
                                        msgs.len()
                                    ),
                                );
                            }
                        }
                    }
                }
                self.focus = Focus::Input;
            }
            KeyCode::Char('d') => {
                if let Some(session) = self.sessions.get(self.session_selected) {
                    let _ = fs::remove_file(&session.path);
                    self.sessions = Self::load_sessions_from_disk();
                    self.session_selected = 0;
                    self.add_toast(ToastLevel::Info, "Session supprimée");
                }
            }
            _ => {}
        }
        false
    }

    // ── Dialog key handling ─────────────────────────────────────────

    fn handle_dialog_key(&mut self, key: KeyEvent, is_model: bool) -> bool {
        match key.code {
            KeyCode::Esc => self.focus = Focus::Input,
            KeyCode::Up | KeyCode::Char('k') => {
                if is_model {
                    self.model_selected = self.model_selected.saturating_sub(1);
                } else {
                    self.provider_selected = self.provider_selected.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if is_model {
                    self.model_selected = self
                        .model_selected
                        .min(self.model_items.len().saturating_sub(1));
                } else {
                    self.provider_selected = self
                        .provider_selected
                        .min(self.provider_items.len().saturating_sub(1));
                }
            }
            KeyCode::Enter => {
                if is_model {
                    let model = self.model_items[self.model_selected].clone();
                    self.add_message(Role::System, &format!("Modèle changé: {}", model));
                    self.add_toast(ToastLevel::Success, &format!("Modèle: {}", model));
                } else {
                    let provider = self.provider_items[self.provider_selected].clone();
                    self.add_message(
                        Role::System,
                        &format!("Provider changé: {}", provider.label()),
                    );
                    self.add_toast(
                        ToastLevel::Success,
                        &format!("Provider: {}", provider.label()),
                    );
                }
                self.focus = Focus::Input;
            }
            _ => {}
        }
        false
    }

    // ── Message management ──────────────────────────────────────────

    fn add_message(&mut self, role: Role, text: &str) {
        self.messages.push(Message {
            role,
            text: text.to_string(),
            timestamp: Instant::now(),
            tool_call: None,
        });
        let total_lines: usize = self
            .messages
            .iter()
            .map(|m| m.text.lines().count().max(1))
            .sum();
        if total_lines > 20 {
            self.scroll_offset = (total_lines - 20) as u16;
        }
    }

    // ── Command execution ───────────────────────────────────────────

    async fn execute_command(&mut self, input: &str, llm: &LlmClient) {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0];
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match cmd {
            "ask" | "/ask" => self.cmd_ask(arg, llm).await,
            "help" | "/help" => self.focus = Focus::HelpDialog,
            "models" | "/models" => match llm.list_models().await {
                Ok(models) => {
                    let list = models
                        .iter()
                        .map(|m| format!("  {}", m.name))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.add_message(Role::System, &list);
                }
                Err(e) => self.add_message(Role::Error, &e.to_string()),
            },
            "status" | "/status" => {
                let alive = llm.is_alive().await;
                self.add_message(
                    Role::System,
                    &format!(
                        "Provider: {} | LLM: {} | Messages: {} | Commandes: {}",
                        llm.config().provider,
                        if alive { "connecté" } else { "déconnecté" },
                        self.messages.len(),
                        self.command_count
                    ),
                );
            }
            "plan" | "/plan" => {
                if arg.is_empty() {
                    self.add_message(Role::Error, "Usage: plan <objectif>");
                } else {
                    self.add_message(Role::System, &format!("Plan créé pour \"{}\"", arg));
                }
            }
            "run" | "/run" => {
                if arg.is_empty() {
                    self.add_message(Role::Error, "Usage: run <commande>");
                } else {
                    self.add_message(Role::System, &format!("Exécution: {}", arg));
                }
            }
            "observe" | "/observe" => {
                if arg.is_empty() {
                    self.add_message(Role::Error, "Usage: observe <message>");
                } else {
                    self.add_message(Role::System, &format!("Observé: {}", arg));
                }
            }
            "clear" | "/clear" => {
                self.messages.clear();
                self.add_toast(ToastLevel::Success, "Historique effacé");
            }
            "save" | "/save" => match self.save_session() {
                Ok(name) => self.add_toast(ToastLevel::Success, &format!("Session: {}", name)),
                Err(e) => self.add_toast(ToastLevel::Error, &e),
            },
            "export" | "/export" => match self.export_chat() {
                Ok(path) => self.add_toast(ToastLevel::Success, &format!("Exporté: {}", path)),
                Err(e) => self.add_toast(ToastLevel::Error, &e),
            },
            "files" | "/files" => {
                self.open_file_browser();
            }
            "search" | "/search" => {
                self.history_search = Some(HistorySearch {
                    query: arg.to_string(),
                    cursor: 0,
                    results: Vec::new(),
                    selected: 0,
                });
                self.update_history_search();
                self.focus = Focus::HistorySearch;
            }
            _ => self.cmd_ask(input, llm).await,
        }
    }

    async fn cmd_ask(&mut self, msg: &str, llm: &LlmClient) {
        if msg.is_empty() {
            self.add_message(Role::Error, "Usage: ask <message>");
            return;
        }
        self.streaming = true;
        self.stream_buffer.clear();
        self.command_count += 1;
        self.last_input_time = Instant::now();

        let prompt = msg.to_string();
        let llm_clone = llm.clone();
        let tx = self.llm_tx.clone();

        tokio::spawn(async move {
            match llm_clone.generate(&prompt).await {
                Ok(resp) => {
                    let _ = tx.send(LlmEvent::Response(resp.text));
                }
                Err(e) => {
                    let _ = tx.send(LlmEvent::Error(format!("Erreur LLM: {}", e)));
                }
            }
        });
    }

    // ── Drawing ─────────────────────────────────────────────────────
}
