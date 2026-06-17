use anyhow::{Context, Result};
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
use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

const CONFIG_DIR: &str = "soulsystem";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmPersistConfig {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
}

impl Default for LlmPersistConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".into(),
            base_url: "http://127.0.0.1:11434".into(),
            model: "qwen3:8b".into(),
            api_key: None,
            temperature: 0.7,
            max_tokens: 2048,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersistConfig {
    pub llm: LlmPersistConfig,
    pub entity_name: String,
    pub gateway_addr: String,
    pub autonomous: bool,
}

impl Default for PersistConfig {
    fn default() -> Self {
        Self {
            llm: LlmPersistConfig::default(),
            entity_name: "soul".into(),
            gateway_addr: "127.0.0.1:7878".into(),
            autonomous: false,
        }
    }
}

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("impossible de déterminer le répertoire de config")?
        .join(CONFIG_DIR);
    Ok(dir.join(CONFIG_FILE))
}

pub fn load_config() -> PersistConfig {
    let path = match config_path() {
        Ok(p) => p,
        Err(_) => return PersistConfig::default(),
    };
    match fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_default(),
        Err(_) => PersistConfig::default(),
    }
}

pub fn save_config(cfg: &PersistConfig) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("impossible de créer {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(cfg).context("erreur sérialisation TOML")?;
    fs::write(&path, &content)
        .with_context(|| format!("impossible d'écrire {}", path.display()))?;
    // Sécurité: restreindre les permissions du fichier contenant la clé API
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

// ─── TUI Config App ────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum Focus {
    SettingsList,
    EditDialog,
    ProviderDialog,
    ModelDialog,
    ConfirmSave,
    ConfirmQuit,
}

#[derive(Clone, PartialEq)]
enum SettingField {
    Provider,
    Url,
    Model,
    ApiKey,
    Temperature,
    MaxTokens,
    EntityName,
    Gateway,
}

struct ConfigApp {
    cfg: PersistConfig,
    focus: Focus,
    selected: usize,
    edit_buffer: String,
    edit_cursor: usize,
    edit_field: Option<SettingField>,
    provider_selected: usize,
    model_selected: usize,
    model_items: Vec<String>,
    dirty: bool,
    status_msg: String,
    should_quit: bool,
}

impl ConfigApp {
    fn new(existing: &PersistConfig) -> Self {
        let model_items = suggested_models(&existing.llm.provider);
        Self {
            cfg: existing.clone(),
            focus: Focus::SettingsList,
            selected: 0,
            edit_buffer: String::new(),
            edit_cursor: 0,
            edit_field: None,
            provider_selected: provider_index(&existing.llm.provider),
            model_selected: 0,
            model_items,
            dirty: false,
            status_msg: String::new(),
            should_quit: false,
        }
    }

    fn settings_count() -> usize {
        9
    }

