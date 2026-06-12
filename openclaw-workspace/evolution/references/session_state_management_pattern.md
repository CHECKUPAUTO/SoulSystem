# Session State Management Pattern

**Pattern ID:** SESSION-STATE-PERSISTENCE  
**Origin:** OpenEvolve Night Cycle 2026-04-12 (GitHub Issue #64687)  
**Status:** Ready for Implementation  
**Classification:** P0 - Critical Gateway Pattern

## Description

The Session State Management Pattern provides a robust mechanism for persisting and restoring agent session state across gateway restarts. This pattern prevents session loss, maintains conversation continuity, and enables crash recovery for long-running agent sessions.

## Problem Context

**Issue:** Gateway restarts cause complete session loss  
**Impact:** Users lose conversation context, tool state, and agent configuration  
**Severity:** P0 - Critical

### Before: Session Loss on Restart

```typescript
// Session exists only in memory
class SessionManager {
  private sessions = new Map<string, Session>();  // Lost on restart!
  
  async restart(): Promise<void> {
    // All sessions lost - no persistence
    this.sessions.clear();
  }
}
```

## Solution Architecture

### Core Pattern: Serialize → Store → Restore

```typescript
// Session lifecycle with persistence
export interface PersistentSessionManager {
  // Running Sessions → Serialize
  snapshot(session: Session): SessionSnapshot;
  
  // Serialize → Store
  persist(snapshot: SessionSnapshot): Promise<void>;
  
  // Store → Restore
  restore(sessionId: string): Promise<Session>;
  
  // Rehydrate with TTL validation
  validate(snapshot: SessionSnapshot): boolean;
}
```

## Implementation

### Session Snapshot Schema

```typescript
export interface SessionSnapshot {
  // Identity
  sessionId: string;
  sessionKey: string;
  
  // Temporal
  createdAt: Date;
  lastActivityAt: Date;
  expiresAt: Date;
  
  // Content
  messageHistory: Message[];
  toolState: Record<string, unknown>;
  agentConfig: AgentConfigSnapshot;
  
  // Versioning for migrations
  version: number;
  
  // Integrity
  checksum: string;  // SHA-256 of serialized content
}

export interface AgentConfigSnapshot {
  model: string;
  temperature: number;
  maxTokens: number;
  systemPrompt?: string;
  tools: string[];  // Tool IDs only
  customConfig: Record<string, unknown>;
}

export interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string;
  toolCalls?: ToolCall[];
  toolResults?: ToolResult[];
  timestamp: Date;
  metadata: Record<string, unknown>;
}
```

### Persistent Session Store

```typescript
export class PersistentSessionStore {
  private store: KeyValueStore;  // Redis, SQLite, etc.
  private config: PersistenceConfig;
  
  constructor(store: KeyValueStore, config: PersistenceConfig) {
    this.store = store;
    this.config = config;
  }
  
  async persist(snapshot: SessionSnapshot): Promise<void> {
    // 1. Serialize to JSON
    const serialized = JSON.stringify(snapshot);
    
    // 2. Compute integrity checksum
    const checksum = computeChecksum(serialized);
    snapshot.checksum = checksum;
    
    // 3. Store with TTL
    const ttl = this.calculateTTL(snapshot);
    await this.store.set(
      `session:${snapshot.sessionId}`,
      serialized,
      { ttl }
    );
    
    // 4. Index for lookup
    await this.store.set(
      `session:key:${snapshot.sessionKey}`,
      snapshot.sessionId,
      { ttl }
    );
  }
  
  async restore(sessionId: string): Promise<Session | null> {
    // 1. Retrieve from store
    const serialized = await this.store.get(`session:${sessionId}`);
    if (!serialized) return null;
    
    // 2. Parse and validate
    const snapshot: SessionSnapshot = JSON.parse(serialized);
    
    // 3. Verify integrity
    if (!this.verifyIntegrity(snapshot)) {
      await this.store.delete(`session:${sessionId}`);
      return null;
    }
    
    // 4. Check expiration
    if (new Date() > new Date(snapshot.expiresAt)) {
      await this.store.delete(`session:${sessionId}`);
      return null;
    }
    
    // 5. Rehydrate session
    return this.rehydrate(snapshot);
  }
  
  private calculateTTL(snapshot: SessionSnapshot): number {
    // TTL based on activity patterns
    const idleTime = Date.now() - new Date(snapshot.lastActivityAt).getTime();
    const baseTTL = this.config.baseTTL;
    const maxTTL = this.config.maxTTL;
    
    // Extend TTL for recently active sessions
    const activityBonus = Math.max(0, baseTTL - idleTime);
    return Math.min(maxTTL, baseTTL + activityBonus);
  }
  
  private verifyIntegrity(snapshot: SessionSnapshot): boolean {
    const serialized = JSON.stringify({
      ...snapshot,
      checksum: undefined
    });
    const computedChecksum = computeChecksum(serialized);
    return computedChecksum === snapshot.checksum;
  }
  
  private rehydrate(snapshot: SessionSnapshot): Session {
    return new Session({
      id: snapshot.sessionId,
      key: snapshot.sessionKey,
      createdAt: new Date(snapshot.createdAt),
      messageHistory: snapshot.messageHistory.map(m => ({
        ...m,
        timestamp: new Date(m.timestamp)
      })),
      toolState: snapshot.toolState,
      config: snapshot.agentConfig
    });
  }
}
```

### Gateway Integration

```typescript
export class GatewaySessionManager {
  private sessionStore: PersistentSessionStore;
  private activeSessions = new Map<string, Session>();
  
  constructor(sessionStore: PersistentSessionStore) {
    this.sessionStore = sessionStore;
  }
  
  // Called on gateway startup
  async initialize(): Promise<void> {
    // Restore all non-expired sessions
    const sessionIds = await this.sessionStore.listActiveSessions();
    
    for (const sessionId of sessionIds) {
      const session = await this.sessionStore.restore(sessionId);
      if (session) {
        this.activeSessions.set(sessionId, session);
        console.log(`Restored session: ${sessionId}`);
      }
    }
    
    console.log(`Restored ${this.activeSessions.size} sessions`);
  }
  
  // Called on gateway shutdown
  async shutdown(): Promise<void> {
    // Persist all active sessions
    for (const [sessionId, session] of this.activeSessions) {
      const snapshot = this.createSnapshot(session);
      await this.sessionStore.persist(snapshot);
    }
    
    console.log(`Persisted ${this.activeSessions.size} sessions`);
  }
  
  // Periodic persistence (every N minutes)
  async checkpoint(): Promise<void> {
    const promises = Array.from(this.activeSessions.values())
      .map(session => {
        const snapshot = this.createSnapshot(session);
        return this.sessionStore.persist(snapshot);
      });
    
    await Promise.all(promises);
  }
  
  private createSnapshot(session: Session): SessionSnapshot {
    return {
      sessionId: session.id,
      sessionKey: session.key,
      createdAt: session.createdAt,
      lastActivityAt: new Date(),
      expiresAt: this.calculateExpiry(session),
      messageHistory: session.messageHistory,
      toolState: session.toolState,
      agentConfig: this.snapshotAgentConfig(session.config),
      version: 1,
      checksum: ''  // Computed during persist
    };
  }
}
```

## Recovery Scenarios

### Scenario 1: Graceful Restart

```typescript
// Normal shutdown sequence
await gateway.shutdown();
// Sessions persisted

// Startup
await gateway.initialize();
// Sessions restored from store
```

### Scenario 2: Crash Recovery

```typescript
// Gateway crashes - sessions in memory lost
// But persisted sessions remain in store

// On restart
await gateway.initialize();
// Restore from last checkpoint (max TTL worth of data loss)
```

### Scenario 3: TTL Expiration

```typescript
// Session not accessed for extended period
const snapshot = await store.get(`session:${sessionId}`);
if (!snapshot) {
  // Session expired and was cleaned up
  return null;
}
```

## Configuration

```typescript
export interface PersistenceConfig {
  // TTL settings
  baseTTL: number;        // Default: 24 hours (in ms)
  maxTTL: number;         // Default: 7 days (in ms)
  
  // Checkpoint interval
  checkpointInterval: number;  // Default: 5 minutes (in ms)
  
  // Store backend
  backend: 'redis' | 'sqlite' | 'memory';
  
  // Encryption
  encryptAtRest: boolean;
  encryptionKey?: string;
}

export const DEFAULT_PERSISTENCE_CONFIG: PersistenceConfig = {
  baseTTL: 24 * 60 * 60 * 1000,      // 24 hours
  maxTTL: 7 * 24 * 60 * 60 * 1000,   // 7 days
  checkpointInterval: 5 * 60 * 1000,  // 5 minutes
  backend: 'redis',
  encryptAtRest: true
};
```

## State Machine Preservation

### Replay State Management

Based on commits `7f54cf73e2`, `eb185f4a03`, `b9a9472cfd`:

```typescript
export interface ReplayState {
  // Session validation
  isValid: boolean;
  invalidationReason?: string;
  
  // Retry tracking
  retryCount: number;
  maxRetries: number;
  lastRetryAt?: Date;
  
  // Compaction state
  compactionVersion: number;
  lastCompactionAt?: Date;
  
  // Must be preserved across:
  // - Compaction retries
  // - Retry exhaustion
  // - Mutating operations
  // - Lifecycle transitions
}

export function preserveReplayState(
  current: ReplayState,
  operation: 'retry' | 'compact' | 'mutate'
): ReplayState {
  return {
    ...current,
    // Always preserve validation truth
    isValid: current.isValid,
    invalidationReason: current.invalidationReason,
    // Update operation-specific fields
    retryCount: operation === 'retry' ? current.retryCount + 1 : current.retryCount
  };
}
```

## Storage Backends

### Redis Backend

```typescript
export class RedisSessionStore implements KeyValueStore {
  private redis: Redis;
  
  async set(key: string, value: string, options: { ttl: number }): Promise<void> {
    await this.redis.setex(key, Math.floor(options.ttl / 1000), value);
  }
  
  async get(key: string): Promise<string | null> {
    return this.redis.get(key);
  }
  
  async delete(key: string): Promise<void> {
    await this.redis.del(key);
  }
  
  async listActiveSessions(): Promise<string[]> {
    const keys = await this.redis.keys('session:*');
    return keys
      .filter(k => !k.startsWith('session:key:'))
      .map(k => k.replace('session:', ''));
  }
}
```

### SQLite Backend

```typescript
export class SQLiteSessionStore implements KeyValueStore {
  private db: Database;
  
  constructor(dbPath: string) {
    this.db = new Database(dbPath);
    this.initialize();
  }
  
  private initialize(): void {
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS sessions (
        session_id TEXT PRIMARY KEY,
        session_key TEXT UNIQUE NOT NULL,
        data TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        created_at INTEGER DEFAULT (unixepoch())
      );
      
      CREATE INDEX IF NOT EXISTS idx_expires 
      ON sessions(expires_at);
    `);
  }
  
  async set(key: string, value: string, options: { ttl: number }): Promise<void> {
    const expiresAt = Date.now() + options.ttl;
    this.db.prepare(`
      INSERT OR REPLACE INTO sessions (session_id, session_key, data, expires_at)
      VALUES (?, ?, ?, ?)
    `).run(key.replace('session:', ''), key, value, expiresAt);
  }
  
  async get(key: string): Promise<string | null> {
    const row = this.db.prepare(
      'SELECT data FROM sessions WHERE session_id = ? AND expires_at > ?'
    ).get(key.replace('session:', ''), Date.now());
    return row?.data || null;
  }
  
  async cleanup(): Promise<void> {
    this.db.prepare('DELETE FROM sessions WHERE expires_at < ?')
      .run(Date.now());
  }
}
```

## Integration with Active-Memory

```typescript
// Active-memory context preservation
export interface ActiveMemoryContext {
  // What to remember across sessions
  longTermFacts: Fact[];
  userPreferences: Preference[];
  projectContext: ProjectContext;
  
