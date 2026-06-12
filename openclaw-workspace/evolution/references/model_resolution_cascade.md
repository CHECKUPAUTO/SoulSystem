# Model Resolution Cascade Pattern - Priority-Based Fallback Chain

**Source:** OpenEvolve Night Cycle Report 2026-04-12 04:45  
**Author:** Pattern identified from commits 6800579e, 00d0dcfa  
**Priority:** P1 - High Priority  
**Classification:** Architecture Pattern / Config Pattern

---

## Problem Statement

**Hardcoded Fallback Model Anti-Pattern:** Active-memory previously relied on a built-in fallback model (`github-copilot/gpt-5.4-mini`) when primary models failed. This caused:
- Deployment rigidity (cannot change fallback without code release)
- Silent failures (fallback model may not match requirements)
- Configuration drift (runtime behavior differs from config expectations)
- Vendor lock-in (hardcoded provider)

**T430 Assessment:** Config-driven architecture violation. Pattern identified as candidate for "remove built-in fallbacks" refactoring.

**Evidence from Night Cycle:**
```
6800579e - fix(active-memory): remove built-in fallback model
00d0dcfa - fix(active-memory): config schema alignment for fallback
7fbf0b304b - fix(active-memory): remove built-in fallback model (duplicate)
```

---

## Solution: Config-Driven Model Resolution

### Core Concept

**Priority-based model selection** with explicit configuration:

```
Resolution Cascade (highest to lowest priority):

1. Plugin-configured model (extension specific)
2. Session-configured model (per-session override)
3. Agent primary model (agent default)
4. Config fallback model (global fallback)
5. Fail gracefully (no model)
```

### Implementation

#### 1. Config Schema

```typescript
// Config schema extension for active-memory
interface ActiveMemoryConfig {
  // Primary model for active-memory operations
  model?: string;
  
  // Fallback model when primary is unavailable
  modelFallback?: string;
  
  // Fallback behavior policy
  modelFallbackPolicy: 'default-remote' | 'resolved-only';
  
  // Timeout for model resolution
  modelTimeoutMs?: number;
}

// Example config.yaml
active-memory:
  model: "ollama/qwen3-coder-next:cloud"
  modelFallback: "ollama/gemma4:9b-cloud"
  modelFallbackPolicy: "resolved-only"
  modelTimeoutMs: 15000
```

#### 2. Resolution Function

```typescript
// src/config/model-resolution.ts

export interface ModelCandidate {
  model: string;
  source: 'plugin' | 'session' | 'agent-primary' | 'config-fallback';
  priority: number;
}

export interface ResolvedModel {
  model: string;
  source: ModelCandidate['source'];
  wasFallback: boolean;
}

/**
 * Resolves model using priority cascade
 * Highest priority candidate wins
 */
export function resolveModelCascade(
  candidates: ModelCandidate[],
  options: ResolutionOptions = {}
): ResolvedModel | null {
  const {
    allowFallback = true,
    requireLocal = false,
  } = options;

  // Sort by priority (highest first)
  const sorted = [...candidates].sort((a, b) => b.priority - a.priority);

  for (const candidate of sorted) {
    // Check if model is available
    if (isModelAvailable(candidate.model, { requireLocal })) {
      return {
        model: candidate.model,
        source: candidate.source,
        wasFallback: candidate.source === 'config-fallback',
      };
    }
  }

  // No viable model found
  if (!allowFallback) {
    return null;
  }

  // Last resort: fail gracefully
  return null;
}

// Priority constants (higher = more preferred)
export const MODEL_PRIORITIES = {
  PLUGIN: 100,        // Extension-specific override
  SESSION: 90,        // Per-session configuration
  AGENT_PRIMARY: 80,  // Agent's default model
  CONFIG_FALLBACK: 10, // Global fallback
} as const;

/**
 * Checks if a model is available for use
 */
function isModelAvailable(
  model: string,
  options: { requireLocal?: boolean }
): boolean {
  // Check if model is running/available
  const available = getAvailableModels();
  
  if (options.requireLocal && isRemoteModel(model)) {
    return false;
  }
  
  return available.includes(model);
}
```

#### 3. Active-Memory Integration

