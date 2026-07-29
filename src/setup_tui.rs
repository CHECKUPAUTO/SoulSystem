//! # Setup TUI
//!
//! Interactive terminal user interface for `soulsystem --setup-tui`.
//! Uses `ratatui` + `crossterm`.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::backend::Backend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;
use ratatui::Terminal;
use souls::config::PersistConfig;

#[derive(Clone, Copy, PartialEq)]
enum Field {
    Provider,
    Url,
    Model,
    ApiKey,
    EntityName,
    GatewayAddr,
    Save,
    Cancel,
}

struct App {
    cfg: PersistConfig,
    focus: Field,
    provider_idx: usize,
    providers: Vec<(&'static str, &'static str, &'static str, &'static str)>,
    temp_api_key: String,
    saved: bool,
}

impl App {
    fn new() -> Self {
        let providers = vec![
            ("ollama", "Ollama", "http://127.0.0.1:11434", "qwen3:8b"),
            (
                "openai",
                "OpenAI",
                "https://api.openai.com/v1",
                "gpt-4o-mini",
            ),
            (
                "anthropic",
                "Anthropic",
                "https://api.anthropic.com/v1",
                "claude-3-5-haiku-latest",
            ),
        ];
        let mut cfg = PersistConfig::default();
        let provider_idx = providers
            .iter()
            .position(|(id, _, _, _)| *id == cfg.llm.provider)
            .unwrap_or(0);
        cfg.llm.base_url = providers[provider_idx].2.to_string();
        cfg.llm.model = providers[provider_idx].3.to_string();
        Self {
            cfg,
            focus: Field::Provider,
            provider_idx,
            providers,
            temp_api_key: String::new(),
            saved: false,
        }
    }

    fn selected_provider_id(&self) -> &str {
        self.providers[self.provider_idx].0
    }

    fn apply_provider_defaults(&mut self) {
        let (_, _, url, model) = self.providers[self.provider_idx];
        self.cfg.llm.provider = self.selected_provider_id().to_string();
        self.cfg.llm.base_url = url.to_string();
        self.cfg.llm.model = model.to_string();
        if self.selected_provider_id() == "ollama" {
            self.cfg.llm.api_key = None;
            self.temp_api_key.clear();
        }
    }

    fn mask_key(&self) -> String {
        if self.temp_api_key.is_empty() {
            return "(none)".into();
        }
        "configured in system credential store".into()
    }

    fn next_field(&mut self) {
        self.focus = match self.focus {
            Field::Provider => Field::Url,
            Field::Url => Field::Model,
            Field::Model => Field::ApiKey,
            Field::ApiKey => Field::EntityName,
            Field::EntityName => Field::GatewayAddr,
            Field::GatewayAddr => Field::Save,
            Field::Save => Field::Cancel,
            Field::Cancel => Field::Provider,
        }
    }

    fn prev_field(&mut self) {
        self.focus = match self.focus {
            Field::Provider => Field::Cancel,
            Field::Url => Field::Provider,
            Field::Model => Field::Url,
            Field::ApiKey => Field::Model,
            Field::EntityName => Field::ApiKey,
            Field::GatewayAddr => Field::EntityName,
            Field::Save => Field::GatewayAddr,
            Field::Cancel => Field::Save,
        }
    }
}