  // What can be discarded
  ephemeralContext: Ephemeral[];
}

export function preserveActiveMemory(session: Session): ActiveMemorySnapshot {
  const context = session.getActiveMemory();
  
  return {
    longTermFacts: context.facts.filter(f => f.persistent),
    userPreferences: context.preferences,
    projectContext: context.project,
    // Ephemeral context NOT included - will be reconstructed
  };
}
```

## T430 Fitness Integration

Session persistence affects T430 fitness metrics:

| Component | Impact | Weight |
|-----------|--------|--------|
| Syntax | Schema validation | 30% |
| Semantic | State integrity preservation | 40% |
| Quality | Recovery completeness | 20% |
| Security | Encryption, TTL validation | 10% |

## Migration Guide

### Phase 1: Add Store Interface (Non-breaking)

```typescript
// Add optional persistence
interface SessionManager {
  sessionStore?: PersistentSessionStore;  // Optional initially
}
```

### Phase 2: Enable Persistence (Config flag)

```typescript
// Default: disabled, opt-in
const manager = new SessionManager({
  persistence: {
    enabled: true,
    backend: 'redis'
  }
});
```

### Phase 3: Make Default (Breaking change for v2)

```typescript
// Persistence enabled by default
const manager = new SessionManager({
  persistence: {
    enabled: true,  // Now default
    backend: 'redis'
  }
});
```

## Testing

```typescript
describe('Session Persistence', () => {
  it('should persist session on shutdown', async () => {
    const session = await manager.createSession();
    await manager.shutdown();
    
    const restored = await manager.restore(session.id);
    expect(restored.messageHistory).toEqual(session.messageHistory);
  });
  
  it('should reject expired sessions', async () => {
    const snapshot = createSnapshot({ expiresAt: Date.now() - 1000 });
    const valid = manager.validate(snapshot);
    expect(valid).toBe(false);
  });
  
  it('should detect integrity violations', async () => {
    const snapshot = createSnapshot();
    snapshot.checksum = 'tampered';
    const valid = manager.verifyIntegrity(snapshot);
    expect(valid).toBe(false);
  });
});
```

## Classification

- **Safety:** Requires manual review - touches core gateway state
- **Scope:** Gateway session management, storage layer
- **Breaking Changes:** None if implemented as optional feature
- **Dependencies:** Key-value store (Redis/SQLite)

---

*Generated by OpenEvolve Night Cycle 2026-04-12*  
*Based on GitHub Issue #64687 and commits 7f54cf73e2, eb185f4a03, b9a9472cfd*  
*Neural State: ChaosInitial | Turbulence: 0.0939 | Engineer: 34.7%*
