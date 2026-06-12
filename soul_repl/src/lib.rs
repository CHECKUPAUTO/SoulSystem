//! # soul_repl — TUI interactif pour l'entité autonome
//!
//! Interface terminal riche inspirée d'OpenCode :
//! - Barre de statut en haut (provider, modèle, session)
//! - Zone de chat avec timeline des messages
//! - Zone de saisie en bas
//! - Barre latérale optionnelle (historique des sessions)
//! - Dialogues modaux (sélection modèle/provider)

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io;
use std::sync::Arc;
use std::time::Duration;

use soul_llm::{LlmClient, LlmConfig, ProviderKind};
use soul_persistence::LongTermMemory;
use soul_planner::{CognitiveLoop, Goal, GoalStatus};
use soul_sandbox::Sandbox;
use soul_tools::ToolRegistry;

// ─── Public API ────────────────────────────────────────────────────────

pub struct ReplState {
    pub llm: LlmClient,
    pub planner: CognitiveLoop,
    pub tools: ToolRegistry,
    pub sandbox: Arc<Sandbox>,
    pub memory: Option<Arc<LongTermMemory>>,
    pub entity_name: String,
}

impl ReplState {
    pub fn new(config: LlmConfig) -> Self {
        let mut reg = ToolRegistry::new();
        for t in soul_tools::discover_system_tools() {
            reg.register(t);
        }
        Self {
            llm: LlmClient::new(config).expect("failed to init LLM client"),
            planner: CognitiveLoop::new(),
            tools: reg,
            sandbox: Arc::new(Sandbox::new(soul_sandbox::SandboxPolicy::default())),
            memory: None,
            entity_name: "soul".into(),
        }
    }

    pub fn with_memory(mut self, mem: Arc<LongTermMemory>) -> Self {
        self.memory = Some(mem);
        self
    }
}

/// Point d'entrée du TUI.
pub async fn run_repl(state: &mut ReplState) {
    enable_raw_mode().expect("failed to enable raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("failed to enter alternate screen");
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");

    let mut app = App::new(state);

    loop {
        terminal
            .draw(|f| app.draw(f))
            .expect("failed to draw");

        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if app.handle_key(key, state).await {
                    break;
                }
            }
        }

        // Drain streaming chunks if active
        app.drain_stream(state).await;
    }

    disable_raw_mode().expect("failed to disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .expect("failed to leave alternate screen");
    terminal.show_cursor().expect("failed to show cursor");
}

// ─── App State ─────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum Focus {
    Chat,
    Input,
    ModelDialog,
    ProviderDialog,
    HelpDialog,
}

#[derive(Clone, PartialEq, Debug)]
enum ProviderOpt {
    Ollama,
    OpenAI,
    Anthropic,
}

impl ProviderOpt {
    fn label(&self) -> &str {
        match self {
            Self::Ollama => "Ollama",
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
        }
    }
    fn from_kind(k: &ProviderKind) -> Self {
        match k {
            ProviderKind::Ollama => Self::Ollama,
            ProviderKind::OpenAI => Self::OpenAI,
            ProviderKind::Anthropic => Self::Anthropic,
        }
    }
    fn to_kind(&self) -> ProviderKind {
        match self {
            Self::Ollama => ProviderKind::Ollama,
            Self::OpenAI => ProviderKind::OpenAI,
            Self::Anthropic => ProviderKind::Anthropic,
        }
    }
}

struct Message {
    role: Role,
    text: String,
}

#[derive(Clone, PartialEq, Debug)]
enum Role {
    User,
    Assistant,
    System,
    Error,
}

struct App {
    messages: Vec<Message>,
    input: String,
    input_cursor: usize,
    focus: Focus,
    scroll_offset: u16,
    sidebar_visible: bool,
    sidebar_items: Vec<String>,
    sidebar_state: ListState,
    model_items: Vec<String>,
    model_selected: usize,
    provider_items: Vec<ProviderOpt>,
    provider_selected: usize,
    streaming: bool,
    stream_buffer: String,
    status_msg: String,
}

