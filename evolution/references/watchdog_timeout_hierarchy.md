# Watchdog Timeout Hierarchy

**Source:** Night cycle report 2026-04-13 (0433)
**Status:** Reference documentation
**Priority:** P2

---

## Problem

The LLM idle watchdog was overriding explicit user-configured timeouts with defaults, causing premature termination of long-running agents.

**Fix commit:** `7f2814fc4a` — `agents: honor explicit run timeout for LLM idle watchdog`

---

## Timeout Resolution Hierarchy

When resolving how long before an idle watchdog triggers, the following priority order **must** be respected:

| Priority | Source | Example | Override? |
|----------|--------|---------|-----------|
| 1 (highest) | Explicit per-run config | `runTimeoutMs: 300000` | Never overridden |
| 2 | Model/provider-specific default | GPT-5 models get longer timeouts | Overridden by tier 1 |
| 3 | Agent-level default | `agents.defaults.timeoutSeconds` | Overridden by tiers 1-2 |
| 4 (lowest) | System-wide fallback | Hard-coded minimum | Overridden by tiers 1-3 |

### Resolution Algorithm

```typescript
function resolveTimeout(run: AgentRun, agent: AgentConfig, model: ModelConfig): number {
  // Tier 1: Explicit run config — always wins
  if (run.runTimeoutMs) return run.runTimeoutMs

  // Tier 2: Model-specific default
  if (model.idleTimeoutMs) return model.idleTimeoutMs

  // Tier 3: Agent-level default
  if (agent.timeoutSeconds) return agent.timeoutSeconds * 1000

  // Tier 4: System fallback
  return SYSTEM_DEFAULT_TIMEOUT_MS
}
```

### Logging Requirement

When a timeout fires, log which tier was used:

```
[watchdog] Idle timeout triggered (300000ms, source=run-config)
```

This makes debugging timeout issues trivial — you can always see why a particular value was chosen.

---

## Related Patterns

- `config_driven_fallback_pattern.md` — removing built-in fallbacks for config-driven architecture
- `adaptive_idle_timeout.md` — model-aware adaptive timeout strategy
- `unified_timeout_config_schema.md` — unified timeout configuration object

## Open Questions

- Should Tier 2 (model-specific) be configurable per provider, or hardcoded?
- Should there be a maximum cap even on explicit timeouts to prevent runaway agents?