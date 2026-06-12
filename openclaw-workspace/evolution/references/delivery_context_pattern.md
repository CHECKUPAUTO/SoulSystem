# Delivery Context Pattern - Notification Routing Preservation

**Source:** OpenEvolve Night Cycle Report 2026-04-12 04:45  
**Author:** Pattern identified from commit 29142a9d  
**Priority:** P1 - High Priority  
**Classification:** Bug Fix Pattern / Architecture Pattern

---

## Problem Statement

**Ghost Reminders on Topic/Thread Mismatch:** When async operations (exec, cron, subagents) complete and send notifications, the delivery context (topic ID, thread ID) is often lost. This causes:
- Messages appearing in wrong Telegram topics
- Notifications sent to wrong channels
- "Ghost" reminders that appear in unexpected places
- Confusing user experience

**Evidence from Night Cycle:**
```
29142a9d - fix(Telegram): preserve topic routing for exec completions
- Adds delivery context normalization
- 67-line test for ghost reminder scenarios
- Fixes user-reported issue #64580
```

---

## Root Cause Analysis

### The Delivery Context Loss

```typescript
// BEFORE: Context lost in async handoff
async function executeCommand(command: Command) {
  const result = await exec(command);
  
  // Problem: replyTo is lost here
  await message.send({
    text: result.output,
    // Missing: replyTo, topicId, threadId
  });
}

// User sees message in wrong place
```

### Where Context Gets Lost

| Path | Source | Destination | Risk |
|------|--------|-------------|------|
| exec completion | bash-tools.exec-runtime.ts | channels/telegram | **High** |
| cron runner | cron.runner.ts | notification channels | **High** |
| subagent dispatch | subagents.dispatcher.ts | original channel | **Medium** |
| heartbeat runner | heartbeat-runner.ts | various | **Low** |

---

## Solution: Delivery Context Normalization

### Core Pattern

**Normalize delivery context at the edge and propagate through async boundaries:**

```typescript
// NEW: src/utils/delivery-context.ts

export interface DeliveryContext {
  // Channel identification
  channelId?: string;
  channelType?: 'telegram' | 'whatsapp' | 'discord' | 'slack';
  
  // Threading (Telegram topics, Discord threads, etc.)
  threadId?: string;
  topicId?: string;
  
  // Reply context
  replyToMessageId?: string;
  
  // Trusted sender flag
  trusted?: boolean;
  
  // Original message reference
  originalMessageId?: string;
}

/**
 * Normalizes delivery context from various input formats
 * Ensures consistent structure across the codebase
 */
export function normalizeDeliveryContext(
  input: Partial<DeliveryContext> | Record<string, unknown> | undefined
): DeliveryContext {
  if (!input) {
    return {};
  }

  return {
    channelId: input.channelId ?? input.channel_id ?? input.channel,
    channelType: normalizeChannelType(input.channelType ?? input.channel_type),
    threadId: input.threadId ?? input.thread_id ?? input.thread,
    topicId: input.topicId ?? input.topic_id ?? input.topic,
    replyToMessageId: input.replyToMessageId ?? input.reply_to ?? input.replyTo,
    trusted: input.trusted ?? false,
    originalMessageId: input.originalMessageId ?? input.message_id,
  };
}

function normalizeChannelType(
  type: string | undefined
): DeliveryContext['channelType'] | undefined {
  if (!type) return undefined;
  
  const normalized = type.toLowerCase();
  if (['telegram', 'whatsapp', 'discord', 'slack'].includes(normalized)) {
    return normalized as DeliveryContext['channelType'];
  }
  return undefined;
}
```

### Integration with Exec Runtime

```typescript
// bash-tools.exec-runtime.ts

export async function executeWithDeliveryContext(
  command: string,
  options: ExecOptions,
  deliveryContext: DeliveryContext
): Promise<ExecResult> {
  const result = await exec(command, options);
  
  // Preserve delivery context for completion notification
  await sendCompletionNotification(result, deliveryContext);
  
  return result;
}

async function sendCompletionNotification(
  result: ExecResult,
  deliveryContext: DeliveryContext
) {
  const messageOptions: MessageOptions = {
    text: formatResult(result),
    // Preserve all routing context
    replyTo: deliveryContext.replyToMessageId,
    threadId: deliveryContext.threadId,
    topicId: deliveryContext.topicId,
    trusted: deliveryContext.trusted,
  };
  
  await message.send(messageOptions);
}
```

### Channel-Specific Implementation

#### Telegram

```typescript
// channels/telegram/web-outbound.ts

export async function sendTelegramMessage(
  content: string,
  deliveryContext: DeliveryContext
): Promise<void> {
  const params: TelegramSendParams = {
    chat_id: deliveryContext.channelId,
    text: content,
    // Critical: preserve topic/thread context
    message_thread_id: deliveryContext.topicId,
    reply_parameters: deliveryContext.replyToMessageId ? {
      message_id: deliveryContext.replyToMessageId,
    } : undefined,
  };
  
  await telegramApi.sendMessage(params);
}
```

