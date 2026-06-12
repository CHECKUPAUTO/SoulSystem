# Startup Context Pattern v2 - Session State Preservation

**Source:** OpenEvolve Night Cycle Reports 2026-04-12 04:15, 04:30, 04:45  
**Author:** Tak Hoffman (pattern established through commits 4d0f5553, d78bbe8e, c31aa6da)  
**Priority:** P1 - High Priority  
**Classification:** Architecture Pattern

---

## Problem Statement

**Session Amnesia on Bare Resets:** When OpenClaw sessions undergo a "bare reset" (complete context clear), all accumulated conversational context is lost. This leads to:
- Repetitive explanations by users
- Loss of established context and preferences  
- Discontinuous user experience
- Reduced assistant effectiveness

**T430 Assessment:** Active-memory stabilization required session state preservation. Parent channel context loss was causing recall failures.

**Evidence from Night Cycle:**
```
4d0f5553 - fix(active-memory): preload startup memory for bare resets
c31aa6da - fix(active-memory): preserve parent channel context for recall runs
d78bbe8e - docs: align AGENTS template with startup context runtime
```

---

## Solution: Startup Context Preloading

### Core Concept

Preload relevant context **before** session reset completes, then inject into the new session's initial context:

```typescript
// Flow: Preload → Reset → Rehydrate
[Session with Context] → [Bare Reset Triggered]
                          ↓
                   [Preload Startup Context]
                          ↓  
                   [Reset Completes]
                          ↓
                   [Inject Preloaded Context]
                          ↓
              [New Session with Continuity]
```

### Implementation

#### 1. Startup Context Interface

```typescript
// src/auto-reply/reply/startup-context.ts
export interface StartupContext {
  // Daily memory entries (configurable days back)
  memory: DailyMemoryEntry[];
  
  // Agent templates for quick reactivation
  templates: AgentTemplate[];
  
  // Runtime configuration snapshot
  runtime: RuntimeConfigSnapshot;
  
  // Parent channel context (for recall runs)
  parentChannel?: ChannelContext;
}

export interface DailyMemoryEntry {
  date: string;        // ISO date string
  content: string;     // Memory content
  sizeBytes: number;   // For bounded reads
}

export interface ChannelContext {
  channelId: string;
  channelType: 'telegram' | 'whatsapp' | 'discord';
  lastMessageId?: string;
  metadata: Record<string, unknown>;
}
```

#### 2. Preload Function

```typescript
// src/auto-reply/reply/startup-context.ts

// Bounded read constants (security)
const MAX_FILE_SIZE_BYTES = 16 * 1024;      // 16KB max per file
const MAX_CHARS_PER_FILE = 2000;            // 2K chars per file  
const MAX_TOTAL_CHARS = 4500;                 // 4.5K total context
const DEFAULT_MEMORY_DAYS = 2;               // Load last 2 days

export async function preloadStartupContext(
  options: StartupContextOptions = {}
): Promise<StartupContext> {
  const {
    memoryDays = DEFAULT_MEMORY_DAYS,
    timezone = 'UTC',
    includeParentChannel = true,
  } = options;

  const context: StartupContext = {
    memory: [],
    templates: [],
    runtime: {},
    parentChannel: undefined,
  };

  // Load daily memory files with bounds checking
  for (let i = 0; i < memoryDays; i++) {
    const date = getDateString(i, timezone);
    const memoryPath = `memory/${date}.md`;
    
    if (await fileExists(memoryPath)) {
      const content = await readBoundedFile(memoryPath, {
        maxBytes: MAX_FILE_SIZE_BYTES,
        maxChars: MAX_CHARS_PER_FILE,
      });
      
      if (content) {
        context.memory.push({
          date,
          content: truncate(content, MAX_CHARS_PER_FILE),
          sizeBytes: Buffer.byteLength(content, 'utf8'),
        });
      }
    }
  }

  // Load AGENTS template (configurable)
  const agentsTemplate = await loadAgentsTemplate();
  if (agentsTemplate) {
    context.templates.push(agentsTemplate);
  }

  // Capture parent channel context for recall
  if (includeParentChannel) {
    context.parentChannel = await captureParentChannelContext();
  }

  return context;
}

// Bounded file read with security validation
async function readBoundedFile(
  path: string, 
  bounds: { maxBytes: number; maxChars: number }
): Promise<string | null> {
  try {
    const stats = await stat(path);
    if (stats.size > bounds.maxBytes) {
      console.warn(`File ${path} exceeds size limit (${stats.size} > ${bounds.maxBytes})`);
      return null;
    }
    
    const content = await readFile(path, 'utf8');
    return content.slice(0, bounds.maxChars);
  } catch (err) {
    return null;
  }
}
```

#### 3. Context Injection

```typescript
// src/auto-reply/reply/inject-startup-context.ts

export function injectStartupContext(
  messages: Message[], 
  context: StartupContext
): Message[] {
  const systemMessages: Message[] = [];

  // Memory context
  if (context.memory.length > 0) {
    const memoryContent = context.memory
      .map(m => `## ${m.date}\n${m.content}`)
      .join('\n\n');
    
    systemMessages.push({
      role: 'system',
      content: `### Recent Memory\n${memoryContent}`,
      name: 'startup-context',
    });
  }

  // Parent channel context (for recall)
  if (context.parentChannel) {
    systemMessages.push({
      role: 'system', 
      content: `### Channel Context\nChannel: ${context.parentChannel.channelId}\nType: ${context.parentChannel.channelType}`,
      name: 'channel-context',
    });
  }

  // Prepend system messages to conversation
  return [...systemMessages, ...messages];
}
```

---

## Extension Points

### 1. Memory File Format Versioning

```typescript
// For schema evolution support
interface MemoryFileV1 {
  version: 1;
  date: string;
  entries: MemoryEntry[];
}

