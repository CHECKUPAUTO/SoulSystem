# Performance Optimization Patterns - April 2026

**Source:** Night Cycle Analysis (7 reports from 2026-04-11 07:16-09:45 UTC)
**Status:** Documented patterns from recent commits

---

## 1. Static Capability Fast-Path Pattern

**Location:** `doctor/channel-capabilities.ts`, `outbound/target-normalization.ts`
**Purpose:** Eliminate plugin registry lookups for built-in channels

### Pattern
```typescript
const STATIC_CONFIGS: Record<string, Config> = {
  discord: { /* defaults */ },
  slack: { /* defaults */ },
  // ... built-in channels
};

export function getConfig(channel: string): Config {
  // Fast path: O(1) lookup
  const static = STATIC_CONFIGS[channel];
  if (static) return static;
  
  // Slow path: plugin registry (for custom channels)
  return pluginRegistry.getConfig(channel);
}
```

### Benefits
- Eliminates plugin registry lookup (~0.5-2ms per call)
- Compile-time guarantees for built-in channels
- Maintains extensibility via fallback

### When to Use
- Configuration rarely changes at runtime
- Built-in defaults cover 80% of cases
- Plugin registry lookup is measurable overhead

---

## 2. Plugin Barrel Avoidance

**Commits:** 455535a4, 28291eba, 27212458, f9afdf0a

### Anti-Pattern (Avoid)
```typescript
// Barrel import - requires loading entire plugin index
import { normalizeChannelId } from "../../channels/plugins/index.js";
```

### Pattern (Prefer)
```typescript
// Direct registry access
import { normalizeAnyChannelId } from "../../channels/registry.js";
```

### Benefits
- Reduces module resolution overhead
- Faster cold-start times
- Clearer dependency graph

---

## 3. Test Import Narrowing

**Purpose:** Reduce test overhead by mocking specific functions rather than modules

### Pattern
```typescript
// Before: Broad mock
jest.mock("@/queue");

// After: Surgical mock
jest.mock("@/queue/validation", () => ({
  validateDirective: jest.fn()
}));
```

### Benefits
- Faster test execution
- Reduced test fragility
- Clearer test intent

---

## 4. Async Tool Execution Framework

**Rationale:** High latency for multi-step tool chains in agent loops

### Recommended Pattern
```typescript
// Promise-based tool calls with immediate acknowledgement
interface AsyncToolCall {
  taskId: string;
  execute(): Promise<TaskResult>;
  status: 'pending' | 'running' | 'completed' | 'failed';
}

// Similar to VisionClaw's execute(task:) pattern
```

---

## 5. Memory Metrics Integration

**Recommendation:** Add memory pressure detection to `/status` endpoint

### Implementation Pattern
```typescript
interface StatusResponse {
  // ... existing fields
  memory: {
    used: number;
    total: number;
    sessions: number;
    pressure: 'normal' | 'elevated' | 'critical';
  };
}
```

---

## Metrics (April 2026 Commits)

| Metric | Count | Trend |
|--------|-------|-------|
| Performance Optimizations | 15+ | ↗️ Growing |
| Plugin Registry Hot-Path Removals | -8 | ↘️ Shrinking |
| Test Narrowing Commits | 12 | ↗️ Improving |
| Static Fast-Paths | 5+ | ↗️ Growing |

---

## References

- Night Cycle Reports: night_cycle_20260411_0716.md, night_cycle_20260411_0732.md
- Related: `performance_optimization_patterns.md` (base patterns)
- IronReview T430: `ironreview_t430_integration.md`
