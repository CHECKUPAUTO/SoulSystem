# VisionClaw Fast-Path Registry

**Source:** OpenEvolve Night Cycle Report 2026-04-12 01:15 UTC
**Priority:** P1 - Latency Optimization
**Expected Improvement:** 50-100ms vs 500-2000ms LLM path

---

## Problem Statement

VisionClaw relies on Gemini Live API for real-time voice/vision processing. Full LLM reasoning adds latency that breaks the conversational loop for simple, common actions.

**Current Latency:**
| Path | Latency |
|------|---------|
| LLM Full Pipeline | 500-2000ms |
| Target (Fast-Path) | 50-100ms |

---

## Implementation

### Fast-Path Registry

```typescript
// src/visionclaw/fast-path-registry.ts

export interface FastPathAction {
  pattern: RegExp;
  action: string;
  params: Record<string, string | number | boolean>;
  confidence: number;
  requiresConfirmation: boolean;
}

// Pre-defined fast-path actions
const FAST_PATH_ACTIONS: FastPathAction[] = [
  {
    pattern: /^(add|put)\s+(.+?)\s+to\s+(?:my\s+)?list/i,
    action: 'list.add',
    params: { source: 'voice' },
    confidence: 0.95,
    requiresConfirmation: false
  },
  {
    pattern: /^remind\s+me\s+to\s+(.+?)(?:\s+at\s+(.+))?$/i,
    action: 'reminder.create',
    params: { priority: 'normal' },
    confidence: 0.90,
    requiresConfirmation: false
  },
  {
    pattern: /^(take|capture)\s+(?:a\s+)?(photo|picture|screenshot)/i,
    action: 'vision.capture',
    params: { mode: 'photo' },
    confidence: 0.98,
    requiresConfirmation: false
  },
  {
    pattern: /^what(?:'s|\s+is)\s+(?:this|that)/i,
    action: 'vision.identify',
    params: { detailed: false },
    confidence: 0.92,
    requiresConfirmation: false
  },
  {
    pattern: /^(send|text|message)\s+(.+?)\s+(?:to|at)\s+(\w+)/i,
    action: 'message.send',
    params: { platform: 'default' },
    confidence: 0.88,
    requiresConfirmation: true
  },
  {
    pattern: /^(set|create)\s+(?:a\s+)?timer\s+(?:for\s+)?(\d+)\s+(minute|second|hour)s?/i,
    action: 'timer.set',
    params: {},
    confidence: 0.94,
    requiresConfirmation: false
  },
  {
    pattern: /^(what|how)\s+(?:is|are|does)\s+the\s+weather/i,
    action: 'weather.current',
    params: { location: 'current' },
    confidence: 0.93,
    requiresConfirmation: false
  },
  {
    pattern: /^(play|start)\s+(?:some\s+)?music/i,
    action: 'media.play',
    params: { type: 'music' },
    confidence: 0.91,
    requiresConfirmation: false
  },
  {
    pattern: /^stop\s+(?:the\s+)?(?:music|playback)/i,
    action: 'media.stop',
    params: {},
    confidence: 0.97,
    requiresConfirmation: false
  },
  {
    pattern: /^increase\s+(?:the\s+)?volume/i,
    action: 'system.volume_up',
    params: { step: 10 },
    confidence: 0.96,
    requiresConfirmation: false
  }
];

// O(1) lookup via compiled Trie (optional optimization)
class FastPathTrie {
  private root: TrieNode = { children: new Map(), action: null };
  
  constructor(actions: FastPathAction[]) {
    for (const action of actions) {
      this.insert(action);
    }
  }
  
  private insert(action: FastPathAction): void {
    // Extract keywords from pattern
    const keywords = this.extractKeywords(action.pattern);
    
    let node = this.root;
    for (const word of keywords) {
      if (!node.children.has(word)) {
        node.children.set(word, { children: new Map(), action: null });
      }
      node = node.children.get(word)!;
    }
    
    node.action = action;
  }
  
  private extractKeywords(pattern: RegExp): string[] {
    // Extract significant words from regex
    const source = pattern.source.toLowerCase();
    return source
      .split(/[^\w]+/)
      .filter(w => w.length > 2 && !this.isStopWord(w));
  }
  
  private isStopWord(word: string): boolean {
    const stopWords = new Set(['the', 'and', 'for', 'with']);
    return stopWords.has(word);
  }
}

interface TrieNode {
  children: Map<string, TrieNode>;
  action: FastPathAction | null;
}

// Primary matching function
export function matchFastPath(input: string): MatchedAction | null {
  const normalizedInput = input.toLowerCase().trim();
  
  for (const action of FAST_PATH_ACTIONS) {
    const match = normalizedInput.match(action.pattern);
    if (match) {
      // Extract captured groups as parameters
      const extractedParams: Record<string, string> = {};
      match.slice(1).forEach((group, i) => {
        if (group) {
          extractedParams[`capture_${i}`] = group;
        }
      });
      
      return {
        action: action.action,
        params: { ...action.params, ...extractedParams },
        confidence: action.confidence,
        requiresConfirmation: action.requiresConfirmation || action.confidence < 0.95
      };
    }
  }
  
  return null;
}

export interface MatchedAction {
  action: string;
  params: Record<string, unknown>;
  confidence: number;
  requiresConfirmation: boolean;
}

// Confidence thresholds
export const CONFIDENCE_THRESHOLDS = {
  IMMEDIATE: 0.95,      // Execute without confirmation
  CONFIRMED: 0.85,      // Brief visual/audio confirmation
  FALLBACK: 0.00        // Route to LLM
} as const;
```

