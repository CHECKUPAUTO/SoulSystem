# SoulSystem Memory System

## Architecture

The memory system is organized in a hierarchy:

```
Working Memory (ring buffer, persisted via sled)
    ↓ consolidation
Episodic Memory (SQLite, with timestamp + decay)
    ↓ consolidation (24h or 1000 episodes)
Semantic Memory (sled vector store, SciRust embeddings)
```

## Vector Store (SoulMemory)

- Uses SciRust embeddings (64-dim by default)
- Stores in sled (embedded KV DB)
- Qdrant support via `QDRANT_URL` env var
- Cosine similarity search with pruning

## Key Components

- `SoulMemory` — Vector memory store with embedding-based search
- `PersistentStore` — Sled-backed persistence for working memory, goals, conversations, actions
- `ConversationStore` — SQLite-backed conversation history with sessions
- `KnowledgeGraph` — Typed node/edge graph with pathfinding
- `RagStore` — Web fetch + browser-based RAG with relevance scoring