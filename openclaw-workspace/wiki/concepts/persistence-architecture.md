# Persistence Architecture

_Concept page — how Clawd survives session restarts._

## The Problem
Each session starts fresh. No built-in memory persistence. Without files, everything is lost.

## 6 Persistence Systems

1. **Daily notes** (`memory/YYYY-MM-DD.md`) — Raw logs of what happened
2. **Long-term memory** (`MEMORY.md`) — Curated, distilled knowledge
3. **Session state** (`.clawd-state.json`) — Runtime state restoration
4. **Metacognition** (`.clawd-metacognition.json`) — Self-monitoring layer
5. **Wiki** (`wiki/`) — Structured, interlinked knowledge base (Karpathy pattern)
6. **Heartbeat state** (`memory/heartbeat-state.json`) — Periodic check tracking

## Key Principles
- **Text > Brain**: If you want to remember it, write it to a file
- **Files survive, mental notes don't**: Every important decision → file
- **MEMORY.md = curated wisdom**: Not raw logs, distilled essence
- **Wiki = compounding artifact**: Cross-references built once, maintained incrementally

## What Gets Lost
- Chat history context (windowed)
- Intra-session reasoning (unless logged)
- Causal chains between sessions (unless documented)

## Anti-Patterns
- "I'll remember this" → No, you won't
- Not writing decisions down → Future-you has no idea
- Only raw logs without synthesis → Noise drowns signal

## See Also
- [llm-wiki-pattern](llm-wiki-pattern.md)
- [openclaw](../entities/openclaw.md)