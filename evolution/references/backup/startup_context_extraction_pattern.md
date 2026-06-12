# Startup Context Extraction Pattern

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:30 UTC  
**Priority:** P2 - High  
**Use Case:** Session state preservation across restarts via startup preloading

---

## Problem Statement

Session resets lose startup context, causing:
- Loss of user preferences and templates on restart
- Bare sessions starting without memory
- Inconsistent state between fresh and resumed sessions

**Evidence:**
- Commit 4d0f5553: "fix: preload startup memory for bare session resets"
- 15 files changed, +474 lines
- New `startup-context.ts` module created

---

## Solution: Explicit Startup Context Preloading

Separate startup context from runtime state for predictable session initialization:

```
┌─────────────────────────────────────────────────────────────┐
│                     Startup Context                          │
├─────────────────────────────────────────────────────────────┤
│  Memory        Templates        Runtime                      │
│  Entries       (AGENTS)         Config                       │
├─────────────────────────────────────────────────────────────┤
│  Preloaded at boot → Available for bare session resets       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                    Session Initialization                    │
├─────────────────────────────────────────────────────────────┤
│  Bare Reset → Load Startup Context → Hydrate Session         │
│  Fresh      → Load Startup Context → Initialize New          │
│  Resume     → Load Saved State   → Restore                 │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation

### Core Types

```typescript
// src/startup-context/types.ts

/**
 * Context preloaded at startup for session resets
 */
export interface StartupContext {
  /**
   * Recent memory entries available immediately
   */
  memory: MemoryEntry[];
  
  /**
   * Agent templates (AGENTS.md, etc.)
   */
  templates: AgentTemplate[];
  
  /**
   * Runtime configuration snapshot
   */
  runtime: RuntimeConfig;
  
  /**
   * When this context was generated
   */
  generatedAt: Date;
  
  /**
   * Context version for migrations
   */
  version: string;
}

/**
 * Individual memory entry
 */
export interface MemoryEntry {
  id: string;
  content: string;
  type: 'fact' | 'preference' | 'context';
  priority: number;
  timestamp: Date;
}

/**
 * Agent template structure
 */
export interface AgentTemplate {
  id: string;
  name: string;
  content: string;
  variables: string[];
}

/**
 * Runtime configuration snapshot
 */
export interface RuntimeConfig {
  model: string;
  maxTokens: number;
  temperature: number;
  // ... other config
}
```

### Preloader

```typescript
// src/startup-context/preloader.ts

import { StartupContext, MemoryEntry, AgentTemplate, RuntimeConfig } from './types';

/**
 * Preload startup context at boot time
 */
export async function preloadStartupContext(): Promise<StartupContext> {
  const startTime = performance.now();
  
  const [memory, templates, runtime] = await Promise.all([
    loadRecentMemory(),
    loadAgentTemplates(),
    loadRuntimeConfig()
  ]);
  
  const context: StartupContext = {
    memory,
    templates,
    runtime,
    generatedAt: new Date(),
    version: '1.0.0'
  };
  
  // Metrics
  metrics.record('startup_context_preload_ms', performance.now() - startTime);
  metrics.record('startup_context_memory_count', memory.length);
  
  return context;
}

async function loadRecentMemory(): Promise<MemoryEntry[]> {
  // Load from memory store
  const entries = await memoryStore.query({
    limit: 100,
    orderBy: 'timestamp',
    order: 'desc'
  });
  
  return entries.map(e => ({
    id: e.id,
    content: e.content,
    type: e.type,
    priority: e.priority,
    timestamp: new Date(e.timestamp)
  }));
}

async function loadAgentTemplates(): Promise<AgentTemplate[]> {
  // Load AGENTS.md and other templates
  const templates: AgentTemplate[] = [];
  
  // Main AGENTS.md
  const agentsMd = await fs.readFile(
    path.join(workspaceRoot, 'AGENTS.md'),
    'utf-8'
  );
  templates.push({
    id: 'agents-main',
    name: 'AGENTS.md',
    content: agentsMd,
    variables: extractVariables(agentsMd)
  });
  
  // Additional templates from templates/
  const templateDir = path.join(workspaceRoot, 'templates');
  const templateFiles = await glob('**/*.md', { cwd: templateDir });
  
  for (const file of templateFiles) {
    const content = await fs.readFile(path.join(templateDir, file), 'utf-8');
    templates.push({
      id: `template-${file}`,
      name: file,
      content,
      variables: extractVariables(content)
    });
  }
  
  return templates;
}

async function loadRuntimeConfig(): Promise<RuntimeConfig> {
  // Load from config store
  const config = await configStore.get('runtime');
  return {
    model: config.model ?? 'default',
    maxTokens: config.maxTokens ?? 4096,
    temperature: config.temperature ?? 0.7
  };
}

function extractVariables(content: string): string[] {
  // Extract {{variable}} patterns
  const matches = content.match(/\{\{(\w+)\}\}/g) ?? [];
  return matches.map(m => m.slice(2, -2));
}
```

### Session Reset Integration

```typescript
// src/session/session-reset.ts