    fn setting_label(idx: usize) -> &'static str {
        match idx {
            0 => "Provider",
            1 => "URL du serveur",
            2 => "Modèle",
            3 => "Clé API",
            4 => "Température",
            5 => "Max tokens",
            6 => "Nom entité",
            7 => "Adresse gateway",
            8 => "Mode autonome",
            _ => "",
        }
    }

    fn setting_value(&self, idx: usize) -> String {
        match idx {
            0 => self.cfg.llm.provider.clone(),
            1 => self.cfg.llm.base_url.clone(),
            2 => self.cfg.llm.model.clone(),
            3 => mask_api_key(&self.cfg.llm.api_key),
            4 => format!("{:.1}", self.cfg.llm.temperature),
            5 => self.cfg.llm.max_tokens.to_string(),
            6 => self.cfg.entity_name.clone(),
            7 => self.cfg.gateway_addr.clone(),
            8 => if self.cfg.autonomous {
                "activé"
            } else {
                "désactivé"
            }
            .into(),
            _ => String::new(),
        }
    }

    fn start_edit(&mut self, idx: usize) {
        let field = match idx {
            0 => SettingField::Provider,
            1 => SettingField::Url,
            2 => SettingField::Model,
            3 => SettingField::ApiKey,
            4 => SettingField::Temperature,
            5 => SettingField::MaxTokens,
            6 => SettingField::EntityName,
            7 => SettingField::Gateway,
            8 => {
                self.cfg.autonomous = !self.cfg.autonomous;
                self.dirty = true;
                return;
            }
            _ => return,
        };

        match &field {
            SettingField::Provider => {
                self.provider_selected = provider_index(&self.cfg.llm.provider);
                self.focus = Focus::ProviderDialog;
                return;
            }
            SettingField::Model => {
                self.model_items = suggested_models(&self.cfg.llm.provider);
                self.model_selected = self
                    .model_items
                    .iter()
                    .position(|m| m == &self.cfg.llm.model)
                    .unwrap_or(0);
                self.focus = Focus::ModelDialog;
                return;
            }
            _ => {}
        }

        let current = match &field {
            SettingField::ApiKey => self.cfg.llm.api_key.clone().unwrap_or_default(),
            SettingField::Temperature => format!("{:.1}", self.cfg.llm.temperature),
            SettingField::MaxTokens => self.cfg.llm.max_tokens.to_string(),
            SettingField::Url => self.cfg.llm.base_url.clone(),
            SettingField::EntityName => self.cfg.entity_name.clone(),
            SettingField::Gateway => self.cfg.gateway_addr.clone(),
            _ => String::new(),
        };

        self.edit_buffer = current;
        self.edit_cursor = self.edit_buffer.len();
        self.edit_field = Some(field);
        self.focus = Focus::EditDialog;
    }

    fn apply_edit(&mut self) {
        if let Some(ref field) = self.edit_field.clone() {
            match field {
                SettingField::Url => self.cfg.llm.base_url = self.edit_buffer.clone(),
                SettingField::ApiKey => {
                    self.cfg.llm.api_key = if self.edit_buffer.is_empty() {
                        None
                    } else {
                        Some(self.edit_buffer.clone())
                    }
                }
                SettingField::Temperature => {
                    if let Ok(t) = self.edit_buffer.parse::<f32>() {
                        self.cfg.llm.temperature = t.clamp(0.0, 2.0);
                    }
                }
                SettingField::MaxTokens => {
                    if let Ok(t) = self.edit_buffer.parse::<usize>() {
                        self.cfg.llm.max_tokens = t.clamp(1, 128_000);
                    }
                }
                SettingField::EntityName => self.cfg.entity_name = self.edit_buffer.clone(),
                SettingField::Gateway => self.cfg.gateway_addr = self.edit_buffer.clone(),
                _ => {}
            }
            self.dirty = true;
        }
        self.focus = Focus::SettingsList;
        self.edit_field = None;
    }

    fn apply_provider(&mut self) {
        let providers = ["ollama", "openai", "anthropic"];
        if self.provider_selected < providers.len() {
            let new_provider = providers[self.provider_selected].to_string();
            self.cfg.llm.base_url = default_url(&new_provider);
            self.cfg.llm.provider = new_provider;
            self.model_items = suggested_models(&self.cfg.llm.provider);
            self.model_selected = 0;
            self.dirty = true;
        }
        self.focus = Focus::SettingsList;
    }

    fn apply_model(&mut self) {
        if self.model_selected < self.model_items.len() {
            self.cfg.llm.model = self.model_items[self.model_selected].clone();
            self.dirty = true;
        }
        self.focus = Focus::SettingsList;
    }
}

// ─── TUI Entry Point ───────────────────────────────────────────────────

pub fn run_config_menu(existing: &PersistConfig) -> Result<PersistConfig> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = ConfigApp::new(existing);

    let result = loop {
        terminal.draw(|f| draw_ui(f, &mut app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(&mut app, key) {
                    break app.cfg.clone();
                }
            }
        }

        if app.should_quit {
            break app.cfg.clone();
        }
    };

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if app.dirty {
        save_config(&app.cfg)?;
    }
    Ok(result)
}

// ─── Event Handling ────────────────────────────────────────────────────

fn handle_key(app: &mut ConfigApp, key: KeyEvent) -> bool {
    match app.focus {
        Focus::SettingsList => handle_settings_key(app, key),
        Focus::EditDialog => handle_edit_key(app, key),
        Focus::ProviderDialog => handle_provider_key(app, key),
        Focus::ModelDialog => handle_model_key(app, key),
        Focus::ConfirmSave => handle_confirm_save_key(app, key),
        Focus::ConfirmQuit => handle_confirm_quit_key(app, key),
    }
}