```typescript
// extensions/active-memory/index.ts

export class ActiveMemoryExtension {
  private config: ActiveMemoryConfig;

  constructor(config: ActiveMemoryConfig) {
    this.config = {
      modelFallbackPolicy: 'resolved-only', // Default
      ...config,
    };
  }

  /**
   * Resolves model for recall operations
   */
  private async resolveModel(context: SessionContext): Promise<ResolvedModel> {
    const candidates: ModelCandidate[] = [
      // 1. Plugin-configured model
      ...(this.config.model ? [{
        model: this.config.model,
        source: 'plugin' as const,
        priority: MODEL_PRIORITIES.PLUGIN,
      }] : []),

      // 2. Session-configured model
      ...(context.model ? [{
        model: context.model,
        source: 'session' as const,
        priority: MODEL_PRIORITIES.SESSION,
      }] : []),

      // 3. Agent primary model (from session agent)
      ...(context.agent?.primaryModel ? [{
        model: context.agent.primaryModel,
        source: 'agent-primary' as const,
        priority: MODEL_PRIORITIES.AGENT_PRIMARY,
      }] : []),

      // 4. Config fallback (if policy allows)
      ...(this.shouldUseFallback() && this.config.modelFallback ? [{
        model: this.config.modelFallback,
        source: 'config-fallback' as const,
        priority: MODEL_PRIORITIES.CONFIG_FALLBACK,
      }] : []),
    ];

    const resolved = resolveModelCascade(candidates, {
      allowFallback: this.shouldUseFallback(),
    });

    if (!resolved) {
      throw new ModelResolutionError('No viable model found in cascade');
    }

    return resolved;
  }

  private shouldUseFallback(): boolean {
    return this.config.modelFallbackPolicy === 'default-remote';
  }
}
```

---

## Comparison: Before vs After

### Before (Hardcoded)

```typescript
// extensions/active-memory/index.ts (old)

const FALLBACK_MODEL = 'github-copilot/gpt-5.4-mini';

export async function recallWithFallback(primaryModel: string) {
  try {
    return await recall(primaryModel);
  } catch (err) {
    // Silent fallback to hardcoded model
    console.warn(`Primary model failed, using fallback: ${FALLBACK_MODEL}`);
    return await recall(FALLBACK_MODEL);
  }
}
```

**Problems:**
- Cannot customize fallback
- Hidden behavior
- Provider lock-in
- Code change required for new fallback

### After (Config-Driven)

```typescript
// extensions/active-memory/index.ts (new)

export async function recallWithFallback(
  context: SessionContext,
  config: ActiveMemoryConfig
) {
  const resolved = await resolveModel(context, config);
  
  if (!resolved) {
    throw new Error('No model available for recall');
  }

  // Log fallback usage for observability
  if (resolved.wasFallback) {
    console.info(
      `Using fallback model: ${resolved.model} ` +
      `(source: ${resolved.source})`
    );
  }

  return await recall(resolved.model);
}
```

**Benefits:**
- Configurable fallback chain
- Transparent behavior
- No vendor lock-in
- Runtime customization

---

## Extension Points

### 1. Health-Aware Resolution

```typescript
// Check model health before selecting
export async function resolveModelWithHealthCheck(
  candidates: ModelCandidate[]
): Promise<ResolvedModel | null> {
  for (const candidate of candidates) {
    const health = await checkModelHealth(candidate.model);
    
    if (health.status === 'healthy') {
      return {
        model: candidate.model,
        source: candidate.source,
        wasFallback: candidate.source === 'config-fallback',
      };
    }
    
    // Skip unhealthy models
    console.warn(
      `Skipping unhealthy model ${candidate.model}: ${health.reason}`
    );
  }
  
  return null;
}
```

### 2. Cost-Aware Resolution

```typescript
interface CostWeights {
  inputTokens: number;
  outputTokens: number;
}

export async function resolveModelWithCostOptimization(
  candidates: ModelCandidate[],
  expectedUsage: CostWeights
): Promise<ResolvedModel | null> {
  const costs = await Promise.all(
    candidates.map(async c => ({
      ...c,
      cost: await estimateCost(c.model, expectedUsage),
    }))
  );
  
  // Sort by cost (cheapest first) among available models
  const available = costs
    .filter(c => isModelAvailable(c.model))
    .sort((a, b) => a.cost - b.cost);
  
  return available[0] ?? null;
}
```

### 3. Latency-Aware Resolution

