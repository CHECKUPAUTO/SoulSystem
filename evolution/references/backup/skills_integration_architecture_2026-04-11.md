# Skills Integration Architecture Guide

**Generated:** 2026-04-11  
**Source:** night_cycle_20260411_0815.md  
**Category:** Architecture Documentation

## Current Skill Inventory

**Total Active Skills:** 18

| Skill | Type | Status | Evolution |
|-------|------|--------|-----------|
| py2rust | Transpiler | ✅ Active | Manual |
| notebooklm-kg | Knowledge Graph | ✅ Active | Manual |
| critical-guard | Security | ✅ Active | Manual |
| openclaw-doctor | Monitoring | ✅ Active | Cron (15min) |
| exec-evolved | Execution | ✅ Active | OpenEvolve |
| openclaw-enhanced | Enhancement | ✅ Active | Manual |
| graphify-parallel | Graph Analysis | ✅ Active | Manual |
| shannon-scraper | Web Scraping | ✅ Active | Manual |
| read-evolved | File Reading | ✅ Active | OpenEvolve |
| edit-evolved | File Editing | ✅ Active | OpenEvolve |
| image-reader | OCR/Vision | ✅ Active | Manual |
| phoneinfoga-mcp | MCP Server | ✅ Active | Manual |
| universal-file-reader | I/O | ✅ Active | Manual |
| gallery | Media | ✅ Active | Manual |
| shared | Utilities | ✅ Active | Manual |
| skill-types | Types | ✅ Active | Manual |
| scraper | Scraping | ✅ Active | Manual |
| mcp-builder | MCP Dev | ✅ Active | Manual |

## Deep Skill Analysis

### read-evolved

**Capabilities:**
- Multi-format support: text, images (OCR), PDFs, audio (transcription)
- Auto-detection via extension, mimetype, and content analysis
- Encoding auto-detection (UTF-8, Latin-1, CP1252 fallback)
- Batch processing with async support
- JSON structured output

**Strengths:**
- Clean type detection hierarchy
- Graceful error handling
- Modular design with `detect_file_type()` and `read_text()` functions

**Recommended Improvements:**
1. ✅ Add content caching layer for repeated reads (partially implemented)
2. ⏳ Implement incremental reading for large files
3. ⏳ Add content hashing for change detection
4. ⏳ Support more video format metadata extraction
5. ⏳ Add image thumbnail generation option (implemented in core)

### edit-evolved

**Capabilities:**
- Atomic edit operations with rollback
- Automatic timestamped backups
- Conflict detection via MD5 checksums
- Multi-file batch editing with glob patterns
- Structured edit specification (JSON-based)

**Strengths:**
- Strong safety guarantees (backup + atomicity)
- EditResult class for structured feedback
- `compute_checksum()` for conflict detection

**Recommended Improvements:**
1. ⏳ Add git integration for change tracking
2. ⏳ Implement semantic-aware editing (AST-based for code)
3. ⏳ Add preview/diff mode before applying
4. ✅ Support regex-based pattern matching (implemented)
5. ⏳ Add transaction support for multi-file edits
6. ⏳ Implement file locking for concurrent access

### exec-evolved

**Capabilities:**
- Advanced command execution with safety checks
- Timeout handling with retry logic
- Audit logging with rotation
- Circuit breaker pattern
- Execution metrics tracking

**Strengths:**
- Comprehensive safety features
- Exponential backoff with jitter
- Circuit breaker for external API calls

**Recommended Improvements:**
1. ✅ Circuit breaker pattern (implemented)
2. ✅ Execution metrics tracking (implemented)
3. ✅ Audit log rotation (implemented)
4. ⏳ Add Prometheus metrics export
5. ⏳ Implement distributed tracing support

### openclaw-doctor

**Capabilities:**
- Automated health monitoring
- Orphan session detection and cleanup
- Session transcript consistency checks
- Cron-based execution (15-minute intervals)
- Selective notification (only on errors)

**Strengths:**
- Non-intrusive operation (silent on success)
- Structured logging to memory/openclaw-doctor.log
- Automatic --fix execution

**Recommended Improvements:**
1. ⏳ Add metrics collection (session count trends)
2. ⏳ Implement predictive cleanup (pre-emptive maintenance)
3. ⏳ Add health score dashboard
4. ⏳ Integrate with Prometheus/Grafana
5. ⏳ Add session duration analytics

### py2rust

