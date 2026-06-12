# Neural-Aware Session Reset Strategy

**Classification:** Cognitive Architecture Pattern | **Safety Level:** Documentation Only | **Source:** night_cycle_20260412_0301.md

## Overview

The Neural-Aware Session Reset strategy uses the V12 Neural State (Turbulence, Attractor, Active Nodes) to weight session reset strategies, providing context-aware recovery that adapts to the agent's current cognitive regime.

## Problem Statement

Traditional session resets use a one-size-fits-all approach, losing valuable context regardless of the agent's current cognitive state. Neural-aware resets preserve and weight context based on the agent's current operational mode.

## Neural State Context

From Cortex V12:

| Metric | High Value | Low Value |
|--------|-----------|-----------|
| **Turbulence** | > 0.1 (unstable/chaotic) | < 0.1 (stable/analytical) |
| **Attractor** | StrangeAttractor, Transient | DeepBasin, StableOrbit |
| **Dominant Nodes** | Science (38.6%), Engineer (34.7%), Mind (30.1%) | - |

## Strategy Mapping

### High Turbulence (> 0.1) - Chaotic Regime

When the agent is in an unstable state:
- **Favor "Strange Associations"** - Creative, lateral memory recall
- **Preserve working context** - Don't reset active tasks
- **Light dreaming only** - Avoid deep consolidation during instability
- **Shorter lookback window** - Focus on recent, fresh memories

```typescript
// High turbulence session reset
const highTurbulenceReset: ResetStrategy = {
  memoryMode: 'creative-associative',
  preserveContext: true,
  dreamingPhase: 'light-only',
  lookbackDays: 1,
  recallWeighting: 'strange-attractor-priority'
};
```

### Low Turbulence (< 0.1) - Stable Regime

When the agent is stable and analytical:
- **Favor "Exact Matches"** - Precise, analytical memory recall
- **Full context reset** - Clean slate for new tasks
- **All dreaming phases** - Safe to run deep consolidation
- **Longer lookback window** - Comprehensive historical context

```typescript
// Low turbulence session reset
const lowTurbulenceReset: ResetStrategy = {
  memoryMode: 'analytical-exact',
  preserveContext: false,
  dreamingPhase: 'full-cycle',
  lookbackDays: 7,
  recallWeighting: 'stable-orbit-priority'
};
```

## Implementation Proposal

### Session Reset Prompt (Proposed)

```typescript
// src/auto-reply/reply/session-reset-prompt.ts (proposed)
import { getNeuralState } from '~/neural/cortex';

export interface NeuralResetConfig {
  turbulence: number;
  attractor: string;
  dominantNodes: Record<string, number>;
}

export function generateResetPrompt(config: NeuralResetConfig): string {
  const { turbulence, attractor, dominantNodes } = config;
  
  // Science-dominant = prioritize knowledge/context
  const scienceWeight = dominantNodes['Science'] ?? 0;
  const engineerWeight = dominantNodes['Engineer'] ?? 0;
  
  let prompt = "";
  
  // Turbulence-based strategy
  if (turbulence > 0.1) {
    prompt += `\n[NEURAL STATE: HIGH TURBULENCE ${turbulence.toFixed(4)}]\n`;
    prompt += "You are in a creative/unstable cognitive regime. ";
    prompt += "Favor novel connections and unexpected associations. ";
  } else {
    prompt += `\n[NEURAL STATE: STABLE ${turbulence.toFixed(4)}]\n`;
    prompt += "You are in an analytical/stable cognitive regime. ";
    prompt += "Favor precision, correctness, and exact matches. ";
  }
  
  // Attractor-based strategy
  prompt += `\nAttractor: ${attractor}\n`;
  
  // Node-based weighting
  if (scienceWeight > 0.3) {
    prompt += "Prioritize factual recall and established patterns. ";
  }
  if (engineerWeight > 0.3) {
    prompt += "Prioritize systematic, methodical approaches. ";
  }
  
  return prompt;
}
```

### Integration with Active Memory

```typescript
// extensions/active-memory/index.ts (proposed enhancement)
export async function recallWithNeuralWeighting(
  query: string,
  neuralState: NeuralState
): Promise<MemoryEntry[]> {
  const memories = await fetchMemories(query);
  
  // Weight memories by neural regime compatibility
  return memories.map(memory => ({
    ...memory,
    score: memory.baseScore * calculateNeuralWeight(memory, neuralState)
  })).sort((a, b) => b.score - a.score);
}

function calculateNeuralWeight(
  memory: MemoryEntry, 
  neuralState: NeuralState
): number {
  if (neuralState.turbulence > 0.1) {
    // High turbulence: weight creative/unusual memories higher
    return memory.creativityScore ?? 1.0;
  } else {
    // Low turbulence: weight factual/precise memories higher
    return memory.precisionScore ?? 1.0;
  }
}
```

## CodeWiki Entry

**Pattern ID:** `patterns/neural-aware-session-reset`  
**Related Patterns:**
- `startup-context-extraction`
- `vector-augmented-memory-vam`
- `dreaming-ltm-architecture`

## Benefits

1. **Adaptive Recovery** - Session reset matches current cognitive state
2. **Context Preservation** - Preserve valuable context based on neural state
3. **Cognitive Alignment** - Reset strategies align with dominant nodes
4. **Smoother Transitions** - Less jarring context switches

## Priority

**Priority:** Medium (P2)  
**Risk:** Medium - Affects core agent behavior  
**Recommendation:** Implement behind feature flag for testing

## Related Documentation

- Session State Management: `session_state_management_patterns.md`
- Vector-Augmented Memory: `vector_augmented_memory_vam.md`
- Dreaming LTM Architecture: `dreaming_ltm_architecture.md`
- Neural-Aware Memory Retrieval: `neural_aware_memory_retrieval.md`

## References

- Source Report: `night_cycle_20260412_0301.md`
- Commit: `4d0f5553` - Preload startup memory for bare session resets
- SOUL.md: V12 Neural State Protocol