fn handle_settings_key(app: &mut ConfigApp, key: KeyEvent) -> bool {
    // Ctrl+C: quit
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        if app.dirty {
            app.focus = Focus::ConfirmQuit;
        } else {
            return true;
        }
        return false;
    }

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.selected = app.selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = ConfigApp::settings_count() - 1;
            app.selected = app.selected.min(max);
        }
        KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('e') => {
            app.start_edit(app.selected);
        }
        KeyCode::Char('q') => {
            if app.dirty {
                app.focus = Focus::ConfirmQuit;
            } else {
                return true;
            }
        }
        KeyCode::Char('s') if app.dirty => {
            app.focus = Focus::ConfirmSave;
        }
        _ => {}
    }
    false
}

fn handle_edit_key(app: &mut ConfigApp, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.focus = Focus::SettingsList;
            app.edit_field = None;
        }
        KeyCode::Enter => {
            app.apply_edit();
        }
        KeyCode::Char(c) => {
            app.edit_buffer.insert(app.edit_cursor, c);
            app.edit_cursor += 1;
        }
        KeyCode::Backspace => {
            if app.edit_cursor > 0 {
                app.edit_cursor -= 1;
                app.edit_buffer.remove(app.edit_cursor);
            }
        }
        KeyCode::Delete => {
            if app.edit_cursor < app.edit_buffer.len() {
                app.edit_buffer.remove(app.edit_cursor);
            }
        }
        KeyCode::Left => {
            app.edit_cursor = app.edit_cursor.saturating_sub(1);
        }
        KeyCode::Right => {
            if app.edit_cursor < app.edit_buffer.len() {
                app.edit_cursor += 1;
            }
        }
        KeyCode::Home => app.edit_cursor = 0,
        KeyCode::End => app.edit_cursor = app.edit_buffer.len(),
        _ => {}
    }
    false
}

fn handle_provider_key(app: &mut ConfigApp, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => app.focus = Focus::SettingsList,
        KeyCode::Up | KeyCode::Char('k') => {
            app.provider_selected = app.provider_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.provider_selected = app.provider_selected.min(2);
        }
        KeyCode::Enter => app.apply_provider(),
        _ => {}
    }
    false
}

fn handle_model_key(app: &mut ConfigApp, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => app.focus = Focus::SettingsList,
        KeyCode::Up | KeyCode::Char('k') => {
            app.model_selected = app.model_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app.model_items.len().saturating_sub(1);
            app.model_selected = app.model_selected.min(max);
        }
        KeyCode::Enter => app.apply_model(),
        _ => {}
    }
    false
}

fn handle_confirm_save_key(app: &mut ConfigApp, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            let _ = save_config(&app.cfg);
            app.dirty = false;
            app.status_msg = "Sauvegardé".into();
            app.focus = Focus::SettingsList;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.focus = Focus::SettingsList;
        }
        _ => {}
    }
    false
}

fn handle_confirm_quit_key(app: &mut ConfigApp, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            if app.dirty {
                let _ = save_config(&app.cfg);
            }
            return true;
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            app.focus = Focus::SettingsList;
        }
        _ => {}
    }
    false
}

// ─── Drawing ───────────────────────────────────────────────────────────