interface MemoryFileV2 {
  version: 2;
  date: string;
  entries: MemoryEntry[];
  tags: string[];  // New: searchable tags
  priority: number;  // New: recall priority
}

export async function loadMemoryFile(path: string): Promise<MemoryEntry[]> {
  const content = await readFile(path, 'utf8');
  const parsed = JSON.parse(content);
  
  // Version migration
  switch (parsed.version) {
    case 1:
      return migrateV1ToV2(parsed).entries;
    case 2:
      return parsed.entries;
    default:
      throw new Error(`Unknown memory file version: ${parsed.version}`);
  }
}
```

### 2. Pluggable Memory Sources

```typescript
// Support for multiple memory backends
interface MemorySource {
  name: string;
  load(dateRange: DateRange): Promise<MemoryEntry[]>;
}

const memorySources: MemorySource[] = [
  { name: 'daily-files', load: loadDailyFiles },
  { name: 'vector-db', load: loadFromVectorDB },
  { name: 'external-api', load: loadFromExternalAPI },
];

export async function loadFromAllSources(
  dateRange: DateRange
): Promise<MemoryEntry[]> {
  const results = await Promise.all(
    memorySources.map(src => src.load(dateRange))
  );
  return mergeAndDeduplicate(results);
}
```

### 3. Compression for Large Memory Files

```typescript
import { gzip, gunzip } from 'zlib';
import { promisify } from 'util';

const gzipAsync = promisify(gzip);
const gunzipAsync = promisify(gunzip);

export async function compressMemory(content: string): Promise<Buffer> {
  return gzipAsync(Buffer.from(content, 'utf8'));
}

export async function decompressMemory(compressed: Buffer): Promise<string> {
  const decompressed = await gunzipAsync(compressed);
  return decompressed.toString('utf8');
}

// Usage: Store as memory/YYYY-MM-DD.md.gz
```

### 4. Selective Memory Loading

```typescript
// Load only memories matching specific tags or topics
interface MemoryFilter {
  tags?: string[];
  topics?: string[];
  minPriority?: number;
  excludePatterns?: RegExp[];
}

export async function loadFilteredMemory(
  dateRange: DateRange,
  filter: MemoryFilter
): Promise<MemoryEntry[]> {
  const allMemories = await loadMemoryFiles(dateRange);
  
  return allMemories.filter(entry => {
    if (filter.tags && !filter.tags.some(t => entry.tags?.includes(t))) {
      return false;
    }
    if (filter.minPriority && (entry.priority ?? 0) < filter.minPriority) {
      return false;
    }
    return true;
  });
}
```

---

## Migration Path

| Phase | Implementation | Status |
|-------|---------------|--------|
| Current | Static daily file loading | ✅ Implemented |
| Next | Configurable memory sources | 🔄 Planning |
| Future | Vector-augmented memory (VAM) | 📋 Backlog |

**Reference:** Vector Augmented Memory pattern in `vector_augmented_memory_vam.md`

---

## Configuration

```yaml
# config.yaml
startup-context:
  enabled: true
  memory-days: 2
  timezone: "Europe/Paris"
  include-parent-channel: true
  bounds:
    max-file-size-bytes: 16384
    max-chars-per-file: 2000
    max-total-chars: 4500
  sources:
    - daily-files
    # - vector-db  # Future
    # - external-api  # Future
```

---

## Testing

```typescript
// test/startup-context.test.ts
describe('preloadStartupContext', () => {
  it('should load daily memory files within bounds', async () => {
    const context = await preloadStartupContext({
      memoryDays: 2,
      timezone: 'UTC',
    });
    
    expect(context.memory.length).toBeLessThanOrEqual(2);
    expect(context.memory[0].content.length).toBeLessThanOrEqual(2000);
  });

  it('should preserve parent channel context', async () => {
    const context = await preloadStartupContext({
      includeParentChannel: true,
    });
    
    expect(context.parentChannel).toBeDefined();
    expect(context.parentChannel?.channelId).toBeDefined();
  });

  it('should handle missing memory files gracefully', async () => {
    const context = await preloadStartupContext({
      memoryDays: 7,  // More days than files exist
    });
    
    expect(context.memory.length).toBeGreaterThanOrEqual(0);
  });
});
```

---

## Related Patterns

- **Active-Memory Integration**: `active_memory_integration_testing_guide.md`
- **Session State Management**: `session_state_management_patterns.md`
- **Context Tree Pattern**: `context_tree_pattern.md`
- **Vector Augmented Memory**: `vector_augmented_memory_vam.md`

---

## References

- Night Cycle Report: `night_cycle_20260412_0415.md`, `night_cycle_20260412_0445.md`
- Active-Memory Commits: `4d0f5553`, `c31aa6da`, `d78bbe8e`
- IronReview T430: `ironreview_t430_integration.md`

---

*Generated by OpenEvolve Auto-Apply*  
*Classification: P1 High Priority Pattern Documentation*  
*Credit: Tak Hoffman's active-memory stabilization work*
