//! # soul_repl - TUI interactif pour l'entite autonome
//!
//! Interface terminal riche inspiree d'OpenCode/Claude Code :
//! - File browser avec previsualisation
//! - Command palette (Ctrl+Shift+P)
//! - Session management (save/load)
//! - History search (Ctrl+R)
//! - Notification toasts
//! - Multi-line input (Shift+Enter)
//! - Tool execution display
//! - Copy to clipboard (Ctrl+Y)
//! - Status bar enrichie

mod types;
mod utils;
mod render;
mod app;

use crate::types::*;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::Terminal;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use soul_llm::{LlmClient, LlmConfig};
use soul_persistence::LongTermMemory;
use soul_planner::CognitiveLoop;
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
    pub fn new(config: LlmConfig) -> Result<Self, String> {
        let mut reg = ToolRegistry::new();
        for t in soul_tools::discover_system_tools() {
            reg.register(t);
        }
        Ok(Self {
            llm: LlmClient::new(config).map_err(|e| format!("init LLM client: {e}"))?,
            planner: CognitiveLoop::new(),
            tools: reg,
            sandbox: Arc::new(Sandbox::new(soul_sandbox::SandboxPolicy::default())),
            memory: None,
            entity_name: "soul".into(),
        })
    }

    pub fn with_memory(mut self, mem: Arc<LongTermMemory>) -> Self {
        self.memory = Some(mem);
        self
    }
}

pub async fn run_repl(state: &mut ReplState) -> Result<(), String> {
    enable_raw_mode().map_err(|e| format!("enable raw mode: {e}"))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| format!("enter alternate screen: {e}"))?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| format!("create terminal: {e}"))?;

    let (tx, mut rx) = mpsc::unbounded_channel::<LlmEvent>();
    let mut app = App::new(state, tx);

    loop {
        terminal.draw(|f| app.draw(f)).map_err(|e| format!("terminal draw: {e}"))?;

        while let Ok(evt) = rx.try_recv() {
            app.handle_llm_event(evt);
        }

        if event::poll(Duration::from_millis(16)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if app.handle_key(key, &state.llm).await {
                    break;
                }
            }
        }

        app.tick();
    }

    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::utils::{centered_rect, format_size, base64_encode};
    use ratatui::layout::Rect;

    #[test]
    fn centered_rect_calculation() {
        let rect = Rect::new(0, 0, 100, 50);
        let centered = centered_rect(50, 40, rect);
        assert!(centered.width > 0);
        assert!(centered.height > 0);
    }

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn base64_encode_works() {
        assert_eq!(base64_encode("Hello"), "SGVsbG8=");
        assert_eq!(base64_encode("Hi"), "SGk=");
        assert_eq!(base64_encode(""), "");
    }

    #[test]
    fn provider_opt_labels() {
        use crate::types::ProviderOpt;
        assert_eq!(ProviderOpt::Ollama.label(), "Ollama");
        assert_eq!(ProviderOpt::OpenAI.label(), "OpenAI");
        assert_eq!(ProviderOpt::Anthropic.label(), "Anthropic");
    }

    #[test]
    fn provider_opt_from_kind() {
        use crate::types::ProviderOpt;
        use soul_llm::ProviderKind;
        assert_eq!(ProviderOpt::from_kind(&ProviderKind::Ollama), ProviderOpt::Ollama);
        assert_eq!(ProviderOpt::from_kind(&ProviderKind::OpenAI), ProviderOpt::OpenAI);
        assert_eq!(ProviderOpt::from_kind(&ProviderKind::Anthropic), ProviderOpt::Anthropic);
    }

    #[test]
    fn toast_level_colors() {
        use crate::types::ToastLevel;
        assert_ne!(ToastLevel::Info.color(), ToastLevel::Error.color());
        assert_ne!(ToastLevel::Success.color(), ToastLevel::Warning.color());
    }

    #[test]
    fn toast_level_icons() {
        use crate::types::ToastLevel;
        assert!(!ToastLevel::Info.icon().is_empty());
        assert!(!ToastLevel::Error.icon().is_empty());
    }

    #[test]
    fn focus_default_input() {
        use crate::types::Focus;
        assert_eq!(Focus::Input, Focus::Input);
        assert_ne!(Focus::Input, Focus::HelpDialog);
        assert_ne!(Focus::CommandPalette, Focus::FileBrowser);
    }
}