fn draw_ui(f: &mut Frame, app: &mut ConfigApp) {
    let size = f.area();

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(size);

    draw_header(f, vertical[0], app);
    draw_body(f, vertical[1], app);
    draw_status_bar(f, vertical[2], app);

    match app.focus {
        Focus::EditDialog => draw_edit_dialog(f, size, app),
        Focus::ProviderDialog => draw_provider_dialog(f, size, app),
        Focus::ModelDialog => draw_model_dialog(f, size, app),
        Focus::ConfirmSave => draw_confirm_dialog(f, size, "Sauvegarder la configuration ?", "y/n"),
        Focus::ConfirmQuit => draw_confirm_dialog(
            f,
            size,
            "Quitter sans sauvegarder ?",
            "y sauvegarder / n annuler",
        ),
        _ => {}
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &ConfigApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" SoulSystem Configuration ")
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let title = Line::from(vec![
        Span::styled(
            " SoulSystem ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "Configuration",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  │  "),
        Span::styled(
            if app.dirty {
                "● non sauvegardé"
            } else {
                "○ sauvegardé"
            },
            Style::default().fg(if app.dirty {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        ),
    ]);

    let paragraph = Paragraph::new(title)
        .block(block)
        .style(Style::default().bg(Color::Black));
    f.render_widget(paragraph, area);
}

fn draw_body(f: &mut Frame, area: Rect, app: &mut ConfigApp) {
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_settings_list(f, horizontal[0], app);
    draw_preview(f, horizontal[1], app);
}

fn draw_settings_list(f: &mut Frame, area: Rect, app: &mut ConfigApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Paramètres ")
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(Color::Black));

    let items: Vec<ListItem> = (0..ConfigApp::settings_count())
        .map(|i| {
            let label = ConfigApp::setting_label(i);
            let value = app.setting_value(i);
            let style = if i == app.selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("  {:16}", label), style),
                Span::styled(
                    if i == app.selected { "▸ " } else { "  " },
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    value,
                    if i == app.selected {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_preview(f: &mut Frame, area: Rect, app: &ConfigApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Aperçu TOML ")
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(Color::Black));

    let toml_str = toml::to_string_pretty(&app.cfg).unwrap_or_default();

    let lines: Vec<Line> = toml_str
        .lines()
        .map(|line| {
            if line.starts_with('[') {
                Line::from(Span::styled(
                    line,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if line.contains('=') {
                let parts: Vec<&str> = line.splitn(2, '=').collect();
                if parts.len() == 2 {
                    Line::from(vec![
                        Span::styled(format!("{}=", parts[0]), Style::default().fg(Color::Yellow)),
                        Span::styled(parts[1].trim(), Style::default().fg(Color::Green)),
                    ])
                } else {
                    Line::from(Span::styled(line, Style::default().fg(Color::White)))
                }
            } else {
                Line::from(Span::styled(line, Style::default().fg(Color::DarkGray)))
            }
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .style(Style::default().bg(Color::Black));
    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, area: Rect, app: &ConfigApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(Color::Black));

    let status = if !app.status_msg.is_empty() {
        Line::from(vec![
            Span::styled(
                " ✓ ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(&app.status_msg, Style::default().fg(Color::Green)),
        ])
    } else {
        Line::from(vec![
            Span::styled(
                " Enter ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("éditer  "),
            Span::styled("j/k ", Style::default().fg(Color::Cyan)),
            Span::raw("naviguer  "),
            Span::styled("s ", Style::default().fg(Color::Cyan)),
            Span::raw("sauvegarder  "),
            Span::styled("q ", Style::default().fg(Color::Cyan)),
            Span::raw("quitter  "),
            Span::styled("Ctrl+C ", Style::default().fg(Color::Cyan)),
            Span::raw("abandonner"),
        ])
    };

    let paragraph = Paragraph::new(status)
        .block(block)
        .style(Style::default().bg(Color::Black));
    f.render_widget(paragraph, area);
}

fn draw_edit_dialog(f: &mut Frame, size: Rect, app: &ConfigApp) {
    let area = centered_rect(60, 25, size);
    f.render_widget(Clear, area);

    let label = match &app.edit_field {
        Some(SettingField::Url) => "URL du serveur LLM",
        Some(SettingField::ApiKey) => "Clé API (vide pour supprimer)",
        Some(SettingField::Temperature) => "Température (0.0 — 2.0)",
        Some(SettingField::MaxTokens) => "Max tokens (1 — 128000)",
        Some(SettingField::EntityName) => "Nom de l'entité",
        Some(SettingField::Gateway) => "Adresse du gateway (host:port)",
        _ => "",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", label))
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let display = if app.edit_buffer.is_empty() {
        Span::styled("│", Style::default().fg(Color::Cyan))
    } else {
        Span::styled(&app.edit_buffer, Style::default().fg(Color::White))
    };

    let paragraph = Paragraph::new(Line::from(display));
    f.render_widget(paragraph, inner);

    if !app.edit_buffer.is_empty() || app.focus == Focus::EditDialog {
        let cursor_x = inner.x + app.edit_cursor as u16;
        f.set_cursor_position((cursor_x, inner.y));
    }

    let hint_area = Rect {
        x: inner.x,
        y: inner.y + 2,
        width: inner.width,
        height: 1,
    };
    let hint = Paragraph::new(Line::from(vec![
        Span::styled("Enter ", Style::default().fg(Color::Cyan)),
        Span::raw("valider  "),
        Span::styled("Esc ", Style::default().fg(Color::Cyan)),
        Span::raw("annuler"),
    ]));
    f.render_widget(hint, hint_area);
}

fn draw_provider_dialog(f: &mut Frame, size: Rect, app: &mut ConfigApp) {
    let area = centered_rect(40, 35, size);
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Sélectionner un provider ")
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let providers = ["ollama", "openai", "anthropic"];
    let urls = [
        "http://127.0.0.1:11434",
        "https://api.openai.com",
        "https://api.anthropic.com",
    ];

    let items: Vec<ListItem> = providers
        .iter()
        .zip(urls.iter())
        .enumerate()
        .map(|(i, (name, url))| {
            let style = if i == app.provider_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(vec![
                Line::from(Span::styled(format!("  {}", name), style)),
                Line::from(Span::styled(
                    format!("    {}", url),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    state.select(Some(app.provider_selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_model_dialog(f: &mut Frame, size: Rect, app: &mut ConfigApp) {
    let area = centered_rect(50, 45, size);
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Modèles — {} ", app.cfg.llm.provider))
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let items: Vec<ListItem> = app
        .model_items
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let style = if i == app.model_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let current = if m == &app.cfg.llm.model {
                "  (actuel)"
            } else {
                ""
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("  {}", m), style),
                Span::styled(current, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    state.select(Some(app.model_selected));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_confirm_dialog(f: &mut Frame, size: Rect, message: &str, hint: &str) {
    let area = centered_rect(50, 20, size);
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirmation ")
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            format!("  {}", message),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            format!("  {}", hint),
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines).style(Style::default().bg(Color::Black));
    f.render_widget(paragraph, inner);
}

// ─── Helpers ───────────────────────────────────────────────────────────

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

fn mask_api_key(key: &Option<String>) -> String {
    match key {
        Some(k) if k.len() > 8 => format!("{}...{}", &k[..4], &k[k.len() - 4..]),
        Some(k) => "*".repeat(k.len().min(8)),
        None => "non définie".into(),
    }
}

fn provider_index(provider: &str) -> usize {
    match provider {
        "ollama" => 0,
        "openai" => 1,
        "anthropic" => 2,
        _ => 0,
    }
}

fn default_url(provider: &str) -> String {
    match provider {
        "ollama" => "http://127.0.0.1:11434".into(),
        "openai" => "https://api.openai.com".into(),
        "anthropic" => "https://api.anthropic.com".into(),
        _ => "http://127.0.0.1:11434".into(),
    }
}

fn suggested_models(provider: &str) -> Vec<String> {
    match provider {
        "ollama" => vec![
            "qwen3:8b".into(),
            "llama3.1:8b".into(),
            "mistral:latest".into(),
            "deepseek-r1:8b".into(),
        ],
        "openai" => vec![
            "gpt-4o".into(),
            "gpt-4o-mini".into(),
            "gpt-4.1".into(),
            "o4-mini".into(),
        ],
        "anthropic" => vec![
            "claude-sonnet-4-20250514".into(),
            "claude-haiku-4-20250414".into(),
            "claude-opus-4-20250514".into(),
        ],
        _ => vec![],
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = PersistConfig::default();
        assert_eq!(cfg.llm.provider, "ollama");
        assert_eq!(cfg.llm.temperature, 0.7_f32);
        assert!(!cfg.autonomous);
    }

    #[test]
    fn serialize_roundtrip() {
        let cfg = PersistConfig::default();
        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let parsed: PersistConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.llm.provider, cfg.llm.provider);
        assert_eq!(parsed.llm.model, cfg.llm.model);
    }

    #[test]
    fn mask_api_key_works() {
        assert_eq!(mask_api_key(&None), "non définie");
        assert_eq!(mask_api_key(&Some("short".into())), "*****");
        assert_eq!(
            mask_api_key(&Some("sk-1234567890abcdef".into())),
            "sk-1...cdef"
        );
    }

    #[test]
    fn provider_index_works() {
        assert_eq!(provider_index("ollama"), 0);
        assert_eq!(provider_index("openai"), 1);
        assert_eq!(provider_index("anthropic"), 2);
        assert_eq!(provider_index("unknown"), 0);
    }

    #[test]
    fn centered_rect_calculation() {
        let rect = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(50, 40, rect);
        assert!(centered.width > 0);
        assert!(centered.height > 0);
    }
}