import { StartupContext } from '../startup-context/types';

let startupContext: StartupContext | null = null;

/**
 * Initialize startup context at boot
 */
export async function initializeStartup(): Promise<void> {
  startupContext = await preloadStartupContext();
  logger.info(`Startup context loaded: ${startupContext.memory.length} memories`);
}

/**
 * Get startup context for session initialization
 */
export function getStartupContext(): StartupContext | null {
  return startupContext;
}

/**
 * Reset session with startup context preservation
 */
export async function resetSessionWithContext(
  sessionId: string
): Promise<Session> {
  const context = getStartupContext();
  
  if (!context) {
    logger.warn('No startup context available, creating bare session');
    return createBareSession(sessionId);
  }
  
  // Create session with preloaded context
  return createSession({
    id: sessionId,
    memory: context.memory,
    templates: context.templates,
    config: context.runtime
  });
}
```

---

## Bare Session Reset Flow

```
┌────────────────┐     ┌──────────────────┐     ┌────────────────┐
│   User Types   │────▶│  Detect Reset    │────▶│ Load Startup   │
│   /reset       │     │  Command         │     │ Context        │
└────────────────┘     └──────────────────┘     └────────────────┘
                                                       │
                                                       ▼
┌────────────────┐     ┌──────────────────┐     ┌────────────────┐
│  Ready with    │◄────│  Hydrate Memory  │◀────│ Create New     │
│  Context       │     │  & Templates    │     │ Session        │
└────────────────┘     └──────────────────┘     └────────────────┘
```

---

## Performance Considerations

### Preload Timing

```typescript
// src/main.ts

async function main() {
  // Start preloading early
  const startupPromise = initializeStartup();
  
  // Initialize other components in parallel
  await Promise.all([
    startupPromise,
    initializeGateway(),
    initializeChannels()
  ]);
  
  // Now ready for sessions
  await acceptSessions();
}
```

### Lazy Template Loading

```typescript
// For large template sets, load lazily
export async function getTemplate(id: string): Promise<AgentTemplate | null> {
  // Check cache first
  const cached = templateCache.get(id);
  if (cached) return cached;
  
  // Load from disk if not in startup context
  const template = await loadTemplateFromDisk(id);
  if (template) {
    templateCache.set(id, template);
  }
  
  return template;
}
```

---

## Testing

```typescript
// src/startup-context/preloader.test.ts

describe('preloadStartupContext', () => {
  it('should load memory, templates, and config', async () => {
    const context = await preloadStartupContext();
    
    expect(context.memory).toBeDefined();
    expect(context.templates).toBeDefined();
    expect(context.runtime).toBeDefined();
    expect(context.generatedAt).toBeInstanceOf(Date);
  });
  
  it('should extract template variables', async () => {
    const context = await preloadStartupContext();
    
    const agentsTemplate = context.templates.find(
      t => t.id === 'agents-main'
    );
    
    expect(agentsTemplate).toBeDefined();
    expect(agentsTemplate!.variables).toContain('userName');
  });
  
  it('should record preload metrics', async () => {
    await preloadStartupContext();
    
    const metricsData = metrics.getData();
    expect(metricsData).toHaveProperty('startup_context_preload_ms');
    expect(metricsData).toHaveProperty('startup_context_memory_count');
  });
});

describe('resetSessionWithContext', () => {
  it('should create session with startup context', async () => {
    await initializeStartup();
    
    const session = await resetSessionWithContext('test-session');
    
    expect(session.memory.length).toBeGreaterThan(0);
    expect(session.templates.length).toBeGreaterThan(0);
  });
  
  it('should handle missing startup context gracefully', async () => {
    // Clear startup context
    startupContext = null;
    
    const session = await resetSessionWithContext('bare-session');
    
    expect(session).toBeDefined();
    // Bare session created
  });
});
```

---

## Expected Benefits

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Session reset time | slow | fast | cached |
| Context loss | frequent | rare | reliable |
| User experience | jarring | seamless | improved |
| Testability | low | high | better |

---

## Integration with Session State Management

```typescript
// Combined with context-tree pattern
import { SessionContext, createRootContext } from '../context/SessionContext';
import { getStartupContext } from '../startup-context/preloader';

export async function initializeSession(
  sessionId: string
): Promise<SessionContext> {
  const startup = getStartupContext();
  
  if (!startup) {
    throw new Error('Startup context not initialized');
  }
  
  return createRootContext(
    sessionId,
    { channelId: 'default', channelType: 'telegram' },
    {
      entries: startup.memory,
      templates: startup.templates
    }
  );
}
```

---

## References

- Night Cycle Report: `night_cycle_20260412_0330.md`
- Commit: 4d0f5553 (Tak Hoffman)
- Session State Patterns: `session_state_management_patterns.md`
- Context Tree Pattern: `context_tree_pattern.md`

---

*Generated by OpenEvolve Night Cycle*  
*Classification: P2 Feature Pattern*
