use avid_core::models::{TaskRequest, TaskResult};
use color_eyre::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use reqwest::Client;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use throbber_widgets_tui::ThrobberState;
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

#[derive(PartialEq, Clone, Copy, Debug)]
enum AppState {
    Input,
    Processing,
    TaskDetail,
}

#[derive(Default, Clone)]
struct WorkerStats {
    total_tasks: u64,
    queue_depth: u64,
}

struct App {
    input: Input,
    state: AppState,
    throbber_state: ThrobberState,
    logs: Vec<String>,
    history: Vec<TaskResult>,
    history_state: ListState,
    stats: WorkerStats,
    selected_task: Option<TaskResult>,
}

impl App {
    fn new() -> Self {
        Self {
            input: Input::default(),
            state: AppState::Input,
            throbber_state: ThrobberState::default(),
            logs: vec!["AVID Digital Organism v3.0 initialized.".to_string()],
            history: Vec::new(),
            history_state: ListState::default(),
            stats: WorkerStats::default(),
            selected_task: None,
        }
    }

    fn on_tick(&mut self) {
        self.throbber_state.calc_next();
    }

    fn next_history(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let i = match self.history_state.selected() {
            Some(_i) => {
                if _i >= self.history.len().saturating_sub(1) {
                    0
                } else {
                    _i + 1
                }
            }
            None => 0,
        };
        self.history_state.select(Some(i));
    }

