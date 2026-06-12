# SQLite Fallback Strategy

**Source:** OpenEvolve Night Cycle Report 2026-04-12 05:00  
**Author:** Pattern from SoulLink proposal (eb9d8d41cc)  
**Priority:** P0 - Critical  
**Classification:** Resilience Pattern / Database Pattern

---

## Problem Statement

**node:sqlite Hard Crashes:** The `node:sqlite` module (Node.js native SQLite bindings) can cause hard crashes without graceful degradation. When SQLite fails:
- Gateway crashes completely
- No recovery path
- Session data lost
- Service unavailable

**Impact:** Production stability risk. Single point of failure in database layer.

---

## Solution: Multi-Tier Fallback Strategy

### Core Concept

Automatic fallback through multiple SQLite implementations:

```
Fallback Chain (priority order):

1. Native (node:sqlite)     - Preferred, fastest
2. LibSQL (libsql/client)   - Fallback 1, Turso compatible  
3. BetterSQLite3            - Fallback 2, mature alternative
4. In-Memory                - Emergency fallback, volatile
```

### Implementation

#### 1. SQLite Mode Enum

```typescript
// src/database/sqlite-modes.ts

export enum SQLiteMode {
  Native = 'native',           // node:sqlite (preferred)
  LibSQL = 'libsql',          // libsql/client (fallback 1)
  BetterSQLite3 = 'better',   // better-sqlite3 (fallback 2)
  Memory = 'memory',          // In-memory (emergency)
}

export interface SQLiteModeInfo {
  mode: SQLiteMode;
  package: string;
  features: string[];
  limitations: string[];
}

export const MODE_INFO: Record<SQLiteMode, SQLiteModeInfo> = {
  [SQLiteMode.Native]: {
    mode: SQLiteMode.Native,
    package: 'node:sqlite',
    features: ['Native performance', 'Full SQLite features', 'Bundled with Node.js'],
    limitations: ['Node.js 22.5+ required', 'May crash on certain operations'],
  },
  [SQLiteMode.LibSQL]: {
    mode: SQLiteMode.LibSQL,
    package: '@libsql/client',
    features: ['Remote sync', 'Turso compatible', 'Drop-in replacement'],
    limitations: ['Additional dependency', 'Network latency for remote'],
  },
  [SQLiteMode.BetterSQLite3]: {
    mode: SQLiteMode.BetterSQLite3,
    package: 'better-sqlite3',
    features: ['Synchronous API', 'Mature', 'Well-tested'],
    limitations: ['Native compilation required', 'Heavier binary'],
  },
  [SQLiteMode.Memory]: {
    mode: SQLiteMode.Memory,
    package: 'in-memory',
    features: ['Zero setup', 'Always available', 'No persistence'],
    limitations: ['Data lost on restart', 'Limited capacity', 'No durability'],
  },
};
```

#### 2. Auto-Detection with Fallback

```typescript
// src/database/sqlite-factory.ts

import { Database, SQLiteMode } from './types';
import { MODE_INFO } from './sqlite-modes';

interface DatabaseFactory {
  create(mode: SQLiteMode, path: string): Promise<Database>;
  isAvailable(mode: SQLiteMode): Promise<boolean>;
}

export async function initializeDatabase(
  configPath: string,
  options: InitOptions = {}
): Promise<Database> {
  const {
    preferredMode = SQLiteMode.Native,
    fallbackChain = [
      SQLiteMode.Native,
      SQLiteMode.LibSQL,
      SQLiteMode.BetterSQLite3,
      SQLiteMode.Memory,
    ],
    timeoutMs = 5000,
  } = options;

  // Reorder to put preferred mode first
  const modes = reorderWithPreferred(fallbackChain, preferredMode);

  const errors: Array<{ mode: SQLiteMode; error: Error }> = [];

  for (const mode of modes) {
    try {
      const db = await createDatabaseWithTimeout(mode, configPath, timeoutMs);
      
      // Log successful initialization
      console.info(`Database initialized with ${MODE_INFO[mode].package}`);
      
      // Emit metrics event
      emitDatabaseInitialized(mode, errors.length);
      
      return db;
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      console.warn(`SQLite mode ${mode} failed: ${err.message}`);
      errors.push({ mode, error: err });
    }
  }

  // All fallbacks exhausted
  throw new DatabaseInitializationError(
    'All SQLite fallbacks failed',
    errors
  );
}

async function createDatabaseWithTimeout(
  mode: SQLiteMode,
  path: string,
  timeoutMs: number
): Promise<Database> {
  return Promise.race([
    createDatabase(mode, path),
    new Promise<never>((_, reject) => 
      setTimeout(() => reject(new Error('Database initialization timeout')), timeoutMs)
    ),
  ]);
}

async function createDatabase(mode: SQLiteMode, path: string): Promise<Database> {
  switch (mode) {
    case SQLiteMode.Native:
      return createNativeDatabase(path);
    case SQLiteMode.LibSQL:
      return createLibSQLDatabase(path);
    case SQLiteMode.BetterSQLite3:
      return createBetterSQLite3Database(path);
    case SQLiteMode.Memory:
      return createMemoryDatabase();
    default:
      throw new Error(`Unknown SQLite mode: ${mode}`);
  }
}

// Native node:sqlite
async function createNativeDatabase(path: string): Promise<Database> {
  const { DatabaseSync } = await import('node:sqlite');
  return new DatabaseSync(path);
}

// LibSQL client
async function createLibSQLDatabase(path: string): Promise<Database> {
  const { createClient } = await import('@libsql/client');
  return createClient({ url: `file:${path}` });
}

// Better-sqlite3
async function createBetterSQLite3Database(path: string): Promise<Database> {
  const Database = await import('better-sqlite3');
  return new Database.default(path);
}

// In-memory fallback
async function createMemoryDatabase(): Promise<Database> {
  const { DatabaseSync } = await import('node:sqlite');
  return new DatabaseSync(':memory:');
}
```

