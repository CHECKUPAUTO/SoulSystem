//! # soul_repl — TUI interactif pour l'entité autonome

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use soul_llm::{LlmClient, LlmConfig, ProviderKind};
use soul_persistence::LongTermMemory;
use soul_planner::{CognitiveLoop, Goal, GoalStatus};
use soul_sandbox::Sandbox;
use soul_tools::ToolRegistry;

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

pub async fn run_repl(state: &mut ReplState) {
    enable_raw_mode().expect("failed to enable raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).expect("failed to enter alternate screen");
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).expect("failed to create terminal");

    let (tx, mut rx) = mpsc::unbounded_channel::<LlmEvent>();
    let mut app = App::new(state, tx);

    loop {
        terminal.draw(|f| app.draw(f)).expect("failed to draw");

        while let Ok(evt) = rx.try_recv() {
            match evt {
                LlmEvent::Response(text) => {
                    app.streaming = false;
                    app.add_message(Role::Assistant, &text);
                }
                LlmEvent::Error(e) => {
                    app.streaming = false;
                    app.add_message(Role::Error, &e);
                }
                LlmEvent::StreamChunk(chunk) => {
                    app.stream_buffer.push_str(&chunk);
                }
                LlmEvent::StreamDone => {
                    if !app.stream_buffer.is_empty() {
                        let text = app.stream_buffer.clone();
                        app.stream_buffer.clear();
                        app.streaming = false;
                        app.add_message(Role::Assistant, &text);
                    }
                }
            }
        }

        if event::poll(Duration::from_millis(16)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if app.handle_key(key, &state.llm).await {
                    break;
                }
            }
        }
    }

    disable_raw_mode().expect("failed to disable raw mode");
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .expect("failed to leave alternate screen");
    terminal.show_cursor().expect("failed to show cursor");
}

enum LlmEvent {
    Response(String),
    Error(String),
    StreamChunk(String),
    StreamDone,
}

#[derive(Clone, PartialEq)]
enum Focus {
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
}

struct Message {
    role: Role,
    text: String,
}

#[derive(Clone, PartialEq)]
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
    model_items: Vec<String>,
    model_selected: usize,
    provider_items: Vec<ProviderOpt>,
    provider_selected: usize,
    streaming: bool,
    stream_buffer: String,
    llm_tx: mpsc::UnboundedSender<LlmEvent>,
}

impl App {
    fn new(state: &ReplState, llm_tx: mpsc::UnboundedSender<LlmEvent>) -> Self {
        let provider = ProviderOpt::from_kind(&state.llm.config().provider);
        let model_items = vec![state.llm.config().model.clone()];
        let provider_items = vec![ProviderOpt::Ollama, ProviderOpt::OpenAI, ProviderOpt::Anthropic];
        let provider_selected = provider_items.iter().position(|p| *p == provider).unwrap_or(0);

        Self {
            messages: vec![Message {
                role: Role::System,
                text: "Bienvenue dans SoulSystem REPL.\nTapez votre message ou /help pour les commandes.\nCtrl+P provider, Ctrl+M modèle, Ctrl+H aide, Ctrl+C quitter".into(),
            }],
            input: String::new(),
            input_cursor: 0,
            focus: Focus::Input,
            scroll_offset: 0,
            sidebar_visible: false,
            model_items,
            model_selected: 0,
            provider_items,
            provider_selected,
            streaming: false,
            stream_buffer: String::new(),
            llm_tx,
        }
    }

