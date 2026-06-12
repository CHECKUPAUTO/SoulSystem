# LLM-Wiki Pattern

_Concept page — Karpathy's persistent knowledge base pattern._

## Source
https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f

## Core Idea
Instead of RAG (retrieve chunks from raw docs at query time), the LLM incrementally builds and maintains a **persistent wiki** — a structured, interlinked collection of markdown files between you and raw sources.

The key difference: **knowledge is compiled once and kept current, not re-derived on every query.**

## Three Layers
1. **Raw sources** — Immutable source documents. LLM reads but never modifies.
2. **The wiki** — LLM-owned markdown files. Summaries, entity pages, concept pages, cross-references. LLM writes; human reads.
3. **The schema** — A document (CLAUDE.md/AGENTS.md) that tells the LLM wiki conventions and workflows. Co-evolved with the LLM.

## Three Operations
1. **Ingest** — Drop source → LLM reads, extracts, integrates into wiki (10-15 pages touched per source). Updates index, entities, concepts, appends to log.
2. **Query** — Ask question → LLM searches wiki, synthesizes answer. **Key: good answers get filed back as new pages.** Explorations compound.
3. **Lint** — Periodic health-check: contradictions, stale claims, orphan pages, missing cross-references, data gaps. LLM suggests new questions/sources.

## Navigation
- **index.md** — Content-oriented catalog (page, summary, metadata). Updated on every ingest.
- **log.md** — Chronological, append-only. Parseable with unix tools (grep, tail).

## Tools
- **qmd** — Local search (BM25/vector hybrid + LLM re-ranking). CLI + MCP server.
- **Obsidian Web Clipper** — Browser extension → markdown.
- **Obsidian Graph View** — Visualize wiki shape.

## Key Insight
> The wiki is a persistent, compounding artifact. Cross-references are already there. Contradictions already flagged. Synthesis reflects everything read. It keeps getting richer with every source and every question.

## Our Implementation
- Directory: `wiki/` with raw/, entities/, concepts/, synthesis/
- Schema: AGENTS.md (wiki conventions section)
- Index: wiki/index.md
- Log: wiki/log.md
- Lint: integrated into heartbeat cycle

## See Also
- [persistence-architecture](persistence-architecture.md)