impl App {
    fn new(state: &ReplState) -> Self {
        let provider = ProviderOpt::from_kind(&state.llm.config().provider);
        let model_items = vec![state.llm.config().model.clone()];
        let provider_items = vec![ProviderOpt::Ollama, ProviderOpt::OpenAI, ProviderOpt::Anthropic];
        let provider_selected = provider_items.iter().position(|p| *p == provider).unwrap_or(0);

        let mut sidebar_state = ListState::default();
        sidebar_state.select(Some(0));

        Self {
            messages: vec![Message {
                role: Role::System,
                text: format!(
                    "Bienvenue dans SoulSystem REPL.\nTapez votre message ou utilisez les commandes.\nRaccourcis: Ctrl+P (provider), Ctrl+M (modèle), Ctrl+B (barre latérale), Ctrl+H (aide), Ctrl+C (quitter)"
                ),
            }],
            input: String::new(),
            input_cursor: 0,
            focus: Focus::Input,
            scroll_offset: 0,
            sidebar_visible: false,
            sidebar_items: vec![
                "Nouvelle session".into(),
                "---".into(),
                "Sessions récentes".into(),
            ],
            sidebar_state,
            model_items,
            model_selected: 0,
            provider_items,
            provider_selected,
            streaming: false,
            stream_buffer: String::new(),
            status_msg: String::new(),
        }
    }

    // ── Key handling ────────────────────────────────────────────────