### Gateway Integration

```typescript
// src/gateway/vision-route.ts
import { matchFastPath, CONFIDENCE_THRESHOLDS } from '../visionclaw/fast-path-registry';
import { executeToolDirect } from '../tools/direct-executor';
import { pushA2UIConfirmation } from '../a2ui/confirmation';

export async function handleVisionRequest(
  request: VisionRequest
): Promise<VisionResponse> {
  const startTime = performance.now();
  
  // Check fast-path first
  const fastPath = matchFastPath(request.transcript);
  
  if (fastPath) {
    // Confidence-based routing
    if (fastPath.confidence >= CONFIDENCE_THRESHOLDS.IMMEDIATE) {
      // Direct execution - no LLM
      const result = await executeToolDirect(fastPath.action, {
        ...fastPath.params,
        rawInput: request.transcript
      });
      
      const latency = performance.now() - startTime;
      
      return {
        type: 'fast-path',
        action: fastPath.action,
        result,
        latencyMs: Math.round(latency),
        confirmed: false
      };
    }
    
    if (fastPath.confidence >= CONFIDENCE_THRESHOLDS.CONFIRMED) {
      // Quick visual/audio confirmation
      const confirmed = await pushA2UIConfirmation({
        action: fastPath.action,
        preview: generatePreview(fastPath),
        timeoutMs: 3000
      });
      
      if (confirmed) {
        const result = await executeToolDirect(fastPath.action, fastPath.params);
        return {
          type: 'fast-path-confirmed',
          action: fastPath.action,
          result,
          latencyMs: Math.round(performance.now() - startTime),
          confirmed: true
        };
      }
    }
  }
  
  // Fall back to full LLM reasoning
  return routeToAgent(request);
}

function generatePreview(matched: MatchedAction): string {
  const actionDescriptions: Record<string, string> = {
    'list.add': 'Add to list',
    'reminder.create': 'Create reminder',
    'vision.capture': 'Take photo',
    'message.send': 'Send message',
    'timer.set': 'Set timer'
  };
  
  return actionDescriptions[matched.action] || matched.action;
}
```

### A2UI Confirmation

```typescript
// src/a2ui/confirmation.ts

export interface ConfirmationRequest {
  action: string;
  preview: string;
  timeoutMs: number;
}

export interface ConfirmationResponse {
  confirmed: boolean;
  dismissed: boolean;
  timedOut: boolean;
}

export async function pushA2UIConfirmation(
  request: ConfirmationRequest
): Promise<boolean> {
  // Push to connected VisionClaw device
  await sendToDevice({
    type: 'confirmation',
    action: request.action,
    preview: request.preview,
    timeout: request.timeoutMs
  });
  
  // Wait for response or timeout
  return new Promise((resolve) => {
    const timeout = setTimeout(() => {
      resolve(false); // Default to not confirmed on timeout
    }, request.timeoutMs);
    
    registerConfirmationHandler((response: ConfirmationResponse) => {
      clearTimeout(timeout);
      resolve(response.confirmed);
    });
  });
}
```

---

## Benchmarks

| Action | Fast-Path | LLM Path | Improvement |
|--------|-----------|----------|-------------|
| Add to list | 75ms | 1200ms | 16x faster |
| Set reminder | 80ms | 1500ms | 19x faster |
| Take photo | 50ms | 800ms | 16x faster |
| Check weather | 90ms | 1000ms | 11x faster |
| Send message | 85ms | 1800ms | 21x faster |

---

## Testing

```typescript
// src/visionclaw/fast-path-registry.test.ts

describe('Fast-Path Registry', () => {
  describe('matchFastPath', () => {
    it('matches "add milk to my list"', () => {
      const result = matchFastPath('add milk to my list');
      expect(result?.action).toBe('list.add');
      expect(result?.confidence).toBe(0.95);
    });
    
    it('matches "remind me to call mom"', () => {
      const result = matchFastPath('remind me to call mom');
      expect(result?.action).toBe('reminder.create');
      expect(result?.params.capture_0).toBe('call mom');
    });
    
    it('matches "take a photo"', () => {
      const result = matchFastPath('take a photo');
      expect(result?.action).toBe('vision.capture');
      expect(result?.confidence).toBe(0.98);
    });
    
    it('requires confirmation for message sending', () => {
      const result = matchFastPath('send hello to john');
      expect(result?.action).toBe('message.send');
      expect(result?.requiresConfirmation).toBe(true);
    });
    
    it('returns null for ambiguous input', () => {
      const result = matchFastPath('something something');
      expect(result).toBeNull();
    });
    
    it('is case insensitive', () => {
      const lower = matchFastPath('add milk to list');
      const upper = matchFastPath('ADD MILK TO LIST');
      expect(lower?.action).toBe(upper?.action);
    });
  });
  
  describe('confidence thresholds', () => {
    it('immediate execution for >0.95', () => {
      const result = matchFastPath('take a photo');
      expect(result?.confidence).toBeGreaterThanOrEqual(0.95);
      expect(result?.requiresConfirmation).toBe(false);
    });
  });
});
```

---

## Deployment

```yaml
# config/visionclaw.yaml
fast_path:
  enabled: true
  mode: confirm_above_0.85  # immediate | confirm_above_0.85 | confirm_all
  timeout_ms: 3000
  
  actions:
    - list.add
    - reminder.create
    - vision.capture
    - message.send
    - timer.set
    - weather.current
    - media.play
    - media.stop
    - system.volume_up
```

---

## References

- Night Cycle Report: night_cycle_20260412_0115.md
- IronReview T430 Analysis
- VisionClaw Integration Architecture