#### 3. Database Wrapper with Health Checks

```typescript
// src/database/resilient-database.ts

export class ResilientDatabase {
  private db: Database | null = null;
  private mode: SQLiteMode | null = null;
  private healthCheckInterval: NodeJS.Timeout | null = null;

  constructor(private config: ResilientDatabaseConfig) {}

  async initialize(): Promise<void> {
    this.db = await initializeDatabase(this.config.path, {
      preferredMode: this.config.preferredMode,
      fallbackChain: this.config.fallbackChain,
    });
    
    this.mode = detectMode(this.db);
    
    // Start health checks if not in memory mode
    if (this.mode !== SQLiteMode.Memory) {
      this.startHealthChecks();
    }
  }

  async query<T>(sql: string, params?: unknown[]): Promise<T[]> {
    if (!this.db) {
      throw new Error('Database not initialized');
    }

    try {
      return await this.executeQuery(sql, params);
    } catch (error) {
      // Attempt recovery on query failure
      if (await this.attemptRecovery()) {
        return this.executeQuery(sql, params);
      }
      throw error;
    }
  }

  private async executeQuery<T>(sql: string, params?: unknown[]): Promise<T[]> {
    // Mode-specific query execution
    switch (this.mode) {
      case SQLiteMode.Native:
        return this.queryNative(sql, params);
      case SQLiteMode.LibSQL:
        return this.queryLibSQL(sql, params);
      case SQLiteMode.BetterSQLite3:
        return this.queryBetterSQLite3(sql, params);
      case SQLiteMode.Memory:
        return this.queryNative(sql, params);
      default:
        throw new Error(`Unknown mode: ${this.mode}`);
    }
  }

  private async attemptRecovery(): Promise<boolean> {
    console.warn('Attempting database recovery...');
    
    try {
      // Try to reinitialize with next fallback
      const remainingChain = this.getRemainingFallbackChain();
      
      if (remainingChain.length === 0) {
        return false;
      }

      this.db = await initializeDatabase(this.config.path, {
        fallbackChain: remainingChain,
      });
      
      this.mode = detectMode(this.db);
      console.info(`Recovered with mode: ${this.mode}`);
      
      return true;
    } catch (error) {
      console.error('Recovery failed:', error);
      return false;
    }
  }

  private startHealthChecks(): void {
    this.healthCheckInterval = setInterval(async () => {
      try {
        await this.healthCheck();
      } catch (error) {
        console.warn('Health check failed:', error);
        await this.attemptRecovery();
      }
    }, this.config.healthCheckIntervalMs ?? 30000);
  }

  private async healthCheck(): Promise<void> {
    if (!this.db) throw new Error('Database not initialized');
    
    // Simple ping query
    await this.executeQuery('SELECT 1');
  }

  destroy(): void {
    if (this.healthCheckInterval) {
      clearInterval(this.healthCheckInterval);
    }
    
    if (this.db) {
      this.db.close?.();
      this.db = null;
    }
  }
}
```