pub fn run_tui_setup() -> Result<()> {
    enable_raw_mode()?;
    let mut terminal = Terminal::new(ratatui::backend::CrosstermBackend::new(std::io::stderr()))?;
    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);
    disable_raw_mode()?;
    terminal.show_cursor()?;
    if let Err(e) = res {
        eprintln!("TUI error: {e}");
    }
    if app.saved {
        souls::config::save_config(&app.cfg)?;
        println!("OK Configuration saved.");
        println!("  Run: soulsystem --entity --repl");
    }
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()>
where
    <B as Backend>::Error: Send + Sync + 'static,
{
    loop {
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Tab | KeyCode::Down => app.next_field(),
                KeyCode::BackTab | KeyCode::Up => app.prev_field(),
                KeyCode::Enter => {
                    if app.focus == Field::Save {
                        app.cfg.llm.api_key = if app.temp_api_key.is_empty() {
                            None
                        } else {
                            Some(soulsystem_common::secrets::SecretString::new(
                                app.temp_api_key.clone(),
                            ))
                        };
                        app.saved = true;
                        return Ok(());
                    } else if app.focus == Field::Cancel {
                        return Ok(());
                    }
                }
                KeyCode::Right | KeyCode::Char('l') if app.focus == Field::Provider => {
                    if app.provider_idx + 1 < app.providers.len() {
                        app.provider_idx += 1;
                        app.apply_provider_defaults();
                    }
                }
                KeyCode::Left | KeyCode::Char('h') if app.focus == Field::Provider => {
                    if app.provider_idx > 0 {
                        app.provider_idx -= 1;
                        app.apply_provider_defaults();
                    }
                }
                KeyCode::Char(c) => match app.focus {
                    Field::Url => app.cfg.llm.base_url.push(c),
                    Field::Model => app.cfg.llm.model.push(c),
                    Field::ApiKey => app.temp_api_key.push(c),
                    Field::EntityName => app.cfg.entity_name.push(c),
                    Field::GatewayAddr => app.cfg.gateway_addr.push(c),
                    _ => {}
                },
                KeyCode::Backspace => match app.focus {
                    Field::Url => {
                        app.cfg.llm.base_url.pop();
                    }
                    Field::Model => {
                        app.cfg.llm.model.pop();
                    }
                    Field::ApiKey => {
                        app.temp_api_key.pop();
                    }
                    Field::EntityName => {
                        app.cfg.entity_name.pop();
                    }
                    Field::GatewayAddr => {
                        app.cfg.gateway_addr.pop();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    let title = Paragraph::new("SoulSystem — Setup Wizard")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    let body = build_form(app);
    f.render_widget(body, chunks[1]);

    let help = Paragraph::new(
        "Tab/Down: next field | Shift+Tab/Up: prev field | Enter: save/cancel | ←→: change provider | Esc/q: quit",
    )
    .style(Style::default().fg(Color::Gray))
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true });
    f.render_widget(help, chunks[2]);
}

fn build_form(app: &App) -> Paragraph<'_> {
    let provider_name = app.providers[app.provider_idx].1;
    let mut lines = vec![
        Line::from(""),
        form_line(
            "Provider   ",
            app.focus == Field::Provider,
            format!("  ◀ {} ▶", provider_name),
        ),
        Line::from(""),
        form_line(
            "Base URL   ",
            app.focus == Field::Url,
            app.cfg.llm.base_url.clone(),
        ),
        Line::from(""),
        form_line(
            "Model      ",
            app.focus == Field::Model,
            app.cfg.llm.model.clone(),
        ),
        Line::from(""),
        form_line("API key    ", app.focus == Field::ApiKey, app.mask_key()),
        Line::from(""),
        form_line(
            "Entity     ",
            app.focus == Field::EntityName,
            app.cfg.entity_name.clone(),
        ),
        Line::from(""),
        form_line(
            "Gateway    ",
            app.focus == Field::GatewayAddr,
            app.cfg.gateway_addr.clone(),
        ),
        Line::from(""),
    ];
    lines.push(Line::from(vec![
        Span::styled(
            "  Save  ",
            if app.focus == Field::Save {
                Style::default().bg(Color::Green).fg(Color::Black)
            } else {
                Style::default().fg(Color::Green)
            },
        ),
        Span::raw("    "),
        Span::styled(
            "  Cancel  ",
            if app.focus == Field::Cancel {
                Style::default().bg(Color::Red).fg(Color::Black)
            } else {
                Style::default().fg(Color::Red)
            },
        ),
    ]));

    Paragraph::new(lines)
        .block(
            Block::default()
                .title("Configuration")
                .borders(Borders::ALL),
        )
        .alignment(Alignment::Left)
}

fn form_line(label: &str, focused: bool, value: String) -> Line<'_> {
    let label_style = if focused {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let value_style = if focused {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    };
    Line::from(vec![
        Span::styled(format!("  {}: ", label), label_style),
        Span::styled(value, value_style),
    ])
}

fn _centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
