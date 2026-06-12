//! Types du TUI — structs, enums, et leurs impls triviales.

use ratatui::style::Color;
use ratatui::text::Line;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::mpsc;

use soul_llm::ProviderKind;

#[allow(dead_code)]
pub(crate) enum LlmEvent {
    Response(String),
    Error(String),
    StreamChunk(String),
    StreamDone,
    ToolCall { name: String, args: String },
    ToolResult { name: String, output: String },
}

#[derive(Debug, PartialEq)]
pub(crate) enum Focus {
    Input,
    ModelDialog,
    ProviderDialog,
    HelpDialog,
    CommandPalette,
    FileBrowser,
    HistorySearch,
    SessionManager,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum ProviderOpt {
    Ollama,
    OpenAI,
    Anthropic,
}

impl ProviderOpt {
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Ollama => "Ollama",
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
        }
    }
    pub(crate) fn from_kind(k: &ProviderKind) -> Self {
        match k {
            ProviderKind::Ollama => Self::Ollama,
            ProviderKind::OpenAI => Self::OpenAI,
            ProviderKind::Anthropic => Self::Anthropic,
        }
    }
}

#[allow(dead_code)]
pub(crate) struct Message {
    pub(crate) role: Role,
    pub(crate) text: String,
    pub(crate) timestamp: Instant,
    pub(crate) tool_call: Option<ToolCallInfo>,
}

#[allow(dead_code)]
pub(crate) struct ToolCallInfo {
    pub(crate) name: String,
    pub(crate) args: String,
    pub(crate) result: Option<String>,
    pub(crate) duration_ms: u64,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Role {
    User,
    Assistant,
    System,
    Error,
    Tool,
}

#[allow(dead_code)]
pub(crate) struct Toast {
    pub(crate) message: String,
    pub(crate) level: ToastLevel,
    pub(crate) created_at: Instant,
}

#[allow(dead_code)]
#[derive(Clone, PartialEq)]
pub(crate) enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    pub(crate) fn color(&self) -> Color {
        match self {
            Self::Info => Color::Cyan,
            Self::Success => Color::Green,
            Self::Warning => Color::Yellow,
            Self::Error => Color::Red,
        }
    }
    pub(crate) fn icon(&self) -> &str {
        match self {
            Self::Info => "\u{2139}",
            Self::Success => "\u{2713}",
            Self::Warning => "\u{26A0}",
            Self::Error => "\u{2717}",
        }
    }
}

pub(crate) struct FileEntry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) is_dir: bool,
    pub(crate) size: u64,
}

#[allow(dead_code)]
pub(crate) struct HistorySearch {
    pub(crate) query: String,
    pub(crate) cursor: usize,
    pub(crate) results: Vec<usize>,
    pub(crate) selected: usize,
}

#[allow(dead_code)]
pub(crate) struct SessionEntry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) message_count: usize,
    pub(crate) created_at: String,
}

pub(crate) struct CommandPaletteItem {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) shortcut: Option<String>,
    pub(crate) action: CommandAction,
}

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) enum CommandAction {
    Ask,
    Plan,
    Run,
    Models,
    Status,
    Help,
    Clear,
    SaveSession,
    LoadSession,
    ExportChat,
    CopyLast,
    FileBrowser,
    HistorySearch,
    ThemeChange,
}

#[allow(dead_code)]
pub(crate) struct App {
    pub(crate) messages: Vec<Message>,
    pub(crate) input: String,
    pub(crate) input_cursor: usize,
    pub(crate) input_history: VecDeque<String>,
    pub(crate) history_index: Option<usize>,
    pub(crate) focus: Focus,
    pub(crate) scroll_offset: u16,
    pub(crate) model_items: Vec<String>,
    pub(crate) model_selected: usize,
    pub(crate) provider_items: Vec<ProviderOpt>,
    pub(crate) provider_selected: usize,
    pub(crate) streaming: bool,
    pub(crate) stream_buffer: String,
    pub(crate) llm_tx: mpsc::UnboundedSender<LlmEvent>,
    pub(crate) toasts: Vec<Toast>,
    pub(crate) last_input_time: Instant,
    pub(crate) total_tokens: usize,
    pub(crate) command_count: usize,

    pub(crate) file_browser_path: PathBuf,
    pub(crate) file_browser_entries: Vec<FileEntry>,
    pub(crate) file_browser_selected: usize,
    pub(crate) file_browser_scroll: usize,

    pub(crate) command_palette_items: Vec<CommandPaletteItem>,
    pub(crate) command_palette_selected: usize,
    pub(crate) command_palette_filter: String,

    pub(crate) history_search: Option<HistorySearch>,

    pub(crate) sessions: Vec<SessionEntry>,
    pub(crate) session_selected: usize,

    pub(crate) diff_content: Vec<Line<'static>>,
}
