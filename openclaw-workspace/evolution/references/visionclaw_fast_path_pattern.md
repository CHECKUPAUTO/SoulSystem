# VisionClaw Fast-Path Pattern

**Source:** OpenEvolve Night Cycle Report 2026-04-12  
**Purpose:** Implement deterministic tool call short-circuit for common agentic actions to reduce latency

## Problem Statement

VisionClaw relies on Gemini Live API for real-time voice/vision processing. The full LLM reasoning pipeline adds latency that can break the "conversational loop" for simple, common actions.

## Solution: Fast-Path Pattern

Bypass LLM reasoning for deterministic, common actions using pattern matching and direct tool execution.

## Implementation

### 1. Fast-Path Registry

```typescript
// src/visionclaw/fast-path-registry.ts
interface FastPathAction {
  pattern: RegExp;
  action: string;
  params: Record<string, string>;
  confidence: number;
}

const FAST_PATH_ACTIONS: FastPathAction[] = [
  {
    pattern: /^(add|put).+to (my )?list/i,
    action: 'list.add',
    params: { source: 'voice' },
    confidence: 0.95
  },
  {
    pattern: /^remind me to/i,
    action: 'reminder.create',
    params: { priority: 'normal' },
    confidence: 0.90
  },
  {
    pattern: /^(take|capture) a (photo|picture|screenshot)/i,
    action: 'vision.capture',
    params: { mode: 'photo' },
    confidence: 0.98
  },
  {
    pattern: /^what('| i)s (this|that)/i,
    action: 'vision.identify',
    params: { detailed: false },
    confidence: 0.92
  }
];

export function matchFastPath(input: string): FastPathAction | null {
  for (const action of FAST_PATH_ACTIONS) {
    if (action.pattern.test(input)) {
      return action;
    }
  }
  return null;
}
```

### 2. Gateway Integration

```typescript
// src/gateway/vision-route.ts
import { matchFastPath } from '../visionclaw/fast-path-registry';

export async function handleVisionRequest(request: VisionRequest): Promise<Response> {
  // Check fast-path first
  const fastPath = matchFastPath(request.transcript);
  
  if (fastPath && fastPath.confidence > 0.90) {
    // Direct tool execution - no LLM
    const result = await executeToolDirect(fastPath.action, {
      ...fastPath.params,
      rawInput: request.transcript
    });
    
    return {
      type: 'fast-path',
      action: fastPath.action,
      result,
      latency: 'sub-100ms'
    };
  }
  
  // Fall back to full LLM reasoning
  return await routeToAgent(request);
}
```

### 3. Latency Benchmarks

| Path | Typical Latency | Use Case |
|------|-----------------|----------|
| Fast-Path | 50-100ms | Common, deterministic actions |
| LLM-Assisted | 500-2000ms | Complex, contextual actions |

## Confidence Thresholds

- **> 0.95:** Execute immediately without confirmation
- **0.85 - 0.95:** Execute with brief visual/audio confirmation
- **< 0.85:** Route to full LLM pipeline

## Extending Fast-Paths

```typescript
// Add new fast-path action
registerFastPath({
  pattern: /^(send|text|message) .+ (to|at) \w+/i,
  action: 'message.send',
  params: { platform: 'default' },
  confidence: 0.88
});
```

## A2UI Integration

Fast-path actions can push visual confirmations:

```typescript
if (fastPath.confidence < 0.95) {
  await a2ui.pushConfirmation({
    action: fastPath.action,
    preview: generatePreview(result),
    timeout: 3000 // Auto-dismiss after 3s
  });
}
```

## Testing

```typescript
// fast-path-registry.test.ts
describe('Fast-Path Matching', () => {
  it('matches "add milk to my list"', () => {
    const result = matchFastPath('add milk to my list');
    expect(result?.action).toBe('list.add');
  });
  
  it('returns null for ambiguous input', () => {
    const result = matchFastPath('something something');
    expect(result).toBeNull();
  });
});
```

## References

- Night Cycle Report: night_cycle_20260412_0100.md
- Latency Requirements: VisionClaw integration must maintain sub-second response times
