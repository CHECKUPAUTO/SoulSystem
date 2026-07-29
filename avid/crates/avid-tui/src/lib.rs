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
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;

#[derive(Clone)]
enum MessageRole {
    User,
    Assistant,
    System,
    Error,
}

#[derive(Clone)]
struct ChatMessage {
    role: MessageRole,
    content: String,
}

#[derive(Default, Clone)]
struct WorkerStats {
    total_tasks: u64,
    queue_depth: u64,
    workers: usize,
}

struct App {
    input: Input,
    messages: Vec<ChatMessage>,
    current_model: String,
    stats: WorkerStats,
    history: Vec<TaskResult>,
    pending_tasks: Vec<String>,
    input_history: Vec<String>,
    input_history_index: Option<usize>,
    scroll_offset: u16,
    suggestions: Vec<&'static str>,
    show_suggestions: bool,
}

impl App {
    fn new() -> Self {
        Self {
            input: Input::default(),
            messages: vec![ChatMessage {
                role: MessageRole::System,
                content: "AVID Digital Organism v3.0 initialized. Type /help for commands."
                    .to_string(),
            }],
            current_model: "deepseek-v4-flash:cloud".to_string(),
            stats: WorkerStats::default(),
            history: Vec::new(),
            pending_tasks: Vec::new(),
            input_history: Vec::new(),
            input_history_index: None,
            scroll_offset: 0,
            suggestions: vec![
                "/model", "/task", "/status", "/history", "/view", "/help", "/quit", "/exit",
            ],
            show_suggestions: false,
        }
    }

    fn on_tick(&mut self) {
        // We could add any background update logic here if needed
    }

    fn add_message(&mut self, role: MessageRole, content: String) {
        self.messages.push(ChatMessage { role, content });
    }

    fn scroll_to_bottom(&mut self, viewport_height: u16) {
        let mut total_lines: u16 = 0;
        for m in &self.messages {
            // +1 for role name line, +1 for spacer
            total_lines += (m.content.lines().count() as u16).max(1) + 2;
        }
        self.scroll_offset = total_lines.saturating_sub(viewport_height);
    }

    fn prev_input_history(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        let next_index = match self.input_history_index {
            Some(i) if i > 0 => i - 1,
            Some(_) => 0,
            None => self.input_history.len() - 1,
        };
        self.input_history_index = Some(next_index);
        self.input = Input::new(self.input_history[next_index].clone());
    }

    fn next_input_history(&mut self) {
        let next_index = match self.input_history_index {
            Some(i) if i < self.input_history.len() - 1 => Some(i + 1),
            _ => {
                self.input_history_index = None;
                self.input = Input::default();
                return;
            }
        };
        self.input_history_index = next_index;
        if let Some(i) = next_index {
            self.input = Input::new(self.input_history[i].clone());
        }
    }

    fn autocomplete(&mut self) {
        let current_input = self.input.value();
        if current_input.starts_with('/') {
            let matches: Vec<&&str> = self
                .suggestions
                .iter()
                .filter(|s| s.starts_with(current_input))
                .collect();
            if matches.len() == 1 {
                self.input = Input::new(matches[0].to_string());
                self.show_suggestions = false;
            } else if !matches.is_empty() {
                self.show_suggestions = true;
            }
        }
    }
}