    async fn handle_key(&mut self, key: KeyEvent, llm: &LlmClient) -> bool {
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
        }
    }

    async fn handle_input_key(&mut self, key: KeyEvent, llm: &LlmClient) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => return true,
                KeyCode::Char('p') => {
                    self.focus = Focus::ProviderDialog;
                    self.provider_selected = self.provider_items.iter().position(|p| *p == ProviderOpt::from_kind(&llm.config().provider)).unwrap_or(0);
                    return false;
                }
                KeyCode::Char('m') => {
                    self.focus = Focus::ModelDialog;
                    self.model_selected = 0;
                    return false;
                }
                KeyCode::Char('b') => { self.sidebar_visible = !self.sidebar_visible; return false; }
                KeyCode::Char('h') => { self.focus = Focus::HelpDialog; return false; }
                KeyCode::Char('a') => { self.input_cursor = 0; return false; }
                KeyCode::Char('e') => { self.input_cursor = self.input.len(); return false; }
                KeyCode::Char('u') => { self.input.clear(); self.input_cursor = 0; return false; }
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

        match key.code {
            KeyCode::Enter => {
                let input = self.input.trim().to_string();
                if input.is_empty() { return false; }
                self.input.clear();
                self.input_cursor = 0;
                if input == "exit" || input == "quit" { return true; }
                self.add_message(Role::User, &input);
                self.execute_command(&input, llm).await;
            }
            KeyCode::Char(c) => { self.input.insert(self.input_cursor, c); self.input_cursor += 1; }
            KeyCode::Backspace => { if self.input_cursor > 0 { self.input_cursor -= 1; self.input.remove(self.input_cursor); } }
            KeyCode::Delete => { if self.input_cursor < self.input.len() { self.input.remove(self.input_cursor); } }
            KeyCode::Left => { self.input_cursor = self.input_cursor.saturating_sub(1); }
            KeyCode::Right => { if self.input_cursor < self.input.len() { self.input_cursor += 1; } }
            KeyCode::Up => self.scroll_offset = self.scroll_offset.saturating_sub(1),
            KeyCode::Down => self.scroll_offset = self.scroll_offset.saturating_add(1),
            KeyCode::Home => self.input_cursor = 0,
            KeyCode::End => self.input_cursor = self.input.len(),
            _ => {}
        }
        false
    }

    fn handle_dialog_key(&mut self, key: KeyEvent, is_model: bool) -> bool {
        match key.code {
            KeyCode::Esc => self.focus = Focus::Input,
            KeyCode::Up | KeyCode::Char('k') => {
                if is_model { self.model_selected = self.model_selected.saturating_sub(1); }
                else { self.provider_selected = self.provider_selected.saturating_sub(1); }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if is_model { self.model_selected = self.model_selected.min(self.model_items.len().saturating_sub(1)); }
                else { self.provider_selected = self.provider_selected.min(self.provider_items.len().saturating_sub(1)); }
            }
            KeyCode::Enter => {
                if is_model {
                    let model = self.model_items[self.model_selected].clone();
                    self.add_message(Role::System, &format!("Modèle changé: {}", model));
                } else {
                    let provider = self.provider_items[self.provider_selected].clone();
                    self.add_message(Role::System, &format!("Provider changé: {}", provider.label()));
                }
                self.focus = Focus::Input;
            }
            _ => {}
        }
        false
    }

    fn add_message(&mut self, role: Role, text: &str) {
        self.messages.push(Message { role, text: text.to_string() });
        let total_lines: usize = self.messages.iter().map(|m| m.text.lines().count().max(1)).sum();
        if total_lines > 20 {
            self.scroll_offset = (total_lines - 20) as u16;
        }
    }

    async fn execute_command(&mut self, input: &str, llm: &LlmClient) {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0];
        let arg = parts.get(1).map(|s| s.trim()).unwrap_or("");

        match cmd {
            "ask" | "/ask" => self.cmd_ask(arg, llm).await,
            "help" | "/help" => self.focus = Focus::HelpDialog,
            "models" | "/models" => {
                match llm.list_models().await {
                    Ok(models) => {
                        let list = models.iter().map(|m| format!("  {}", m.name)).collect::<Vec<_>>().join("\n");
                        self.add_message(Role::System, &list);
                    }
                    Err(e) => self.add_message(Role::Error, &e.to_string()),
                }
            }
            "status" | "/status" => {
                let alive = llm.is_alive().await;
                self.add_message(Role::System, &format!("Provider: {} | LLM: {}", llm.config().provider, if alive { "connecté" } else { "déconnecté" }));
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

        let prompt = msg.to_string();
        let llm_clone = llm.clone();
        let tx = self.llm_tx.clone();

        tokio::spawn(async move {
            match llm_clone.generate(&prompt).await {
                Ok(resp) => { let _ = tx.send(LlmEvent::Response(resp.text)); }
                Err(e) => { let _ = tx.send(LlmEvent::Error(format!("Erreur LLM: {}", e))); }
            }
        });
    }

    fn draw(&mut self, f: &mut Frame) {
        let size = f.area();

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5), Constraint::Length(3)])
            .split(size);

        self.draw_status_bar(f, vertical[0]);
        self.draw_chat(f, vertical[1]);
        self.draw_input(f, vertical[2]);

        match self.focus {
            Focus::ModelDialog => self.draw_model_dialog(f, size),
            Focus::ProviderDialog => self.draw_provider_dialog(f, size),
            Focus::HelpDialog => self.draw_help_dialog(f, size),
            _ => {}
        }
    }

    fn draw_status_bar(&self, f: &mut Frame, area: Rect) {
        let provider_label = self.provider_items.get(self.provider_selected).map(|p| p.label()).unwrap_or("?");
        let status_line = Line::from(vec![
            Span::styled(" SoulSystem ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {} ", provider_label), Style::default().fg(Color::White).bg(Color::DarkGray)),
            Span::styled(format!(" {} ", self.model_items.first().unwrap_or(&String::new())), Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(format!("{} msgs", self.messages.len()), Style::default().fg(Color::DarkGray)),
            Span::raw("  "),
            Span::styled(if self.streaming { "⏳" } else { "✓" }, Style::default().fg(if self.streaming { Color::Yellow } else { Color::Green })),
        ]);
        let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)).style(Style::default().bg(Color::Black));
        f.render_widget(Paragraph::new(status_line).block(block).style(Style::default().bg(Color::Black)), area);
    }

    fn draw_chat(&self, f: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Chat ").border_style(Style::default().fg(Color::DarkGray)).style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();
        for msg in &self.messages {
            let (prefix, style) = match msg.role {
                Role::User => ("you", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Role::Assistant => ("ai", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Role::System => ("sys", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Role::Error => ("err", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", prefix), style),
                Span::styled(&msg.text, Style::default().fg(Color::White)),
            ]));
            lines.push(Line::raw(""));
        }

        if self.streaming && !self.stream_buffer.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("ai ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(&self.stream_buffer, Style::default().fg(Color::White)),
            ]));
        }

        let paragraph = Paragraph::new(lines).scroll((self.scroll_offset, 0)).wrap(Wrap { trim: false });
        f.render_widget(paragraph, inner);
    }

    fn draw_input(&self, f: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Input;
        let block = Block::default().borders(Borders::ALL).title(" Input ").border_style(Style::default().fg(if focused { Color::Cyan } else { Color::DarkGray })).style(Style::default().bg(Color::Black));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let display = if self.input.is_empty() && focused {
            Span::styled("Tapez votre message...", Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(&self.input, Style::default().fg(Color::White))
        };

        f.render_widget(Paragraph::new(Line::from(display)), inner);

        if focused {
            f.set_cursor_position((inner.x + self.input_cursor as u16, inner.y));
        }
    }

    fn draw_model_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(50, 40, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Sélectionner un modèle ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let items: Vec<ListItem> = self.model_items.iter().enumerate().map(|(i, m)| {
            let style = if i == self.model_selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(Span::styled(m.as_str(), style)))
        }).collect();
        let list = List::new(items).block(block).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let mut state = ListState::default();
        state.select(Some(self.model_selected));
        f.render_stateful_widget(list, area, &mut state);
    }

    fn draw_provider_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(50, 30, size);
        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Sélectionner un provider ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        let items: Vec<ListItem> = self.provider_items.iter().enumerate().map(|(i, p)| {
            let style = if i == self.provider_selected { Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::White) };
            ListItem::new(Line::from(Span::styled(p.label(), style)))
        }).collect();
        let list = List::new(items).block(block).highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let mut state = ListState::default();
        state.select(Some(self.provider_selected));
        f.render_stateful_widget(list, area, &mut state);
    }

    fn draw_help_dialog(&self, f: &mut Frame, size: Rect) {
        let area = centered_rect(70, 70, size);
        f.render_widget(Clear, area);
        let help_text = vec![
            Line::from(Span::styled(" SoulSystem REPL — Aide", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
            Line::raw(""),
            Line::from(Span::styled(" Commandes:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::raw("   ask <msg>        Poser une question au LLM"),
            Line::raw("   plan <objectif>  Créer un plan"),
            Line::raw("   run <cmd>        Exécuter une commande"),
            Line::raw("   models           Lister les modèles"),
            Line::raw("   status           État du système"),
            Line::raw("   help             Cette aide"),
            Line::raw("   exit             Quitter"),
            Line::raw(""),
            Line::from(Span::styled(" Raccourcis:", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::raw("   Ctrl+C           Quitter"),
            Line::raw("   Ctrl+P           Changer provider"),
            Line::raw("   Ctrl+M           Changer modèle"),
            Line::raw("   Ctrl+H           Aide"),
            Line::raw("   Esc              Fermer dialog"),
            Line::raw(""),
            Line::from(Span::styled(" Esc pour fermer", Style::default().fg(Color::DarkGray))),
        ];
        let block = Block::default().borders(Borders::ALL).title(" Aide ").border_style(Style::default().fg(Color::Cyan)).style(Style::default().bg(Color::Black));
        f.render_widget(Paragraph::new(help_text).block(block).wrap(Wrap { trim: false }), area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage((100 - percent_y) / 2), Constraint::Percentage(percent_y), Constraint::Percentage((100 - percent_y) / 2)]).split(r);
    Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage((100 - percent_x) / 2), Constraint::Percentage(percent_x), Constraint::Percentage((100 - percent_x) / 2)]).split(popup_layout[1])[1]
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
}