#### Discord

```typescript
// channels/discord/web-outbound.ts

export async function sendDiscordMessage(
  content: string,
  deliveryContext: DeliveryContext
): Promise<void> {
  const channel = await client.channels.fetch(
    deliveryContext.threadId ?? deliveryContext.channelId
  );
  
  if (channel?.isTextBased()) {
    await channel.send({
      content,
      reply: deliveryContext.replyToMessageId ? {
        messageReference: deliveryContext.replyToMessageId,
      } : undefined,
    });
  }
}
```

---

## Audit Results

Based on T430 analysis of notification paths:

| Module | Has Delivery Context | Needs Update | Priority |
|--------|---------------------|--------------|----------|
| bash-tools.exec-runtime.ts | ✅ Yes | - | - |
| cron.runner.ts | ❌ No | **HIGH** | P1 |
| subagents.dispatcher.ts | ❌ No | **MEDIUM** | P2 |
| heartbeat-runner.ts | ❌ No | **LOW** | P3 |
| gateway/webhook-handler.ts | ❌ Unknown | **AUDIT** | P2 |

---

## Migration Path

### Phase 1: Shared Utility (Complete)

Create `src/utils/delivery-context.ts` with normalization functions.

### Phase 2: Exec Runtime (Complete)

Commit 29142a9d already implements delivery context for exec completions.

### Phase 3: Cron Runner (Next)

```typescript
// cron.runner.ts

export async function runCronJob(
  job: CronJob,
  deliveryContext: DeliveryContext
): Promise<void> {
  // Preserve context from job creation
  const preservedContext = normalizeDeliveryContext(job.metadata?.deliveryContext);
  
  const result = await executeJob(job);
  
  // Send notification with preserved context
  await notifyJobCompletion(result, preservedContext);
}
```

### Phase 4: Subagent Dispatcher

```typescript
// subagents.dispatcher.ts

export async function dispatchSubagent(
  task: SubagentTask,
  deliveryContext: DeliveryContext
): Promise<SubagentResult> {
  // Pass context to subagent
  const result = await spawnSubagent({
    ...task,
    deliveryContext: normalizeDeliveryContext(deliveryContext),
  });
  
  // Return results to original context
  await sendSubagentResult(result, deliveryContext);
  
  return result;
}
```

---

## Testing

```typescript
// test/delivery-context.test.ts

describe('normalizeDeliveryContext', () => {
  it('should normalize legacy snake_case fields', () => {
    const input = {
      channel_id: '123',
      thread_id: '456',
      reply_to: '789',
    };
    
    const result = normalizeDeliveryContext(input);
    
    expect(result.channelId).toBe('123');
    expect(result.threadId).toBe('456');
    expect(result.replyToMessageId).toBe('789');
  });

  it('should handle Telegram topic IDs', () => {
    const input = {
      channelId: '123',
      topicId: '456',
    };
    
    const result = normalizeDeliveryContext(input);
    
    expect(result.topicId).toBe('456');
  });

  it('should return empty object for undefined input', () => {
    const result = normalizeDeliveryContext(undefined);
    expect(result).toEqual({});
  });
});

describe('ghost reminder prevention', () => {
  it('should preserve topic context through exec completion', async () => {
    const deliveryContext = {
      channelId: 'test-channel',
      topicId: 'test-topic',
    };
    
    const result = await executeWithDeliveryContext(
      'echo "test"',
      {},
      deliveryContext
    );
    
    // Verify notification was sent to correct topic
    expect(mockMessage.send).toHaveBeenCalledWith(
      expect.objectContaining({
        topicId: 'test-topic',
      })
    );
  });
});
```

---

## Configuration

```yaml
# config.yaml
delivery-context:
  # Enable delivery context preservation globally
  enabled: true
  
  # Default trusted channels (skip validation)
  trusted-channels:
    - telegram
    - whatsapp
  
  # Fields to preserve through async boundaries
  preserved-fields:
    - channelId
    - channelType
    - threadId
    - topicId
    - replyToMessageId
    - trusted
```

---

## Related Patterns

- **Session State Management**: `session_state_management_patterns.md`
- **Startup Context Pattern**: `startup_context_pattern_v2.md`
- **Context Tree Pattern**: `context_tree_pattern.md`

---

## References

- Night Cycle Report: `night_cycle_20260412_0445.md`
- Commit: `29142a9d`
- Issue: #64580

---

*Generated by OpenEvolve Auto-Apply*  
*Classification: P1 High Priority Bug Fix Pattern*
