# Session Persistence Implementation Guide

**Status:** Reference Documentation  
**Source:** night_cycle_20260412_0215.md (Issue #64687)  
**Priority:** P0 - Critical Reliability  
**Auto-Apply:** ❌ NO - Requires Core State Management Changes  

## Overview

Implementation guide for session persistence to enable graceful recovery from crashes, restarts, and container preemptions.

## Problem Statement

- Session state is lost on restart/crash
- Long-running tasks cannot recover
- User context disappears between deployments
- Container orchestration kills cause data loss

## Solution: Atomic Session Persistence

### Core Architecture

```typescript
// src/session/session-persistence.ts
export interface SessionPersistenceConfig {
  checkpointInterval: number;      // 30 seconds default
  persistencePath: string;       // Path for session files
  maxSnapshots: number;          // Retention policy
  enableEncryption: boolean;     // Encrypt sensitive data
  compression: boolean;          // Gzip compression
}

interface SessionSnapshot {
  sessionId: string;
  timestamp: number;
  state: SessionState;
  version: number;               // Schema versioning
  checksum: string;              // Integrity verification
}
```

### Implementation

```typescript
import { writeFileSync, renameSync, existsSync, mkdirSync } from 'fs';
import { createHash } from 'crypto';
import { gzipSync } from 'zlib';
import { join } from 'path';

export class SessionPersistence {
  private config: SessionPersistenceConfig;
  private checkpointTimer?: NodeJS.Timeout;
  private currentSession?: SessionSnapshot;

  constructor(config: Partial<SessionPersistenceConfig> = {}) {
    this.config = {
      checkpointInterval: 30000,
      persistencePath: './.openclaw/sessions',
      maxSnapshots: 10,
      enableEncryption: false,
      compression: true,
      ...config
    };

    // Ensure persistence directory exists
    if (!existsSync(this.config.persistencePath)) {
      mkdirSync(this.config.persistencePath, { recursive: true });
    }

    this.setupSignalHandlers();
  }

  start(sessionId: string, initialState: SessionState): void {
    this.currentSession = {
      sessionId,
      timestamp: Date.now(),
      state: initialState,
      version: 1,
      checksum: ''
    };

    // Start periodic checkpointing
    this.checkpointTimer = setInterval(
      () => this.checkpoint(),
      this.config.checkpointInterval
    );
  }

  async checkpoint(): Promise<void> {
    if (!this.currentSession) return;

    const snapshot = { ...this.currentSession, timestamp: Date.now() };
    snapshot.checksum = this.calculateChecksum(snapshot);

    const tempPath = this.getTempPath(snapshot.sessionId);
    const finalPath = this.getSnapshotPath(snapshot.sessionId);

    // Serialize with optional compression
    let data = JSON.stringify(snapshot);
    if (this.config.compression) {
      data = gzipSync(data).toString('base64');
    }

    // Atomic write: write to temp, then rename
    writeFileSync(tempPath, data, 'utf8');
    
    // fsync for durability
    const fd = require('fs').openSync(tempPath, 'r+');
    require('fs').fsyncSync(fd);
    require('fs').closeSync(fd);

    // Atomic rename
    renameSync(tempPath, finalPath);

    // Cleanup old snapshots
    this.cleanupOldSnapshots(snapshot.sessionId);
  }

  restore(sessionId: string): SessionSnapshot | null {
    const snapshotPath = this.getSnapshotPath(sessionId);
    
    if (!existsSync(snapshotPath)) {
      return null;
    }

    try {
      let data = require('fs').readFileSync(snapshotPath, 'utf8');
      
      // Decompress if needed
      if (this.config.compression && data.startsWith('H4')) {
        data = require('zlib').gunzipSync(Buffer.from(data, 'base64')).toString();
      }

      const snapshot: SessionSnapshot = JSON.parse(data);

      // Verify checksum
      const expectedChecksum = snapshot.checksum;
      snapshot.checksum = '';
      const actualChecksum = this.calculateChecksum(snapshot);

      if (expectedChecksum !== actualChecksum) {
        throw new Error('Session snapshot checksum mismatch - possible corruption');
      }

      return snapshot;
    } catch (error) {
      console.error(`Failed to restore session ${sessionId}:`, error);
      return null;
    }
  }

  private setupSignalHandlers(): void {
    const gracefulShutdown = async (signal: string) => {
      console.log(`Received ${signal}, performing final checkpoint...`);
      await this.checkpoint();
      if (this.checkpointTimer) {
        clearInterval(this.checkpointTimer);
      }
      process.exit(0);
    };

    process.on('SIGTERM', () => gracefulShutdown('SIGTERM'));
    process.on('SIGINT', () => gracefulShutdown('SIGINT'));
    
    // Handle uncaught exceptions
    process.on('uncaughtException', async (error) => {
      console.error('Uncaught exception:', error);
      await this.checkpoint();
      process.exit(1);
    });
  }

  private calculateChecksum(snapshot: SessionSnapshot): string {
    const data = JSON.stringify({
      sessionId: snapshot.sessionId,
      timestamp: snapshot.timestamp,
      state: snapshot.state,
      version: snapshot.version
    });
    return createHash('sha256').update(data).digest('hex').substring(0, 16);
  }

  private getTempPath(sessionId: string): string {
    return join(this.config.persistencePath, `${sessionId}.tmp`);
  }

  private getSnapshotPath(sessionId: string): string {
    return join(this.config.persistencePath, `${sessionId}.json`);
  }

  private cleanupOldSnapshots(sessionId: string): void {
    // Implementation for retention policy
    // Keep only maxSnapshots most recent
  }

  stop(): void {
    if (this.checkpointTimer) {
      clearInterval(this.checkpointTimer);
    }
  }
}
```

## Integration with Session Manager

```typescript
// src/session/session-manager.ts
export class SessionManager {
  private persistence: SessionPersistence;
  private sessions = new Map<string, Session>();

  constructor() {
    this.persistence = new SessionPersistence();
  }

  async createSession(sessionId: string, context: SessionContext): Promise<Session> {
    // Check for existing persisted session
    const restored = this.persistence.restore(sessionId);
    
    if (restored) {
      console.log(`Restored session ${sessionId} from checkpoint`);
      return this.hydrateSession(restored);
    }

    // Create new session
    const session = new Session(sessionId, context);
    this.sessions.set(sessionId, session);
    
    // Start persistence
    this.persistence.start(sessionId, session.getState());
    
    return session;
  }

  private hydrateSession(snapshot: SessionSnapshot): Session {
    // Restore session from snapshot
    const session = Session.fromSnapshot(snapshot);
    this.sessions.set(snapshot.sessionId, session);
    return session;
  }
}
```

## Migration Strategy

### Phase 1: Opt-in Persistence
- Add config flag `ENABLE_SESSION_PERSISTENCE`
- Only persist sessions with explicit flag
- Monitor for issues

### Phase 2: Default On
- Enable by default for new sessions
- Maintain opt-out capability
- Full production rollout

### Phase 3: Migration Tools
- CLI tool to migrate old sessions
- Import/export functionality

## Security Considerations

1. **Encryption**: AES-256-GCM for sensitive session data
2. **Access Control**: File permissions 0600 for session files
3. **Key Management**: Integration with existing secret store
4. **Audit Logging**: Log all persistence operations

## Testing

```typescript
// test/session-persistence.test.ts
describe('SessionPersistence', () => {
  it('should persist and restore session state', async () => {
    const persistence = new SessionPersistence({
      persistencePath: './test-sessions'
    });

    const sessionId = 'test-session-123';
    const state = { messages: [], context: {} };

    persistence.start(sessionId, state);
    await persistence.checkpoint();

    const restored = persistence.restore(sessionId);
    expect(restored?.state).toEqual(state);
  });

  it('should handle corruption gracefully', () => {
    // Test checksum validation
  });

  it('should handle SIGTERM gracefully', async () => {
    // Test signal handling
  });
});
```

## Why Manual Implementation Required

This requires:
- New `src/session/` module or extension of existing
- Database schema changes if using DB persistence
- Encryption key management integration
- Signal handling in main process
- Migration tooling for existing sessions
- Security audit and hardening

## References

- Original Proposal: Issue #64687
- Related: `night_cycle_20260412_0215.md`
- Pattern: Atomic file writes with temp + rename
