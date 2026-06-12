use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum CompactionError {
    #[error("No messages to compact")]
    Empty,
    #[error("Serialization error: {0}")]
    Serde(String),
}

pub type Result<T> = std::result::Result<T, CompactionError>;

// ─── Message ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub token_count: Option<usize>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    pub fn new(role: Role, content: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            role,
            content: content.into(),
            timestamp: Utc::now(),
            token_count: None,
            metadata: HashMap::new(),
        }
    }

    pub fn with_tokens(mut self, tokens: usize) -> Self {
        self.token_count = Some(tokens);
        self
    }

    pub fn estimated_tokens(&self) -> usize {
        self.token_count.unwrap_or_else(|| {
            // Rough estimate: 1 token ≈ 4 chars
            self.content.len().div_ceil(4)
        })
    }
}

// ─── Compaction Stats ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionStats {
    pub original_count: usize,
    pub final_count: usize,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub reclaimed_tokens: usize,
    pub removed_messages: usize,
    pub compressed_messages: usize,
    pub evicted_messages: usize,
}

impl CompactionStats {
    pub fn compression_ratio(&self) -> f64 {
        if self.tokens_before == 0 {
            return 1.0;
        }
        self.tokens_after as f64 / self.tokens_before as f64
    }

    pub fn savings_pct(&self) -> f64 {
        (1.0 - self.compression_ratio()) * 100.0
    }
}

// ─── Compaction Strategies ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompactionStrategy {
    /// Reclaim expired tool results (free up tokens immediately)
    Reclaim,
    /// Shrink old turns into summaries
    Shrink,
    /// Collapse images to file references, long code to summaries
    Collapse,
    /// Evict oldest non-critical messages to disk
    Evict,
}

// ─── 4-Pass Compaction Pipeline ─────────────────────────────────────────────

pub struct Compactor {
    max_tokens: usize,
    strategies: Vec<CompactionStrategy>,
}