**Capabilities:**
- Universal Python-to-Rust transpilation
- Multi-source support: local, GitHub, PyPI, MCP, REST API, Docker
- 3-stage pipeline: Resolution → Analysis → Conversion
- Automatic dependency mapping (Python → Rust crates)
- Architecture scanning with AST analysis

**Strengths:**
- Comprehensive source resolution via universal_resolver.py
- Dependency mapping table (requests→reqwest, pandas→polars, etc.)
- Claude API integration for idiomatic Rust generation

**Recommended Improvements:**
1. ⏳ Add incremental transpilation (cache intermediate results)
2. ⏳ Implement parallel processing for large projects
3. ⏳ Add Rust compilation validation step
4. ⏳ Create test generation for transpiled code
5. ⏳ Add PyO3 bindings generation option
6. ⏳ Support for type stub generation

## Cross-Skill Integration Opportunities

### 1. Unified Skill Bus

**Concept:** Create a communication layer for inter-skill coordination.

**Use Cases:**
- read-evolved triggers edit-evolved for file modifications
- py2rust outputs directly to evolved skill structure
- exec-evolved notifies openclaw-doctor of long-running processes

**Implementation Sketch:**
```python
# shared/skill_bus.py
class SkillBus:
    def emit(self, event: str, data: dict):
        """Emit event to all subscribed skills."""
        pass
    
    def subscribe(self, skill: str, event: str, handler: Callable):
        """Subscribe a skill to specific events."""
        pass
```

### 2. Shared Caching Layer

**Concept:** Implement LRU cache shared across skills.

**Benefits:**
- File reads cached in read-evolved
- AST analysis results in py2rust
- Command execution results in exec-evolved

**Implementation Options:**
- In-memory dict with TTL
- Redis for distributed caching
- SQLite for persistent caching

### 3. Enhanced Error Recovery

**Concept:** Standardized error recovery patterns.

**Patterns:**
- Circuit breaker for external API calls (✅ exec-evolved)
- Graceful degradation for OCR failures
- Auto-retry with jitter for network operations
- Fallback chains for skill execution

## External Integration Opportunities

### VisionClaw Integration Study

**Analysis:**
- Real-time AI assistant for Meta Ray-Ban smart glasses
- Uses Gemini Live API + OpenClaw gateway
- Bidirectional audio streaming (PCM 16kHz in, 24kHz out)
- Camera streaming at ~1fps for visual context
- WebRTC live streaming for POV sharing

**Integration Points:**
- OpenClaw tool calling (56+ skills available)
- Voice wake word detection
- POV streaming for visual context

**Reference Architecture:**
```
VisionClaw → Gemini Live API → OpenClaw Gateway
     ↑                            ↓
   Wearables                  Tool Execution
     SDK                         Layer
```

### PolymathicAI the_well Connector

**Analysis:**
- 15TB Collection of Physics Simulation Datasets
- HDF5 + YAML metadata, fp32 numpy arrays
- Focus: Physics-informed machine learning

**Proposed Skill:**
```typescript
@skill.command()
async query_physics_dataset(
  dataset: string,
  field: string,
  trajectory: number
) {
  const well = new TheWellAPI();
  const data = await well.load(dataset, { split: 'train' });
  return await data[trajectory][field].visualize();
}
```

**Use Cases:**
- Natural language queries to physics datasets
- Scientific visualization through OpenClaw
- Bridge to Hugging Face datasets

## Implementation Priority Matrix

| Priority | Skill | Enhancement | Effort | Impact |
|----------|-------|-------------|--------|--------|
| 🔴 P0 | All | Unified skill bus | High | High |
| 🔴 P0 | read-evolved | Content caching | Medium | High |
| 🟡 P1 | exec-evolved | Prometheus metrics | Medium | Medium |
| 🟡 P1 | openclaw-doctor | Predictive cleanup | Medium | Medium |
| 🟡 P1 | edit-evolved | Git integration | Low | Medium |
| 🟢 P2 | py2rust | Incremental transpilation | High | Low |
| 🟢 P2 | All | Documentation sync | Medium | Low |

## Next Steps

1. ⏳ Implement cross-skill integration layer
2. ⏳ Design shared caching infrastructure
3. ⏳ Create VisionClaw integration reference
4. ⏳ Prototype the_well connector
5. ⏳ Benchmark skill performance

---
*Generated by OpenEvolve Auto-Apply*  
*Based on night cycle analysis of 18 active skills*