    async fn handle_key(&mut self, key: KeyEvent, state: &mut ReplState) -> bool {
        match self.focus {
            Focus::Input => self.handle_input_key(key, state).await,
            Focus::ModelDialog => self.handle_dialog_key(key, state, true).await,
            Focus::ProviderDialog => self.handle_dialog_key(key, state, false).await,
            Focus::HelpDialog => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
                    self.focus = Focus::Input;
                }
                false
            }
            Focus::Chat => {
                match key.code {
                    KeyCode::Char('i') | KeyCode::Enter => {
                        self.focus = Focus::Input;
                    }
                    KeyCode::Up => {
                        self.scroll_up();
                    }
                    KeyCode::Down => {
                        self.scroll_down();
                    }
                    KeyCode::PageUp => {
                        self.scroll_offset = self.scroll_offset.saturating_sub(10);
                    }
                    KeyCode::PageDown => {
                        self.scroll_offset = self.scroll_offset.saturating_add(10);
                    }
                    _ => {}
                }
                false
            }
        }
    }

    async fn handle_input_key(&mut self, key: KeyEvent, state: &mut ReplState) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => return true,
                KeyCode::Char('p') => {
                    self.focus = Focus::ProviderDialog;
                    self.provider_selected = self
                        .provider_items
                        .iter()
                        .position(|p| *p == ProviderOpt::from_kind(&state.llm.config().provider))
                        .unwrap_or(0);
                    return false;
                }
                KeyCode::Char('m') => {
                    self.focus = Focus::ModelDialog;
                    self.model_selected = 0;
                    return false;
                }
                KeyCode::Char('b') => {
                    self.sidebar_visible = !self.sidebar_visible;
                    return false;
                }
                KeyCode::Char('h') => {
                    self.focus = Focus::HelpDialog;
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
                    // delete word backward
                    let before = &self.input[..self.input_cursor];
                    let new_pos = before.rfind(' ').map(|p| p + 1).unwrap_or(0);
                    self.input.drain(new_pos..self.input_cursor);
                    self.input_cursor = new_pos;
                    return false;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Enter => {
                let input = self.input.trim().to_string();
                if input.is_empty() {
                    return false;
                }
                self.input.clear();
                self.input_cursor = 0;

                if input == "exit" || input == "quit" {
                    return true;
                }

                self.add_message(Role::User, &input);
                self.execute_command(&input, state).await;
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
            KeyCode::Up => {
                self.scroll_up();
            }
            KeyCode::Down => {
                self.scroll_down();
            }
            KeyCode::Home => {
                self.input_cursor = 0;
            }
            KeyCode::End => {
                self.input_cursor = self.input.len();
            }
            _ => {}
        }
        false
    }

    async fn handle_dialog_key(&mut self, key: KeyEvent, _state: &mut ReplState, is_model: bool) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Input;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if is_model {
                    self.model_selected = self.model_selected.saturating_sub(1);
                } else {
                    self.provider_selected = self.provider_selected.saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if is_model {
                    let max = self.model_items.len().saturating_sub(1);
                    self.model_selected = self.model_selected.min(max);
                } else {
                    let max = self.provider_items.len().saturating_sub(1);
                    self.provider_selected = self.provider_selected.min(max);
                }
            }
            KeyCode::Enter => {
                if is_model {
                    let model = self.model_items[self.model_selected].clone();
                    self.status_msg = format!("Modèle sélectionné: {}", model);
                    self.add_message(Role::System, &format!("Modèle changé: {}", model));
                } else {
                    let provider = self.provider_items[self.provider_selected].clone();
                    self.status_msg = format!("Provider sélectionné: {}", provider.label());
                    self.add_message(
                        Role::System,
                        &format!("Provider changé: {}", provider.label()),
                    );
                }
                self.focus = Focus::Input;
            }
            _ => {}
        }
        false
    }

    // ── Scroll ──────────────────────────────────────────────────────

    fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    // ── Messages ────────────────────────────────────────────────────

    fn add_message(&mut self, role: Role, text: &str) {
        self.messages.push(Message {
            role,
            text: text.to_string(),
        });
        // auto-scroll to bottom
        let total_lines: usize = self.messages.iter().map(|m| m.text.lines().count().max(1)).sum();
        let visible = 20; // approximate
        if total_lines > visible {
            self.scroll_offset = (total_lines - visible) as u16;
        }
    }

    // ── Command execution ───────────────────────────────────────────

    async fn execute_command(&mut self, input: &str, state: &mut ReplState) {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0];
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match cmd {
            "ask" => self.cmd_ask(arg, state).await,
            "plan" => self.cmd_plan(arg, state),
            "run" => self.cmd_run(arg, state),
            "sandbox" => self.cmd_sandbox(arg, state),
            "tools" => {
                let out = self.cmd_tools(state);
                self.add_message(Role::System, &out);
            }
            "memory" => {
                let out = self.cmd_memory(state);
                self.add_message(Role::System, &out);
            }
            "memory-browse" => {
                let out = self.cmd_memory_browse(arg, state);
                self.add_message(Role::System, &out);
            }
            "observe" => {
                let out = self.cmd_observe(arg, state);
                self.add_message(Role::System, &out);
            }
            "decide" => {
                let out = self.cmd_decide(arg, state);
                self.add_message(Role::System, &out);
            }
            "history" => {
                let out = self.cmd_history(state);
                self.add_message(Role::System, &out);
            }
            "models" => {
                let out = self.cmd_models(state).await;
                self.add_message(Role::System, &out);
            }
            "status" => {
                let out = self.cmd_status(state).await;
                self.add_message(Role::System, &out);
            }
            "help" => self.focus = Focus::HelpDialog,
            _ => {
                // Treat as direct LLM message
                self.cmd_ask(input, state).await;
            }
        }
    }

    async fn cmd_ask(&mut self, msg: &str, state: &mut ReplState) {
        if msg.is_empty() {
            self.add_message(Role::Error, "Usage: ask <message>");
            return;
        }
        self.streaming = true;
        self.stream_buffer.clear();

        match state.llm.generate(msg).await {
            Ok(resp) => {
                self.add_message(Role::Assistant, &resp.text);
            }
            Err(e) => {
                self.add_message(Role::Error, &format!("Erreur LLM: {}", e));
            }
        }
        self.streaming = false;
    }

    fn cmd_plan(&mut self, goal_desc: &str, state: &mut ReplState) {
        if goal_desc.is_empty() {
            self.add_message(Role::Error, "Usage: plan <objectif>");
            return;
        }
        let goal = Goal {
            id: uuid::Uuid::new_v4().to_string(),
            description: goal_desc.to_string(),
            priority: 5,
            created_at: chrono::Utc::now(),
            status: GoalStatus::Active,
        };
        let plan = state.planner.create_plan(&goal, &["analyze", "execute"]);
        let mut out = format!("Plan créé: {} étapes\n", plan.steps.len());
        for s in &plan.steps {
            out.push_str(&format!("  [{}] {}\n", s.order, s.command));
        }
        self.add_message(Role::System, &out);
    }

    fn cmd_run(&mut self, shell_cmd: &str, state: &mut ReplState) {
        if shell_cmd.is_empty() {
            self.add_message(Role::Error, "Usage: run <commande>");
            return;
        }
        match state.sandbox.execute(shell_cmd) {
            Ok(v) => {
                let mut out = v.stdout;
                if !v.stderr.is_empty() {
                    out.push_str("\n[stderr]\n");
                    out.push_str(&v.stderr);
                }
                if !v.allowed {
                    out.push_str("\n[bloqué par sandbox]");
                }
                self.add_message(Role::System, &out);
            }
            Err(e) => {
                self.add_message(Role::Error, &e.to_string());
            }
        }
    }

    fn cmd_sandbox(&mut self, args: &str, state: &mut ReplState) {
        if args.is_empty() {
            self.add_message(Role::Error, "Usage: sandbox history|scan|policy");
            return;
        }
        let parts: Vec<&str> = args.splitn(2, ' ').collect();
        let out = match parts[0] {
            "history" => {
                let h = state.sandbox.history();
                if h.is_empty() {
                    "Historique sandbox vide".into()
                } else {
                    h.iter()
                        .rev()
                        .take(10)
                        .map(|v| {
                            format!(
                                "  [{}ms] {} {}",
                                v.duration_ms,
                                if v.exit_code == Some(0) { "OK" } else { "ERR" },
                                v.command
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            "scan" => {
                let cmd = parts.get(1).copied().unwrap_or("");
                if cmd.is_empty() {
                    "Usage: sandbox scan <cmd>".into()
                } else {
                    let threats = state.sandbox.scan(cmd);
                    if !threats.is_empty() {
                        format!("Menaces: {:?}", threats)
                    } else {
                        let paths = state.sandbox.scan_paths(cmd);
                        if !paths.is_empty() {
                            format!("Chemins sensibles: {:?}", paths)
                        } else {
                            match state.sandbox.check(cmd) {
                                Ok(b) => format!("OK (binaire={})", b),
                                Err(e) => format!("Erreur: {}", e),
                            }
                        }
                    }
                }
            }
            "policy" => {
                let p = state.sandbox.policy();
                format!(
                    "whitelist={}, timeout={}s, log_all={}",
                    if p.whitelist.is_some() { "oui" } else { "non" },
                    p.timeout.as_secs(),
                    p.log_all
                )
            }
            _ => "Usage: sandbox history|scan|policy".into(),
        };
        self.add_message(Role::System, &out);
    }

    fn cmd_tools(&self, state: &mut ReplState) -> String {
        let all = state.tools.all();
        if all.is_empty() {
            return "Aucun outil disponible.".into();
        }
        all.iter()
            .map(|t| format!("  {:20} {}", t.name, t.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn cmd_memory(&self, state: &mut ReplState) -> String {
        let obs = state.planner.memory.recent_observations(10);
        if obs.is_empty() {
            return "Mémoire vide.".into();
        }
        obs.iter()
            .enumerate()
            .map(|(i, o)| format!("  [{}] {}", i + 1, o))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn cmd_memory_browse(&self, kind: &str, state: &mut ReplState) -> String {
        let Some(ref mem) = state.memory else {
            return "Aucune mémoire persistante attachée.".into();
        };
        let k = if kind.is_empty() { "all" } else { kind };
        if k == "all" {
            let kinds = [
                "goal", "plan", "observation", "tool_result", "code_artifact", "decision",
            ];
            kinds
                .iter()
                .map(|k| {
                    let ids = mem.list_by_kind(k);
                    format!("{} ({})", k, ids.len())
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            let ids = mem.list_by_kind(k);
            if ids.is_empty() {
                return format!("Aucune entrée de type {}", k);
            }
            ids.iter()
                .take(10)
                .filter_map(|id| {
                    mem.get(id)
                        .ok()
                        .map(|e| format!("  - {} (tags: {:?})", &e.id[..8], e.tags))
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    fn cmd_observe(&mut self, msg: &str, state: &mut ReplState) -> String {
        if msg.is_empty() {
            return "Usage: observe <message>".into();
        }
        state.planner.memory.observe(msg.to_string());
        format!("Observé: {}", msg)
    }

    fn cmd_decide(&self, ctx: &str, state: &mut ReplState) -> String {
        if ctx.is_empty() {
            return "Usage: decide <contexte>".into();
        }
        let decision = state.planner.decide(ctx);
        format!(
            "Décision: \"{}\"\n  Raisonnement: {}\n  Confiance: {:.0}%",
            decision.action,
            decision.reasoning,
            decision.confidence * 100.0
        )
    }

    fn cmd_history(&self, state: &mut ReplState) -> String {
        let recent = state.planner.history.recent(5);
        if recent.is_empty() {
            return "Aucune action enregistrée.".into();
        }
        recent
            .iter()
            .rev()
            .map(|r| {
                format!(
                    "  [{}] {} {}",
                    r.timestamp.format("%H:%M"),
                    if r.success { "OK" } else { "ERR" },
                    if r.result.len() > 60 {
                        format!("{}...", &r.result[..60])
                    } else {
                        r.result.clone()
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn cmd_models(&self, state: &mut ReplState) -> String {
        match state.llm.list_models().await {
            Ok(models) => models
                .iter()
                .map(|m| format!("  {}", m.name))
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("Erreur: {}", e),
        }
    }

    async fn cmd_status(&self, state: &mut ReplState) -> String {
        let alive = state.llm.is_alive().await;
        let success_rate = state.planner.history.success_rate();
        let sandbox_h = state.sandbox.history_len();
        let mem_count = state.memory.as_ref().map(|m| m.len()).unwrap_or(0);
        format!(
            "Provider: {} | LLM: {} | Succès: {:.0}% | Sandbox: {} | Mémoire: {}",
            state.llm.config().provider,
            if alive { "connecté" } else { "déconnecté" },
            success_rate * 100.0,
            sandbox_h,
            mem_count
        )
    }

    // ── Streaming drain ─────────────────────────────────────────────

    async fn drain_stream(&mut self, _state: &mut ReplState) {
        // Future: poll stream chunks and update stream_buffer
    }

    // ── Drawing ─────────────────────────────────────────────────────

    fn draw(&mut self, f: &mut Frame) {
        let size = f.area();

        // Main layout: [sidebar? | chat+input]
        let chunks = if self.sidebar_visible {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(24),
                    Constraint::Min(40),
                ])
                .split(size)
        } else {
            Layout::default()
                .constraints([Constraint::Min(40)])
                .split(size)
        };

        if self.sidebar_visible {
            self.draw_sidebar(f, chunks[0]);
        }

        let main_area = if self.sidebar_visible { chunks[1] } else { chunks[0] };

        // Vertical: [status | chat | input]
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // status bar
                Constraint::Min(5),   // chat area
                Constraint::Length(3), // input area
            ])
            .split(main_area);

        self.draw_status_bar(f, vertical[0]);
        self.draw_chat(f, vertical[1]);
        self.draw_input(f, vertical[2]);

        // Overlay dialogs
        match self.focus {
            Focus::ModelDialog => self.draw_model_dialog(f, size),
            Focus::ProviderDialog => self.draw_provider_dialog(f, size),
            Focus::HelpDialog => self.draw_help_dialog(f, size),
            _ => {}
        }
    }

    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let _provider = match self.messages.last().map(|m| &m.role) {
            _ => "SoulSystem",
        };
        let model_info = if self.streaming { " ..." } else { "" };

        let status_line = Line::from(vec![
            Span::styled(
                " SoulSystem ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ", self.provider_items[self.provider_selected].label()),
                Style::default().fg(Color::White).bg(Color::DarkGray),
            ),
            Span::styled(
                format!(" {}{} ", self.model_items.first().unwrap_or(&String::new()), model_info),
                Style::default().fg(Color::Green),
            ),
            Span::raw(" "),
            Span::styled(
                format!(" msgs:{} ", self.messages.len()),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(" "),
            Span::styled(
                if self.sidebar_visible { " Side:ON " } else { " Side:OFF " },
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().bg(Color::Black));

        let paragraph = Paragraph::new(status_line)
            .block(block)
            .style(Style::default().bg(Color::Black));
        f.render_widget(paragraph, area);
    }

    fn draw_chat(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Chat ")
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        for msg in &self.messages {
            let (prefix, style) = match msg.role {
                Role::User => (
                    "you",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Role::Assistant => (
                    "ai",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Role::System => (
                    "sys",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Role::Error => (
                    "err",
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", prefix), style),
                Span::styled(&msg.text, Style::default().fg(Color::White)),
            ]));
            lines.push(Line::raw(""));
        }

        // Streaming indicator
        if self.streaming && !self.stream_buffer.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("ai ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(&self.stream_buffer, Style::default().fg(Color::White)),
            ]));
        }

        let paragraph = Paragraph::new(lines)
            .scroll((self.scroll_offset, 0))
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(Style::default().fg(if self.focus == Focus::Input {
                Color::Cyan
            } else {
                Color::DarkGray
            }))
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(area);
        f.render_widget(block, area);

        // Show cursor indicator
        let display = if self.input.is_empty() && self.focus == Focus::Input {
            Span::styled(
                "Tapez votre message...",
                Style::default().fg(Color::DarkGray),
            )
        } else {
            Span::styled(&self.input, Style::default().fg(Color::White))
        };

        let paragraph = Paragraph::new(Line::from(display));
        f.render_widget(paragraph, inner);

        // Show cursor position
        if self.focus == Focus::Input {
            let cursor_x = inner.x + self.input_cursor as u16;
            let cursor_y = inner.y;
            f.set_cursor_position((cursor_x, cursor_y));
        }
    }

    fn draw_sidebar(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Sessions ")
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().bg(Color::Black));

        let items: Vec<ListItem> = self
            .sidebar_items
            .iter()
            .map(|i| {
                ListItem::new(Line::from(Span::styled(
                    i.as_str(),
                    Style::default().fg(Color::White),
                )))
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        f.render_stateful_widget(list, area, &mut self.sidebar_state);
    }

    fn draw_model_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(50, 40, size);
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Sélectionner un modèle ")
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let items: Vec<ListItem> = self
            .model_items
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let style = if i == self.model_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(Line::from(Span::styled(m.as_str(), style)))
            })
            .collect();

        let list = List::new(items).block(block).highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let mut state = ListState::default();
        state.select(Some(self.model_selected));
        f.render_stateful_widget(list, area, &mut state);
    }

    fn draw_provider_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(50, 30, size);
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Sélectionner un provider ")
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let items: Vec<ListItem> = self
            .provider_items
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let style = if i == self.provider_selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(Line::from(Span::styled(p.label(), style)))
            })
            .collect();

        let list = List::new(items).block(block).highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let mut state = ListState::default();
        state.select(Some(self.provider_selected));
        f.render_stateful_widget(list, area, &mut state);
    }

    fn draw_help_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(70, 70, size);
        f.render_widget(Clear, area);

        let help_text = vec![
            Line::from(Span::styled(
                " SoulSystem REPL — Aide",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                " Commandes:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw("   ask <msg>           Poser une question au LLM"),
            Line::raw("   plan <objectif>     Créer un plan"),
            Line::raw("   run <cmd>           Exécuter une commande shell"),
            Line::raw("   sandbox history     Historique sandbox"),
            Line::raw("   sandbox scan <cmd>  Vérifier une commande"),
            Line::raw("   sandbox policy      Politique sandbox"),
            Line::raw("   tools               Lister les outils"),
            Line::raw("   memory              Mémoire de travail"),
            Line::raw("   memory-browse [k]   Mémoire persistante"),
            Line::raw("   observe <msg>       Enregistrer observation"),
            Line::raw("   decide <ctx>        Prendre décision"),
            Line::raw("   history             Historique actions"),
            Line::raw("   models              Lister modèles"),
            Line::raw("   status              État système"),
            Line::raw("   help                Cette aide"),
            Line::raw("   exit                Quitter"),
            Line::raw(""),
            Line::from(Span::styled(
                " Raccourcis clavier:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw("   Ctrl+C              Quitter"),
            Line::raw("   Ctrl+P              Changer provider"),
            Line::raw("   Ctrl+M              Changer modèle"),
            Line::raw("   Ctrl+B              Basculer barre latérale"),
            Line::raw("   Ctrl+H              Aide"),
            Line::raw("   Ctrl+A / Ctrl+E     Début / Fin de ligne"),
            Line::raw("   Ctrl+U              Effacer ligne"),
            Line::raw("   Ctrl+W              Effacer mot"),
            Line::raw("   Esc                 Fermer dialog / Mode navigation"),
            Line::raw("   i / Enter           Mode saisie"),
            Line::raw(""),
            Line::from(Span::styled(
                " Appuyez sur Esc pour fermer",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Aide [Esc pour fermer] ")
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let paragraph = Paragraph::new(help_text)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(paragraph, area);
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_calculation() {
        let rect = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(50, 40, rect);
        assert!(centered.width > 0);
        assert!(centered.height > 0);
    }

    #[test]
    fn provider_opt_roundtrip() {
        let opts = [ProviderOpt::Ollama, ProviderOpt::OpenAI, ProviderOpt::Anthropic];
        for opt in &opts {
            let kind = opt.to_kind();
            let back = ProviderOpt::from_kind(&kind);
            assert_eq!(*opt, back);
        }
    }
}