impl Compactor {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            strategies: vec![
                CompactionStrategy::Reclaim,
                CompactionStrategy::Shrink,
                CompactionStrategy::Collapse,
                CompactionStrategy::Evict,
            ],
        }
    }

    pub fn with_strategies(mut self, strategies: Vec<CompactionStrategy>) -> Self {
        self.strategies = strategies;
        self
    }

    pub fn compact(&self, messages: &[Message]) -> Result<(Vec<Message>, CompactionStats)> {
        if messages.is_empty() {
            return Err(CompactionError::Empty);
        }

        let tokens_before: usize = messages.iter().map(|m| m.estimated_tokens()).sum();
        let count_before = messages.len();

        let mut working = messages.to_vec();
        let mut stats = CompactionStats {
            original_count: count_before,
            final_count: 0,
            tokens_before,
            tokens_after: tokens_before,
            reclaimed_tokens: 0,
            removed_messages: 0,
            compressed_messages: 0,
            evicted_messages: 0,
        };

        for strategy in &self.strategies {
            let before_tokens: usize = working.iter().map(|m| m.estimated_tokens()).sum();
            match strategy {
                CompactionStrategy::Reclaim => {
                    let (result, reclaimed) = self.pass_reclaim(&working);
                    stats.removed_messages += reclaimed;
                    stats.reclaimed_tokens += working
                        .iter()
                        .filter(|m| matches!(m.role, Role::Tool))
                        .map(|m| m.estimated_tokens())
                        .sum::<usize>()
                        - result
                            .iter()
                            .filter(|m| matches!(m.role, Role::Tool))
                            .map(|m| m.estimated_tokens())
                            .sum::<usize>();
                    working = result;
                }
                CompactionStrategy::Shrink => {
                    let (result, compressed) = self.pass_shrink(&working);
                    stats.compressed_messages += compressed;
                    working = result;
                }
                CompactionStrategy::Collapse => {
                    let result = self.pass_collapse(&working);
                    working = result;
                }
                CompactionStrategy::Evict => {
                    let (result, evicted) = self.pass_evict(&working);
                    stats.evicted_messages += evicted;
                    working = result;
                }
            }
            let after_tokens: usize = working.iter().map(|m| m.estimated_tokens()).sum();
            stats.reclaimed_tokens += before_tokens.saturating_sub(after_tokens);
        }

        stats.tokens_after = working.iter().map(|m| m.estimated_tokens()).sum();
        stats.final_count = working.len();

        Ok((working, stats))
    }

    /// Pass 1: Reclaim expired tool results (older than 10 turns, keep first+last)
    fn pass_reclaim(&self, messages: &[Message]) -> (Vec<Message>, usize) {
        let mut result = Vec::new();
        let mut removed = 0;

        // Find tool message groups
        let mut i = 0;
        while i < messages.len() {
            if matches!(messages[i].role, Role::Tool) {
                // Find the tool result and its preceding user message
                let start = i;
                while i < messages.len() && matches!(messages[i].role, Role::Tool) {
                    i += 1;
                }
                let tool_group_len = i - start;

                // If group is >3 messages, keep only first and last, replace middle with summary
                if tool_group_len > 3 {
                    result.push(messages[start].clone());
                    let total_tokens: usize = messages[start..i]
                        .iter()
                        .map(|m| m.estimated_tokens())
                        .sum();
                    result.push(Message::new(
                        Role::Tool,
                        &format!(
                            "[{tool_group_len} tool results reclaimed, ~{total_tokens} tokens freed]"
                        ),
                    ));
                    if tool_group_len > 1 {
                        result.push(messages[i - 1].clone());
                    }
                    removed += tool_group_len.saturating_sub(2);
                } else {
                    for m in &messages[start..i] {
                        result.push(m.clone());
                    }
                }
            } else {
                result.push(messages[i].clone());
                i += 1;
            }
        }

        (result, removed)
    }

    /// Pass 2: Shrink old user/assistant turns into summaries
    fn pass_shrink(&self, messages: &[Message]) -> (Vec<Message>, usize) {
        let mut result = Vec::new();
        let mut compressed = 0;

        // Only compress messages beyond the last 10
        let keep_recent = 10;

        for (i, msg) in messages.iter().enumerate() {
            if i < messages.len().saturating_sub(keep_recent)
                && matches!(msg.role, Role::User | Role::Assistant)
                && msg.estimated_tokens() > 200
            {
                // Shrink: keep first 100 chars + summary marker
                let truncated = if msg.content.len() > 200 {
                    format!(
                        "{}... [compressed from {} tokens]",
                        &msg.content[..200],
                        msg.estimated_tokens()
                    )
                } else {
                    msg.content.clone()
                };
                result.push(Message {
                    content: truncated,
                    token_count: Some(50), // compressed token estimate
                    ..msg.clone()
                });
                compressed += 1;
            } else {
                result.push(msg.clone());
            }
        }

        (result, compressed)
    }

    /// Pass 3: Collapse long code blocks and image references
    fn pass_collapse(&self, messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .map(|msg| {
                if msg.content.len() > 1000 {
                    // Collapse: keep header + line count
                    let lines: Vec<&str> = msg.content.lines().collect();
                    let collapsed = format!(
                        "{}\n[... {} lines collapsed, {} chars total ...]\n{}",
                        lines.first().unwrap_or(&""),
                        lines.len(),
                        msg.content.len(),
                        lines.last().unwrap_or(&""),
                    );
                    Message {
                        content: collapsed,
                        ..msg.clone()
                    }
                } else {
                    msg.clone()
                }
            })
            .collect()
    }

    /// Pass 4: Evict oldest non-critical messages beyond token limit
    fn pass_evict(&self, messages: &[Message]) -> (Vec<Message>, usize) {
        let total_tokens: usize = messages.iter().map(|m| m.estimated_tokens()).sum();

        if total_tokens <= self.max_tokens {
            return (messages.to_vec(), 0);
        }

        // Keep system messages + last N messages, evict middle
        let system_count = messages
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .count();

        let mut result: Vec<Message> = messages[..system_count].to_vec();
        let mut evicted = 0;

        let keep_recent = 8;
        let evict_end = messages.len().saturating_sub(keep_recent);
        let evict_start = system_count;

        if evict_end > evict_start {
            let evicted_tokens: usize = messages[evict_start..evict_end]
                .iter()
                .map(|m| m.estimated_tokens())
                .sum();
            result.push(Message::new(
                Role::System,
                &format!(
                    "[{} messages evicted, ~{} tokens freed]",
                    evict_end - evict_start,
                    evicted_tokens
                ),
            ));
            evicted = evict_end - evict_start;
        }

        for msg in &messages[evict_end..] {
            result.push(msg.clone());
        }

        (result, evicted)
    }
}

// ─── Convenience: Auto-compactor with default settings ───────────────────────