    fn prev_history(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let i = match self.history_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.history.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.history_state.select(Some(i));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let mut terminal = setup_terminal()?;
    let app = Arc::new(Mutex::new(App::new()));

    let app_clone = Arc::clone(&app);
    tokio::spawn(async move {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        loop {
            // Poll metrics
            if let Ok(resp) = client.get("http://localhost:3000/metrics").send().await {
                if let Ok(text) = resp.text().await {
                    let mut stats = WorkerStats::default();
                    for line in text.lines() {
                        if line.starts_with("avid_tasks_total") {
                            if let Some(val) = line.split_whitespace().last() {
                                if let Ok(n) = val.parse::<u64>() {
                                    stats.total_tasks = n;
                                }
                            }
                        } else if line.starts_with("avid_queue_depth") {
                            if let Some(val) = line.split_whitespace().last() {
                                if let Ok(n) = val.parse::<u64>() {
                                    stats.queue_depth = n;
                                }
                            }
                        }
                    }
                    if let Ok(mut a) = app_clone.lock() {
                        a.stats = stats;
                    }
                }
            }

            // Poll history
            if let Ok(resp) = client
                .get("http://localhost:3000/tasks")
                .header("Authorization", "Bearer dev-token")
                .send()
                .await
            {
                if let Ok(tasks) = resp.json::<Vec<TaskResult>>().await {
                    if let Ok(mut a) = app_clone.lock() {
                        a.history = tasks;
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    let res = run_app(&mut terminal, Arc::clone(&app)).await;
    restore_terminal(&mut terminal)?;
    res
}

use avid_tui::restore_terminal;
use avid_tui::setup_terminal;

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: Arc<Mutex<App>>,
) -> Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        {
            let app_guard = app.lock().unwrap();
            terminal.draw(|f| ui(f, &app_guard))?;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut should_break = false;
                    let mut submission_content = None;
                    let mut detail_task_id = None;

                    {
                        let mut app_guard = app.lock().unwrap();
                        match app_guard.state {
                            AppState::Input => match key.code {
                                KeyCode::Enter => {
                                    if let Some(idx) = app_guard.history_state.selected() {
                                        if let Some(task) = app_guard.history.get(idx) {
                                            detail_task_id = Some(task.task_id.clone());
                                        }
                                    } else {
                                        let content = app_guard.input.value().to_string();
                                        if !content.is_empty() {
                                            submission_content = Some(content);
                                            app_guard.state = AppState::Processing;
                                            app_guard.input.reset();
                                        }
                                    }
                                }
                                KeyCode::Char('j') | KeyCode::Down => app_guard.next_history(),
                                KeyCode::Char('k') | KeyCode::Up => app_guard.prev_history(),
                                KeyCode::Esc => should_break = true,
                                _ => {
                                    app_guard.input.handle_event(&Event::Key(key));
                                }
                            },
                            AppState::Processing => {
                                if key.code == KeyCode::Esc {
                                    app_guard.state = AppState::Input;
                                }
                            }
                            AppState::TaskDetail => {
                                if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                                    app_guard.state = AppState::Input;
                                }
                            }
                        }
                    }

                    if should_break {
                        return Ok(());
                    }

                    if let Some(task_id) = detail_task_id {
                        let app_clone = Arc::clone(&app);
                        tokio::spawn(async move {
                            let client = Client::builder()
                                .timeout(Duration::from_secs(5))
                                .build()
                                .unwrap();
                            if let Ok(resp) = client
                                .get(format!("http://localhost:3000/tasks/{task_id}"))
                                .header("Authorization", "Bearer dev-token")
                                .send()
                                .await
                            {
                                if let Ok(task) = resp.json::<TaskResult>().await {
                                    let mut app_guard = app_clone.lock().unwrap();
                                    app_guard.selected_task = Some(task);
                                    app_guard.state = AppState::TaskDetail;
                                }
                            }
                        });
                    }

                    if let Some(content) = submission_content {
                        let app_clone = Arc::clone(&app);
                        tokio::spawn(async move {
                            let client = Client::builder()
                                .timeout(Duration::from_secs(5))
                                .build()
                                .unwrap();
                            let req = TaskRequest {
                                task: content.clone(),
                                url: if content.starts_with("http") {
                                    Some(content.clone())
                                } else {
                                    None
                                },
                                task_type: None,
                                metadata: None,
                            };

                            let res = client
                                .post("http://localhost:3000/tasks")
                                .header("Authorization", "Bearer dev-token")
                                .json(&req)
                                .send()
                                .await;

                            let mut app_guard = app_clone.lock().unwrap();
                            match res {
                                Ok(resp) => {
                                    if resp.status().is_success() {
                                        app_guard.logs.push("Task accepted.".to_string());
                                    } else {
                                        app_guard.logs.push(format!("Error: {}", resp.status()));
                                    }
                                }
                                Err(e) => {
                                    app_guard.logs.push(format!("Failed: {e}"));
                                }
                            }
                            app_guard.state = AppState::Input;
                        });
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            let mut app_guard = app.lock().unwrap();
            app_guard.on_tick();
            last_tick = Instant::now();
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(size);

    // Header
    let header_text = format!(
        " AVID v3.0 | Tasks: {} | Queue: {}",
        app.stats.total_tasks, app.stats.queue_depth
    );
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            " AVID ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" - "),
        Span::raw(header_text),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(header, chunks[0]);

    // Main Body
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    // Left Panel: History & Logs
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[0]);

    let history_items: Vec<ListItem> = app
        .history
        .iter()
        .map(|t| {
            let color = match t.status.as_str() {
                "accepted" => Color::Green,
                "rejected" => Color::Red,
                "timeout" => Color::Yellow,
                _ => Color::Gray,
            };
            ListItem::new(format!(
                "[{}] {}",
                t.status,
                &t.task_id[..8.min(t.task_id.len())]
            ))
            .style(Style::default().fg(color))
        })
        .collect();

    let mut history_state = app.history_state.clone();
    let history_list = List::new(history_items)
        .block(
            Block::default()
                .title(" History (Enter to view) ")
                .borders(Borders::ALL),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    f.render_stateful_widget(history_list, left_chunks[0], &mut history_state);

    let logs: Vec<ListItem> = app
        .logs
        .iter()
        .rev()
        .map(|l| {
            ListItem::new(Line::from(vec![
                Span::styled("● ", Style::default().fg(Color::Green)),
                Span::raw(l),
            ]))
        })
        .collect();
    let logs_list = List::new(logs).block(Block::default().title(" Logs ").borders(Borders::ALL));
    f.render_widget(logs_list, left_chunks[1]);

    // Right Panel
    let right_block = Block::default().title(" Dashboard ").borders(Borders::ALL);
    match &app.state {
        AppState::Input | AppState::Processing => {
            let inner_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(right_block.inner(main_chunks[1]));

            let input_text = app.input.value();
            let input_widget = Paragraph::new(input_text)
                .block(Block::default().borders(Borders::ALL).title(" New Task "));
            f.render_widget(input_widget, inner_chunks[0]);

            if app.state == AppState::Processing {
                f.render_widget(Paragraph::new("Processing..."), inner_chunks[1]);
            } else {
                f.render_widget(
                    Paragraph::new("Type task and press Enter.\nUse arrow keys to select history.")
                        .style(Style::default().fg(Color::Gray)),
                    inner_chunks[1],
                );
            }
        }
        AppState::TaskDetail => {
            if let Some(task) = &app.selected_task {
                let mut content = format!("ID: {}\nStatus: {}\n\n", task.task_id, task.status);
                if let Some(sub) = &task.submission {
                    content.push_str("Files:\n");
                    for path in sub.files.keys() {
                        content.push_str(&format!("- {path}\n"));
                    }
                    if let Some(code) = sub.files.get(&sub.entrypoint) {
                        content.push_str("\n--- Entrypoint Code ---\n");
                        content.push_str(code);
                    }
                } else {
                    content.push_str("No submission available.");
                }
                f.render_widget(
                    Paragraph::new(content)
                        .block(
                            Block::default()
                                .title(" Task Detail ")
                                .borders(Borders::ALL),
                        )
                        .wrap(Wrap { trim: false }),
                    main_chunks[1],
                );
            }
        }
    }

    // Footer
    let footer = Paragraph::new(" AVID v3.0 Hardened | [Esc] Exit | [Enter] Select/Submit ")
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(footer, chunks[2]);
}