pub async fn run() -> Result<()> {
    color_eyre::install()?;
    let mut terminal = setup_terminal()?;
    // Held in `SecretString` so the token cannot reach a log line or a `{:?}`
    // render of any enclosing structure (INV-SEC-2). It is cloned into several
    // async tasks below; each clone stays typed, and is unwrapped only at the
    // header.
    let api_token = soulsystem_common::secrets::SecretString::new(
        std::env::var("AVID_API_TOKEN").unwrap_or_else(|_| "dev-token".to_string()),
    );
    let server_port = std::env::var("AVID_PORT").unwrap_or_else(|_| "3000".to_string());
    let server_url = format!("http://localhost:{server_port}");

    let app = Arc::new(Mutex::new(App::new()));

    let app_clone = Arc::clone(&app);
    let api_token_clone = api_token.clone();
    let server_url_clone = server_url.clone();

    tokio::spawn(async move {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        loop {
            // Poll metrics
            if let Ok(resp) = client
                .get(format!("{server_url_clone}/metrics"))
                .send()
                .await
            {
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
                    // Extract worker count from healthz or metrics if available
                    // For now, let's assume it's hardcoded to 4 as in main.rs or try to find it
                    stats.workers = 4;
                    if let Ok(mut a) = app_clone.lock() {
                        a.stats = stats;
                    }
                }
            }

            // Poll history and check for pending task completion
            if let Ok(resp) = client
                .get(format!("{server_url_clone}/tasks"))
                .header(
                    "Authorization",
                    format!("Bearer {}", api_token_clone.expose()),
                )
                .send()
                .await
            {
                if let Ok(tasks) = resp.json::<Vec<TaskResult>>().await {
                    let mut completed_tasks = Vec::new();
                    {
                        let mut a = app_clone.lock().unwrap();
                        a.history = tasks.clone();

                        // Check if any pending tasks are now completed
                        for task_id in &a.pending_tasks {
                            if let Some(t) = tasks.iter().find(|t| &t.task_id == task_id) {
                                if t.status != "accepted" && t.status != "pending" {
                                    completed_tasks.push(task_id.clone());
                                }
                            }
                        }
                    }

                    for task_id in completed_tasks {
                        if let Ok(resp) = client
                            .get(format!("{server_url_clone}/tasks/{task_id}"))
                            .header(
                                "Authorization",
                                format!("Bearer {}", api_token_clone.expose()),
                            )
                            .send()
                            .await
                        {
                            if let Ok(task) = resp.json::<TaskResult>().await {
                                let mut a = app_clone.lock().unwrap();
                                a.pending_tasks.retain(|id| id != &task_id);
                                let mut content = format!(
                                    "Task {} completed with status: {}\n",
                                    task.task_id, task.status
                                );
                                if let Some(sub) = task.submission {
                                    content.push_str("\n--- Generated Code ---\n");
                                    for (path, code) in sub.files {
                                        content.push_str(&format!("\nFile: {path}\n{code}\n"));
                                    }
                                }
                                a.add_message(MessageRole::Assistant, content);
                            }
                        }
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    let res = run_app(&mut terminal, Arc::clone(&app), api_token, server_url).await;
    restore_terminal(&mut terminal)?;
    res
}

pub fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: Arc<Mutex<App>>,
    api_token: soulsystem_common::secrets::SecretString,
    server_url: String,
) -> Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        {
            let mut app_guard = app.lock().unwrap();
            terminal.draw(|f| {
                ui(f, &app_guard);
                // After drawing, we know the viewport height (chunks[1].height in ui)
                // but ui doesn't return it easily here without refactoring.
                // Let's use f.area().height - 2 (header + input)
                let viewport_height = f.area().height.saturating_sub(2);
                app_guard.scroll_to_bottom(viewport_height);
            })?;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut should_break = false;
                    let mut cmd_to_execute = None;

                    {
                        let mut app_guard = app.lock().unwrap();
                        match key.code {
                            KeyCode::Enter => {
                                let content = app_guard.input.value().to_string();
                                if !content.is_empty() {
                                    app_guard.input_history.push(content.clone());
                                    app_guard.input_history_index = None;
                                    app_guard.add_message(MessageRole::User, content.clone());
                                    cmd_to_execute = Some(content);
                                    app_guard.input.reset();
                                }
                            }
                            KeyCode::Tab => {
                                app_guard.autocomplete();
                            }
                            KeyCode::Up => {
                                app_guard.prev_input_history();
                            }
                            KeyCode::Down => {
                                app_guard.next_input_history();
                            }
                            KeyCode::Char('c')
                                if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                            {
                                should_break = true;
                            }
                            _ => {
                                app_guard.input.handle_event(&Event::Key(key));
                                app_guard.show_suggestions = false;
                            }
                        }
                    }

                    if should_break {
                        return Ok(());
                    }

                    if let Some(cmd) = cmd_to_execute {
                        if cmd.starts_with('/') {
                            let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
                            let command = parts[0];
                            let args = if parts.len() > 1 { parts[1] } else { "" };

                            match command {
                                "/quit" | "/exit" => return Ok(()),
                                "/help" => {
                                    let mut app_guard = app.lock().unwrap();
                                    app_guard.add_message(MessageRole::System, "/model <name> - change model\n/task <desc> - submit task\n/status - view server status\n/history - view task history\n/view <id> - view task details\n/help - show this help\n/quit - exit".to_string());
                                }
                                "/model" => {
                                    let mut app_guard = app.lock().unwrap();
                                    if args.is_empty() {
                                        app_guard.add_message(
                                            MessageRole::Error,
                                            "Usage: /model <name>".to_string(),
                                        );
                                    } else {
                                        app_guard.current_model = args.to_string();
                                        app_guard.add_message(
                                            MessageRole::System,
                                            format!("Model changed to {args}"),
                                        );
                                    }
                                }
                                "/status" => {
                                    let app_clone = Arc::clone(&app);
                                    let server_url_clone = server_url.clone();
                                    tokio::spawn(async move {
                                        let client = Client::builder()
                                            .timeout(Duration::from_secs(5))
                                            .build()
                                            .unwrap();
                                        match client
                                            .get(format!("{server_url_clone}/healthz"))
                                            .send()
                                            .await
                                        {
                                            Ok(resp) => {
                                                if let Ok(json) =
                                                    resp.json::<serde_json::Value>().await
                                                {
                                                    let mut app_guard = app_clone.lock().unwrap();
                                                    app_guard.add_message(MessageRole::Assistant, format!("Server Status: {}\nQueue: {}\nMemory: {}\nLLM: {}",
                                                        json["status"], json["queue"], json["memory"], json["llm"]));
                                                }
                                            }
                                            Err(e) => {
                                                let mut app_guard = app_clone.lock().unwrap();
                                                app_guard.add_message(
                                                    MessageRole::Error,
                                                    format!("Failed to fetch status: {e}"),
                                                );
                                            }
                                        }
                                    });
                                }
                                "/history" => {
                                    let mut app_guard = app.lock().unwrap();
                                    if app_guard.history.is_empty() {
                                        app_guard.add_message(
                                            MessageRole::Assistant,
                                            "No tasks found.".to_string(),
                                        );
                                    } else {
                                        let mut history_str = String::from("Recent Tasks:\n");
                                        for t in app_guard.history.iter().take(10) {
                                            history_str.push_str(&format!(
                                                "- [{}] {}\n",
                                                t.status, t.task_id
                                            ));
                                        }
                                        app_guard.add_message(MessageRole::Assistant, history_str);
                                    }
                                }
                                "/view" => {
                                    if args.is_empty() {
                                        let mut app_guard = app.lock().unwrap();
                                        app_guard.add_message(
                                            MessageRole::Error,
                                            "Usage: /view <task_id>".to_string(),
                                        );
                                    } else {
                                        let task_id = args.to_string();
                                        let app_clone = Arc::clone(&app);
                                        let api_token_clone = api_token.clone();
                                        let server_url_clone = server_url.clone();
                                        tokio::spawn(async move {
                                            let client = Client::builder()
                                                .timeout(Duration::from_secs(5))
                                                .build()
                                                .unwrap();
                                            match client
                                                .get(format!("{server_url_clone}/tasks/{task_id}"))
                                                .header(
                                                    "Authorization",
                                                    format!("Bearer {}", api_token_clone.expose()),
                                                )
                                                .send()
                                                .await
                                            {
                                                Ok(resp) => {
                                                    if let Ok(task) =
                                                        resp.json::<TaskResult>().await
                                                    {
                                                        let mut app_guard =
                                                            app_clone.lock().unwrap();
                                                        let mut content = format!(
                                                            "Task ID: {}\nStatus: {}\n",
                                                            task.task_id, task.status
                                                        );
                                                        if let Some(sub) = task.submission {
                                                            content.push_str("\n--- Code ---\n");
                                                            for (path, code) in sub.files {
                                                                content.push_str(&format!(
                                                                    "\nFile: {path}\n{code}\n"
                                                                ));
                                                            }
                                                        }
                                                        app_guard.add_message(
                                                            MessageRole::Assistant,
                                                            content,
                                                        );
                                                    } else {
                                                        let mut app_guard =
                                                            app_clone.lock().unwrap();
                                                        app_guard.add_message(
                                                            MessageRole::Error,
                                                            "Task not found or invalid response."
                                                                .to_string(),
                                                        );
                                                    }
                                                }
                                                Err(e) => {
                                                    let mut app_guard = app_clone.lock().unwrap();
                                                    app_guard.add_message(
                                                        MessageRole::Error,
                                                        format!("Error fetching task: {e}"),
                                                    );
                                                }
                                            }
                                        });
                                    }
                                }
                                "/task" => {
                                    if args.is_empty() {
                                        let mut app_guard = app.lock().unwrap();
                                        app_guard.add_message(
                                            MessageRole::Error,
                                            "Usage: /task <description>".to_string(),
                                        );
                                    } else {
                                        let task_desc = args.to_string();
                                        let app_clone = Arc::clone(&app);
                                        let api_token_clone = api_token.clone();
                                        let server_url_clone = server_url.clone();
                                        let model = app_clone.lock().unwrap().current_model.clone();
                                        tokio::spawn(async move {
                                            let client = Client::builder()
                                                .timeout(Duration::from_secs(5))
                                                .build()
                                                .unwrap();
                                            let req = TaskRequest {
                                                task: task_desc.clone(),
                                                url: None,
                                                task_type: None,
                                                metadata: Some(serde_json::json!({"model": model})),
                                            };
                                            match client
                                                .post(format!("{server_url_clone}/tasks"))
                                                .header(
                                                    "Authorization",
                                                    format!("Bearer {}", api_token_clone.expose()),
                                                )
                                                .json(&req)
                                                .send()
                                                .await
                                            {
                                                Ok(resp) if resp.status().is_success() => {
                                                    if let Ok(json) =
                                                        resp.json::<serde_json::Value>().await
                                                    {
                                                        let task_id = json["task_id"]
                                                            .as_str()
                                                            .unwrap_or_default()
                                                            .to_string();
                                                        let mut app_guard =
                                                            app_clone.lock().unwrap();
                                                        app_guard
                                                            .pending_tasks
                                                            .push(task_id.clone());
                                                        app_guard.add_message(MessageRole::Assistant, format!("Task submitted successfully. ID: {task_id}"));
                                                    }
                                                }
                                                Ok(resp) => {
                                                    let mut app_guard = app_clone.lock().unwrap();
                                                    app_guard.add_message(
                                                        MessageRole::Error,
                                                        format!(
                                                            "Server returned error: {}",
                                                            resp.status()
                                                        ),
                                                    );
                                                }
                                                Err(e) => {
                                                    let mut app_guard = app_clone.lock().unwrap();
                                                    app_guard.add_message(
                                                        MessageRole::Error,
                                                        format!("Failed to submit task: {e}"),
                                                    );
                                                }
                                            }
                                        });
                                    }
                                }
                                _ => {
                                    let mut app_guard = app.lock().unwrap();
                                    app_guard.add_message(
                                        MessageRole::Error,
                                        format!("Unknown command: {command}"),
                                    );
                                }
                            }
                        } else {
                            // Default behavior for non-slash command: submit as task
                            let task_desc = cmd.clone();
                            let app_clone = Arc::clone(&app);
                            let api_token_clone = api_token.clone();
                            let server_url_clone = server_url.clone();
                            let model = app_clone.lock().unwrap().current_model.clone();
                            tokio::spawn(async move {
                                let client = Client::builder()
                                    .timeout(Duration::from_secs(5))
                                    .build()
                                    .unwrap();
                                let req = TaskRequest {
                                    task: task_desc,
                                    url: None,
                                    task_type: None,
                                    metadata: Some(serde_json::json!({"model": model})),
                                };
                                match client
                                    .post(format!("{server_url_clone}/tasks"))
                                    .header(
                                        "Authorization",
                                        format!("Bearer {}", api_token_clone.expose()),
                                    )
                                    .json(&req)
                                    .send()
                                    .await
                                {
                                    Ok(resp) if resp.status().is_success() => {
                                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                                            let task_id = json["task_id"]
                                                .as_str()
                                                .unwrap_or_default()
                                                .to_string();
                                            let mut app_guard = app_clone.lock().unwrap();
                                            app_guard.pending_tasks.push(task_id.clone());
                                            app_guard.add_message(
                                                MessageRole::Assistant,
                                                format!(
                                                    "Task submitted successfully. ID: {task_id}"
                                                ),
                                            );
                                        }
                                    }
                                    _ => {}
                                }
                            });
                        }
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
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(1),    // Chat area
            Constraint::Length(1), // Input line
        ])
        .split(area);

    // ── Header ───────────────────────────────────────────────────────────
    let header_content = Line::from(vec![
        Span::styled(
            " AVID ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ({}) ", app.current_model),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(" | "),
        Span::styled(
            format!(" Workers: {} ", app.stats.workers),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" | "),
        Span::styled(
            format!(" Queue: {} ", app.stats.queue_depth),
            Style::default().fg(Color::Yellow),
        ),
    ]);
    f.render_widget(
        Paragraph::new(header_content).bg(Color::Rgb(20, 20, 20)),
        chunks[0],
    );

    // ── Chat Area ────────────────────────────────────────────────────────
    let mut chat_lines = Vec::new();
    for msg in &app.messages {
        let (role_name, color) = match msg.role {
            MessageRole::User => ("You", Color::Blue),
            MessageRole::Assistant => ("AVID", Color::Green),
            MessageRole::System => ("System", Color::DarkGray),
            MessageRole::Error => ("Error", Color::Red),
        };

        chat_lines.push(Line::from(vec![Span::styled(
            format!("{role_name} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )]));

        for line in msg.content.lines() {
            chat_lines.push(Line::from(vec![Span::raw(format!("  {line}"))]));
        }
        chat_lines.push(Line::from("")); // Spacer
    }

    let chat_widget = Paragraph::new(chat_lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));
    f.render_widget(chat_widget, chunks[1]);

    // ── Input Line ───────────────────────────────────────────────────────
    let input_prefix = Span::styled("> ", Style::default().fg(Color::Magenta));
    let input_text = app.input.value();

    // Simple slash command highlighting
    let input_content = if input_text.starts_with('/') {
        let parts: Vec<&str> = input_text.splitn(2, ' ').collect();
        let mut spans = vec![
            input_prefix,
            Span::styled(parts[0], Style::default().fg(Color::Cyan)),
        ];
        if parts.len() > 1 {
            spans.push(Span::raw(" "));
            spans.push(Span::raw(parts[1]));
        }
        Line::from(spans)
    } else {
        Line::from(vec![input_prefix, Span::raw(input_text)])
    };

    f.render_widget(
        Paragraph::new(input_content).bg(Color::Rgb(30, 30, 30)),
        chunks[2],
    );

    // Cursor
    f.set_cursor_position(Position::new(
        chunks[2].x + u16::try_from(app.input.cursor()).unwrap() + 2,
        chunks[2].y,
    ));

    // Suggestions popup (very simple)
    if app.show_suggestions {
        let current_input = app.input.value();
        let matches: Vec<ListItem> = app
            .suggestions
            .iter()
            .filter(|s| s.starts_with(current_input))
            .map(|s| ListItem::new(*s))
            .collect();

        if !matches.is_empty() {
            let area = Rect::new(
                chunks[2].x + 2,
                chunks[2].y.saturating_sub(matches.len() as u16),
                20,
                matches.len() as u16,
            );
            let list = List::new(matches).block(
                Block::default()
                    .borders(Borders::ALL)
                    .bg(Color::Rgb(40, 40, 40)),
            );
            f.render_widget(Clear, area);
            f.render_widget(list, area);
        }
    }
}

#[cfg(test)]
mod tests_inline;
