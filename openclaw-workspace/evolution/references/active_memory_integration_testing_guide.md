# Active Memory Integration Testing Guide

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:15  
**Priority:** P1  
**Related Commits:** c31aa6da, 00d0dcf, 6800579e

---

## Background

The Active Memory extension underwent a series of cascading fixes indicating an incomplete initial implementation:

1. **c31aa6da**: Parent channel context not preserved for "recall runs"
2. **00d0dcf**: Configuration schema fallback field regressions
3. **6800579e/7fbf0b30**: Built-in fallback model causing conflicts

These issues suggest insufficient test coverage for edge cases in the recall lifecycle.

---

## Recommended Integration Test Suite

### File: `extensions/active-memory/integration.test.ts`

```typescript
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { ActiveMemoryExtension } from './index';
import { createMockChannelContext, createMockSession } from '../../test/mocks';

describe('Active Memory Integration', () => {
  let extension: ActiveMemoryExtension;
  let mockContext: ReturnType<typeof createMockChannelContext>;

  beforeEach(() => {
    extension = new ActiveMemoryExtension();
    mockContext = createMockChannelContext({
      channelId: 'test-channel-123',
      parentChannelId: 'parent-channel-456',
    });
  });

  afterEach(async () => {
    await extension.cleanup?.();
  });

  describe('Context Preservation', () => {
    it('should preserve parent channel context across recall runs', async () => {
      // Arrange: Initial session with parent context
      const initialSession = createMockSession({
        id: 'session-1',
        channelContext: mockContext,
      });

      // Act: Store and recall
      await extension.store(initialSession);
      const recalledSession = await extension.recall({
        channelId: mockContext.channelId,
      });

      // Assert: Parent context maintained
      expect(recalledSession.channelContext.parentChannelId)
        .toBe(mockContext.parentChannelId);
    });

    it('should handle missing parent context gracefully', async () => {
      // Arrange: Session without parent
      const orphanContext = createMockChannelContext({
        channelId: 'orphan-channel',
        parentChannelId: undefined,
      });
      const session = createMockSession({
        channelContext: orphanContext,
      });

      // Act & Assert: Should not throw
      await expect(extension.store(session)).resolves.not.toThrow();
      await expect(extension.recall({
        channelId: orphanContext.channelId,
      })).resolves.not.toThrow();
    });

    it('should merge recall context with stored context', async () => {
      // Arrange: Store session
      const storedSession = createMockSession({
        channelContext: {
          ...mockContext,
          metadata: { key1: 'value1' },
        },
      });
      await extension.store(storedSession);

      // Act: Recall with additional context
      const recalled = await extension.recall({
        channelId: mockContext.channelId,
        additionalContext: { key2: 'value2' },
      });

      // Assert: Both contexts present
      expect(recalled.context).toMatchObject({
        key1: 'value1',
        key2: 'value2',
      });
    });
  });

  describe('Schema Fallback Field Migration', () => {
    it('should handle schema fallback field migrations', async () => {
      // Arrange: Legacy session with old schema
      const legacySession = createMockSession({
        config: {
          activeMemory: {
            // Old field name (deprecated)
            fallbackModel: 'gpt-4o',
            // New field missing
          },
        },
      });

      // Act: Store with legacy schema
      await extension.store(legacySession);

      // Assert: Should migrate on recall
      const recalled = await extension.recall({
        sessionId: legacySession.id,
      });

      // New field should be populated
      expect(recalled.config.activeMemory.model).toBeDefined();
    });

    it('should reject invalid fallback configurations', async () => {
      // Arrange: Session with malformed config
      const badConfigSession = createMockSession({
        config: {
          activeMemory: {
            fallbackModel: 12345, // Invalid type
          },
        },
      });

      // Act & Assert: Should throw ConfigError
      await expect(extension.store(badConfigSession))
        .rejects.toThrow('ConfigError');
    });
  });

  describe('Recall Run Lifecycle', () => {
    it('should complete full recall run without context loss', async () => {
      // Arrange: Complex session state
      const complexSession = createMockSession({
        id: 'complex-session',
        channelContext: {
          ...mockContext,
          threadId: 'thread-789',
          replyTo: { messageId: 'msg-abc' },
        },
        memory: [
          { role: 'user', content: 'Hello' },
          { role: 'assistant', content: 'Hi there' },
        ],
      });

      // Act: Store and full recall
      await extension.store(complexSession);
      const recalled = await extension.recall({
        channelId: mockContext.channelId,
        threadId: 'thread-789',
      });

      // Assert: All context preserved
      expect(recalled.channelContext.threadId).toBe('thread-789');
      expect(recalled.channelContext.replyTo).toEqual({ messageId: 'msg-abc' });
      expect(recalled.memory).toHaveLength(2);
    });

    it('should handle concurrent recall requests', async () => {
      // Arrange: Multiple concurrent recalls
      const session = createMockSession({ channelContext: mockContext });
      await extension.store(session);

      // Act: Concurrent recalls
      const recalls = await Promise.all([
        extension.recall({ channelId: mockContext.channelId }),
        extension.recall({ channelId: mockContext.channelId }),
        extension.recall({ channelId: mockContext.channelId }),
      ]);

      // Assert: All succeed, same data
      expect(recalls).toHaveLength(3);
      expect(recalls[0].id).toBe(recalls[1].id);
      expect(recalls[1].id).toBe(recalls[2].id);
    });
  });
});
```

---

## Test Patterns

### 1. Context Preservation Pattern
- Store sessions with parent channel references
- Recall and verify parent chain intact
- Handle missing parents gracefully (orphan sessions)

### 2. Schema Migration Pattern
- Store with legacy schema fields
- Recall and verify automatic migration
- Reject truly invalid configurations

### 3. Recall Run Pattern
- Full lifecycle: store → recall → verify
- Complex state preservation (threads, replies, memory)
- Concurrent access safety

---

## Implementation Notes

1. **Mock Factory Requirements:**
   - `createMockChannelContext()` - supports parentChannelId
   - `createMockSession()` - full session object

2. **Extension Lifecycle:**
   - Cleanup after each test to prevent state leakage
   - Use isolated storage for parallel test runs

3. **Error Assertions:**
   - Expect `ConfigError` for schema violations
   - Use specific error messages for debugging

---

## References

- Source Report: `night_cycle_20260412_0315.md`
- Related Pattern: `config_driven_fallback_pattern.md`
- Related Pattern: `session_state_management_patterns.md`
