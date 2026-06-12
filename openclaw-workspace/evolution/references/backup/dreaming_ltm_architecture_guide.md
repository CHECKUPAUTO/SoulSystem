# Dreaming / LTM Architecture Guide

**Source:** Night Cycle Analysis (2026-04-11)
**Status:** Feature analysis and recommendations

---

## Overview

The `memory-wiki` extension represents OpenClaw's evolution toward proactive, self-reflective agent capabilities with **Long-Term Memory (LTM)**.

## Components

### 1. Import Pipeline (`chatgpt-import.ts`)
- **Purpose:** ETL for ChatGPT conversation history
- **Size:** 903 lines
- **Features:**
  - JSON export parsing
  - URL normalization
  - PII handling considerations

### 2. Insight Extraction (`import-insights.ts`)
```typescript
export type Insight = {
  source: ChatGPTExport;
  extractedAt: Date;
  themes: string[];
  knowledgeGraph: Node[];
};
```
- Theme detection
- Knowledge graph construction
- Semantic clustering

### 3. Memory Palace (`memory-palace.ts`)
```typescript
export type MemoryPalace = {
  rooms: Map<string, MemoryRoom>;
  associations: Graph<MemoryNode>;
  lastWalked: Date;
};
```

**Cognitive Model:** Spatial organization for retrieval - a technique used by memory champions since antiquity.

### 4. Dreaming UI (`views/dreaming.ts`)
- Interactive memory navigation
- CSS subsystem (`dreams.css`, 241 lines)
- State management via `app-view-state.ts`

---

## Security Considerations

### Data Protection
ChatGPT exports may contain PII. The import pipeline should implement:

1. **Data Retention Policies**
   - Auto-expiry for imported conversations
   - User-configurable retention periods

2. **Encryption at Rest**
   - Memory palace encryption
   - Secure key storage

3. **User Consent Flows**
   - Explicit import confirmation
   - Granular selection of conversations

### Implementation Pattern
```typescript
interface MemorySecurityPolicy {
  encryption: 'aes-256-gcm' | 'none';
  retentionDays: number;
  allowSensitiveContent: boolean;
  autoScrubPII: boolean;
}
```

---

## Integration Points

### Gateway Methods
- Server-side import processing
- Batch operations
- Status queries

### CLI Commands
```bash
openclaw memory import --source chatgpt --file export.json
openclaw memory palace walk --theme "project-foo"
openclaw memory query --text "how did I solve X?"
```

### UI Controllers
- Interactive memory navigation
- Knowledge graph visualization
- Theme clustering heatmaps

---

## Neural Context

**Science Node Pressure:** 38.6% (systematic knowledge organization)
**Engineer Node Pressure:** 34.7% (heavy architectural construction)

This indicates systematic knowledge organization with heavy architectural construction - emergence rather than maintenance.

---

## Recommendations

### Immediate (This Week)
1. Document LTM Architecture in VISION.md
2. Security audit for ChatGPT export handling
3. Add memory metrics to `/status` endpoint

### Short Term (This Month)
4. Memory Palace Graph Query Optimization
   - Consider vector index for semantic search
   - Benchmark: Query latency vs. memory size

5. Dreaming Sync Protocol
   - Real-time memory updates across sessions
   - Conflict resolution for concurrent edits
   - Versioning for memory snapshots

### Long Term (This Quarter)
6. Export Format Expansion
   - Claude, Gemini, OpenClaw native
   - Generic conversation JSON schema
   - Plugin architecture for custom importers

7. Dreaming Visualization
   - 3D memory palace navigation
   - Knowledge graph explorer
   - Theme clustering heatmaps

---

## Predictive Insights

Based on commit patterns and neural pressure:

1. **Next 48h:** Continued Dreaming UI polish
2. **Next Week:** Memory palace query optimization
3. **Next Month:** LTM integration with agent loops (proactive memory surfacing)

**Foundation for:**
- Persistent agent identity across sessions
- Proactive insight surfacing
- Knowledge-based reasoning augmentation

---

## References

- Night Cycle: night_cycle_20260411_0732.md
- Related: `dreaming_ltm_architecture.md`
- Memory Palace Technique: https://en.wikipedia.org/wiki/Method_of_loci