```typescript
// Prefer faster models for time-sensitive operations
export async function resolveModelWithLatencyPreference(
  candidates: ModelCandidate[],
  maxLatencyMs: number
): Promise<ResolvedModel | null> {
  for (const candidate of candidates) {
    const latency = await measureLatency(candidate.model);
    
    if (latency < maxLatencyMs) {
      return {
        model: candidate.model,
        source: candidate.source,
        wasFallback: candidate.source === 'config-fallback',
      };
    }
  }
  
  return null;
}
```

---

## Configuration Schema

```typescript
// Zod schema for validation
import { z } from 'zod';

export const ModelResolutionConfigSchema = z.object({
  model: z.string().optional(),
  modelFallback: z.string().optional(),
  modelFallbackPolicy: z.enum(['default-remote', 'resolved-only']),
  modelTimeoutMs: z.number().min(1000).max(60000).optional(),
});

export type ModelResolutionConfig = z.infer<typeof ModelResolutionConfigSchema>;

// Validate at plugin load time
export function validateConfig(
  config: unknown
): ModelResolutionConfig {
  return ModelResolutionConfigSchema.parse(config);
}
```

---

## Migration Guide

### For Extension Authors

1. **Remove hardcoded fallbacks:**
   ```typescript
   // Remove this
   const FALLBACK_MODEL = 'vendor/model-name';
   ```

2. **Add config options:**
   ```typescript
   // Add to your config schema
   interface MyExtensionConfig {
     modelFallback?: string;
     modelFallbackPolicy: 'default-remote' | 'resolved-only';
   }
   ```

3. **Use resolution cascade:**
   ```typescript
   const resolved = resolveModelCascade([
     { model: config.model, source: 'plugin', priority: 100 },
     { model: config.modelFallback, source: 'config-fallback', priority: 10 },
   ]);
   ```

### For Users

Update your `config.yaml`:

```yaml
# Before (no fallback control)
active-memory:
  model: "ollama/qwen3-coder-next:cloud"

# After (explicit fallback)
active-memory:
  model: "ollama/qwen3-coder-next:cloud"
  modelFallback: "ollama/gemma4:9b-cloud"
  modelFallbackPolicy: "resolved-only"
```

---

## Testing

```typescript
// test/model-resolution.test.ts

describe('resolveModelCascade', () => {
  it('should select highest priority available model', () => {
    const candidates: ModelCandidate[] = [
      { model: 'low-model', source: 'config-fallback', priority: 10 },
      { model: 'high-model', source: 'plugin', priority: 100 },
    ];
    
    const result = resolveModelCascade(candidates);
    
    expect(result?.model).toBe('high-model');
    expect(result?.source).toBe('plugin');
  });

  it('should skip unavailable models', () => {
    const candidates: ModelCandidate[] = [
      { model: 'unavailable', source: 'plugin', priority: 100 },
      { model: 'fallback', source: 'config-fallback', priority: 10 },
    ];
    
    // Mock unavailable model
    jest.spyOn(modelUtils, 'isModelAvailable')
      .mockImplementation(m => m !== 'unavailable');
    
    const result = resolveModelCascade(candidates);
    
    expect(result?.model).toBe('fallback');
    expect(result?.wasFallback).toBe(true);
  });

  it('should return null when no models available', () => {
    const candidates: ModelCandidate[] = [
      { model: 'unavailable1', source: 'plugin', priority: 100 },
      { model: 'unavailable2', source: 'config-fallback', priority: 10 },
    ];
    
    jest.spyOn(modelUtils, 'isModelAvailable').mockReturnValue(false);
    
    const result = resolveModelCascade(candidates);
    
    expect(result).toBeNull();
  });
});
```

---

## Related Patterns

- **Config-Driven Fallback Pattern**: `config_driven_fallback_pattern.md`
- **Startup Context Pattern**: `startup_context_pattern_v2.md`
- **Active-Memory Integration**: `active_memory_integration_testing_guide.md`

---

## References

- Night Cycle Report: `night_cycle_20260412_0445.md`
- Commits: `6800579e`, `00d0dcfa`, `7fbf0b304b`
- Pattern Source: `config_driven_fallback_pattern.md`

---

*Generated by OpenEvolve Auto-Apply*  
*Classification: P1 High Priority Architecture Pattern*  
*Credit: Active-memory config-driven architecture refactoring*