---

## Configuration

```yaml
# config.yaml
database:
  sqlite:
    path: "/data/openclaw.db"
    preferred-mode: "native"  # node:sqlite
    fallback-chain:
      - "native"
      - "libsql"
      - "better"
      - "memory"
    init-timeout-ms: 5000
    health-check-interval-ms: 30000
    
    # Mode-specific options
    libsql:
      url: "file:/data/openclaw.db"
      sync-url: null  # Optional remote sync
      
    better-sqlite3:
      options:
        verbose: false
        timeout: 5000
```

---

## Migration Path

### Phase 1: Add Fallback Detection (Immediate)

```typescript
// Wrap existing database initialization
export async function migrateToResilientDatabase(
  existingDb: Database
): Promise<ResilientDatabase> {
  const resilient = new ResilientDatabase({
    path: ':memory:',  // Use existing connection
    fallbackChain: [SQLiteMode.Memory],  // Skip to memory if wrapped
  });
  
  // Inject existing database
  (resilient as any).db = existingDb;
  
  return resilient;
}
```

### Phase 2: Gradual Rollout (Week 1-2)

Enable for non-critical extensions first:
- `healthcheck` extension
- `blogwatcher` extension
- Test extensions

### Phase 3: Full Migration (Week 3-4)

Migrate core gateway database:
- Session store
- Plugin registry
- Configuration cache

---

## Monitoring & Alerting

### Metrics to Track

| Metric | Description | Alert Threshold |
|--------|-------------|-----------------|
| `db_init_fallbacks_used` | Number of fallbacks used | > 0 (warning) |
| `db_query_failures` | Query failure rate | > 1% (critical) |
| `db_recovery_attempts` | Recovery attempts | > 0 (warning) |
| `db_mode_distribution` | Active modes by count | memory > 10% (warning) |

### Alert Example

```yaml
# alerts.yaml
- name: DatabaseFallbackTriggered
  condition: db_init_fallbacks_used > 0
  severity: warning
  message: "Database fallback triggered - check node:sqlite health"

- name: EmergencyMemoryMode
  condition: db_mode == 'memory'
  severity: critical
  message: "Database in emergency memory mode - data will be lost on restart"
```

---

## Testing

```typescript
// test/sqlite-fallback.test.ts

describe('initializeDatabase', () => {
  it('should prefer native mode when available', async () => {
    const db = await initializeDatabase(':memory:', {
      preferredMode: SQLiteMode.Native,
    });
    
    expect(detectMode(db)).toBe(SQLiteMode.Native);
  });

  it('should fallback to libsql when native fails', async () => {
    // Mock native to fail
    jest.spyOn(nativeModule, 'DatabaseSync').mockImplementation(() => {
      throw new Error('Native SQLite unavailable');
    });
    
    const db = await initializeDatabase(':memory:');
    
    expect(detectMode(db)).toBe(SQLiteMode.LibSQL);
  });

  it('should use memory as last resort', async () => {
    // Mock all modes to fail
    jest.spyOn(nativeModule, 'DatabaseSync').mockImplementation(() => {
      throw new Error('Native unavailable');
    });
    jest.spyOn(libsqlModule, 'createClient').mockRejectedValue(new Error('LibSQL unavailable'));
    
    const db = await initializeDatabase(':memory:');
    
    expect(detectMode(db)).toBe(SQLiteMode.Memory);
  });

  it('should throw when all fallbacks exhausted', async () => {
    // Mock all modes including memory to fail
    jest.spyOn(nativeModule, 'DatabaseSync').mockImplementation(() => {
      throw new Error('Native unavailable');
    });
    
    await expect(initializeDatabase(':memory:', {
      fallbackChain: [SQLiteMode.Native],
    })).rejects.toThrow('All SQLite fallbacks failed');
  });
});
```

---

## Related Patterns

- **Circuit Breaker Pattern**: `circuit_breaker_pattern.md`
- **Config-Driven Fallback**: `config_driven_fallback_pattern.md`
- **Session Persistence**: `session_persistence_pattern.md`

---

## References

- Night Cycle Report: `night_cycle_20260412_0500.md`
- SoulLink Proposal: `eb9d8d41cc`
- GitHub Issue: #64695

---

*Generated by OpenEvolve Auto-Apply*  
*Classification: P0 Critical Resilience Pattern*  
*Credit: SQLite fallback strategy from SoulLink*