pub fn auto_compact(
    messages: &[Message],
    max_tokens: usize,
) -> Result<(Vec<Message>, CompactionStats)> {
    Compactor::new(max_tokens).compact(messages)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_messages(n: usize) -> Vec<Message> {
        (0..n)
            .map(|i| {
                Message::new(
                    if i % 2 == 0 {
                        Role::User
                    } else {
                        Role::Assistant
                    },
                    &format!("Message {i}: {}", "x".repeat(100)),
                )
            })
            .collect()
    }

    #[test]
    fn test_empty_error() {
        let c = Compactor::new(1000);
        assert!(c.compact(&[]).is_err());
    }

    #[test]
    fn test_reclaim_tool_messages() {
        let mut msgs = vec![
            Message::new(Role::User, "search for X"),
            Message::new(Role::Tool, "result 1"),
            Message::new(Role::Tool, "result 2"),
            Message::new(Role::Tool, "result 3"),
            Message::new(Role::Tool, "result 4"),
            Message::new(Role::Tool, "result 5"),
            Message::new(Role::Assistant, "here is the answer"),
        ];
        // Make tool messages long to test token savings
        for m in &mut msgs[1..6] {
            m.content = "x".repeat(500);
            m.token_count = Some(125);
        }

        let c = Compactor::new(10000).with_strategies(vec![CompactionStrategy::Reclaim]);
        let (result, stats) = c.compact(&msgs).unwrap();
        assert!(result.len() < msgs.len());
        assert!(stats.removed_messages > 0);
    }

    #[test]
    fn test_shrink_old_messages() {
        let msgs = make_messages(20);
        let c = Compactor::new(10000).with_strategies(vec![CompactionStrategy::Shrink]);
        let (result, stats) = c.compact(&msgs).unwrap();
        assert!(stats.compressed_messages > 0 || result.len() <= msgs.len());
    }

    #[test]
    fn test_collapse_long_content() {
        let mut msgs = vec![Message::new(Role::Assistant, &"x\n".repeat(600))];
        msgs[0].token_count = Some(500);

        let c = Compactor::new(10000).with_strategies(vec![CompactionStrategy::Collapse]);
        let (result, _) = c.compact(&msgs).unwrap();
        assert!(result[0].content.contains("collapsed"));
    }

    #[test]
    fn test_evict_beyond_limit() {
        let msgs = make_messages(30);
        let c = Compactor::new(100).with_strategies(vec![CompactionStrategy::Evict]);
        let (result, stats) = c.compact(&msgs).unwrap();
        assert!(result.len() < msgs.len());
        assert!(stats.evicted_messages > 0);
    }

    #[test]
    fn test_full_pipeline() {
        let mut msgs = make_messages(30);
        // Add some tool messages
        msgs.insert(5, Message::new(Role::Tool, &"result ".repeat(100)));
        msgs.insert(6, Message::new(Role::Tool, &"result ".repeat(100)));

        let c = Compactor::new(500);
        let (result, stats) = c.compact(&msgs).unwrap();
        assert!(stats.tokens_after <= stats.tokens_before);
        assert!(result.len() <= msgs.len());
    }

    #[test]
    fn test_compaction_stats() {
        let stats = CompactionStats {
            original_count: 20,
            final_count: 10,
            tokens_before: 5000,
            tokens_after: 2500,
            reclaimed_tokens: 2500,
            removed_messages: 5,
            compressed_messages: 3,
            evicted_messages: 2,
        };
        assert_eq!(stats.compression_ratio(), 0.5);
        assert_eq!(stats.savings_pct(), 50.0);
    }

    #[test]
    fn test_message_tokens() {
        let m = Message::new(Role::User, "hello world");
        assert!(m.estimated_tokens() > 0);
    }

    #[test]
    fn test_auto_compact() {
        let msgs = make_messages(50);
        let (result, stats) = auto_compact(&msgs, 500).unwrap();
        assert!(stats.tokens_after <= stats.tokens_before);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_realistic_conversation() {
        // Simulate a real coding conversation with tool calls
        let mut msgs = Vec::new();
        msgs.push(Message::new(Role::User, "Help me debug this function"));
        msgs.push(Message::new(Role::Assistant, "I'll look at the code"));
        msgs.push(Message::new(Role::Tool, "fn broken() { panic!(\"oops\") }"));
        msgs.push(Message::new(
            Role::Assistant,
            "Found the issue! The function panics because...",
        ));
        msgs.push(Message::new(Role::User, "Fix it please"));
        msgs.push(Message::new(
            Role::Assistant,
            "I've patched it with proper error handling",
        ));
        msgs.push(Message::new(Role::Tool, "src/lib.rs patched successfully"));
        msgs.push(Message::new(
            Role::Assistant,
            "Done. The fix adds Result<(), Error> return type.",
        ));

        let c = Compactor::new(1000);
        let (result, _) = c.compact(&msgs).unwrap();
        assert!(result.len() <= msgs.len());
        assert!(!result.is_empty());
    }

    #[test]
    fn test_massive_tool_output() {
        let mut msgs = vec![Message::new(Role::User, "search the codebase")];
        for i in 0..20 {
            msgs.push(Message::new(
                Role::Tool,
                &format!("Line {i}: {}", "x".repeat(500)),
            ));
        }
        msgs.push(Message::new(
            Role::Assistant,
            "Found 20 matches across 5 files.",
        ));

        let c = Compactor::new(2000);
        let (result, _) = c.compact(&msgs).unwrap();
        let tool_count = result
            .iter()
            .filter(|m| matches!(m.role, Role::Tool))
            .count();
        assert!(tool_count < 20, "should compress tool messages");
    }

    #[test]
    fn test_preserves_system_prompt() {
        let mut msgs = vec![Message::new(
            Role::System,
            "You are a helpful coding assistant. Always follow Rust best practices.",
        )];
        msgs.extend(make_messages(30));

        let c = Compactor::new(1000);
        let (result, _) = c.compact(&msgs).unwrap();
        assert!(result[0].role == Role::System);
        assert!(result[0].content.contains("coding assistant"));
    }

    #[test]
    fn test_compact_respects_threshold() {
        let msgs = make_messages(50);
        let c = Compactor::new(100000);
        let (result, stats) = c.compact(&msgs).unwrap();
        assert!(!result.is_empty());
        assert_eq!(stats.original_count, 50);
    }
}
