# Dreaming / Long-Term Memory Architecture

**Based on OpenEvolve Night Cycle Reports 2026-04-11 through 2026-04-13**  
**Source:** OpenClaw memory-wiki extension analysis

---

## Overview

The Dreaming subsystem represents a paradigm shift in OpenClaw - evolving from a reactive assistant to a **self-reflective agent with long-term memory (LTM)**. This cognitive augmentation layer enables persistent agent identity across sessions, proactive insight surfacing, and knowledge-based reasoning augmentation.

---

## Core Components

### 1. ChatGPT Export Import Pipeline
**Location:** `extensions/memory-wiki/src/chatgpt-import.ts` (903 lines)

An ETL pipeline for importing ChatGPT conversation history:
- Parses ChatGPT export JSON format
- Extracts conversations, themes, and knowledge
- Validates and transforms for OpenClaw consumption

```typescript
export type ChatGPTExport = {
  conversations: Conversation[];
  exportDate: Date;
  version: string;
};
```

### 2. Insight Extraction
**Location:** `extensions/memory-wiki/src/import-insights.ts`

Theme detection and knowledge graph construction:
- Automatically extracts recurring themes from conversations
- Builds knowledge graph nodes from insights
- Links related concepts for semantic retrieval

```typescript
export type Insight = {
  source: ChatGPTExport;
  extractedAt: Date;
  themes: string[];
  knowledgeGraph: Node[];
};
```

### 3. Memory Palace
**Location:** `extensions/memory-wiki/src/memory-palace.ts` (148 lines)

Spatial memory organization using the ancient Method of Loci technique:
- **Rooms:** Contain related memories (e.g., "work", "personal", "projects")
- **Associations:** Graph-based linking between memory nodes
- **Spatial Navigation:** Traverse memory by "walking" through rooms

```typescript
export type MemoryPalace = {
  rooms: Map<string, MemoryRoom>;
  associations: Graph<MemoryNode>;
  lastWalked: Date;
};

export type MemoryRoom = {
  id: string;
  name: string;
  memories: MemoryNode[];
  position: { x: number; y: number; z: number };
};
```

### 4. Dreaming UI
Complete CSS and view controller subsystem:
- `dreams.css` (241 lines): Themed styling for memory visualization
- `app-view-state.ts`: UI state management
- `dreaming.ts`: Interactive memory navigation controller

---

## Security Considerations

⚠️ **PII Warning:** ChatGPT exports may contain personally identifiable information.

**Required Safeguards:**
1. **Data Retention Policies:** Auto-expire old memories based on user preferences
2. **Encryption at Rest:** Memory storage should be encrypted
3. **User Consent Flows:** Explicit opt-in for import and storage
4. **Access Controls:** Memory should respect channel allowlists

---

## Integration Points

### Gateway Methods
Server-side import endpoints for batch processing:
```typescript
// POST /api/memory/import
async function importChatGPTExport(exportData: ChatGPTExport): Promise<ImportResult>

// GET /api/memory/search?q={query}
async function searchMemories(query: string): Promise<MemoryNode[]>
```

### CLI Commands
Batch operations via command line:
```bash
# Import ChatGPT export
openclaw memory import --file conversations.json --format chatgpt

# Rebuild knowledge graph
openclaw memory rebuild-graph

# Export memories
openclaw memory export --format markdown --output memories.md
```

### UI Controllers
Interactive session management:
- Memory browser with search/filter
- Knowledge graph visualization
- Theme clustering view
- Room navigation interface

---

## Neural Significance

The Dreaming subsystem enables **LTM (Long-Term Memory)** for agents:

| Feature | Benefit |
|---------|---------|
| Persistent Identity | Agent remembers user preferences across sessions |
| Proactive Insights | Surface relevant memories without explicit query |
| Knowledge Augmentation | Enrich responses with historical context |
| Spatial Memory | Natural memory retrieval via spatial navigation |

---

## Future Enhancements

### High Priority
1. **Memory Palace Query Optimization**
   - Current: Likely naive graph traversal
   - Consider: Vector index for semantic search
   - Benchmark: Query latency vs. memory size

2. **Dreaming Sync Protocol**
   - Real-time memory updates across sessions
   - Conflict resolution for concurrent edits
   - Versioning for memory snapshots

### Medium Priority
3. **Export Format Expansion**
   - Beyond ChatGPT: Claude, Gemini, OpenClaw native
   - Generic conversation JSON schema
   - Plugin architecture for custom importers

4. **Security Hardening**
   - Automated PII detection and scrubbing
   - Memory encryption at rest
   - Audit logging for memory access

### Low Priority
5. **Dreaming Visualization**
   - 3D memory palace navigation
   - Knowledge graph explorer
   - Theme clustering heatmaps

---

## References

- OpenClaw Commit: `64693d2e` - Dreaming feature merge
- Memory Palace Technique: https://en.wikipedia.org/wiki/Method_of_loci
- LTM Research: https://openclaw.ai/docs/ltm

---

*Generated from OpenEvolve Night Cycle Report*
*Date: 2026-04-11*

---

## Dreaming UI: Phase-Aware Memory Management (Updated 2026-04-13)

**Source:** OpenEvolve Night Cycle Reports 2026-04-13 00:30, 00:34

The Dreaming subsystem has matured with a phase-aware UI that mirrors cognitive science stages of memory consolidation:

### Phase Labels

| Phase | Description | Cognitive Analog |
|-------|-------------|------------------|
| `waiting` | Queued memories awaiting processing | Encoding |
| `processing` | Active dreaming/consolidation | Consolidation |
| `reviewing` | Advanced review tab for curation | Reconsolidation |
| `integrating` | Writing back to memory wiki | Storage |

### Recent UI Improvements (April 10-12, 2026)
- Advanced review tab for memory curation (`f479ab1498` through `cc387edf87`)
- Diary navigation with phase labels and sorting by recency
- Unknown phase state preservation (handles edge cases gracefully)
- i18n support for phase labels
- Memory wiki and dreaming check restoration (`279cbfc61c`)

### Proposed: Dream Quality Metric

See `evolution/references/dream_quality_metric.md` for the full proposal — a 1-5 self-assessment score per dreaming session to create a feedback loop for tuning dreaming parameters.

### Integration with Memory Palace

The "advanced review" tab in the Dreaming UI is essentially the Memory Palace's "locus navigation" concept. Future work should unify these UI components:
- Memory Palace spatial navigation ↔ Dreaming phase review
- Locus creation from dream insights
- Dream quality as locus selection criteria

