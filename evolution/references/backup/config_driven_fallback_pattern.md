# Pattern: Config-Driven Fallback Strategy

**Classification:** Configuration Pattern | **Safety Level:** Documentation Only | **Source:** night_cycle_20260412_0301.md

## Overview

The Config-Driven Fallback pattern replaces built-in fallback values with configuration-driven defaults to prevent configuration drift and ensure predictable behavior.

## Problem Statement

Built-in fallback models and hard-coded defaults can lead to:
- Configuration drift across environments
- Unexpected behavior when configs change
- Hidden dependencies on hard-coded values
- Difficulty in tracking where defaults come from

## Pattern Transformation

### Before (Anti-Pattern)

```typescript
// BEFORE: Built-in fallback
const fallbackModel = "gpt-4o";

function getModel(config?: UserConfig): string {
  return config?.model ?? fallbackModel; // Hidden fallback
}
```

### After (Config-Driven Pattern)

```typescript
// AFTER: Config-driven only
interface Config {
  activeMemory?: {
    model: string; // Required
  };
}

function getModel(config: Config): string {
  // Explicit error when config is missing
  const model = config.activeMemory?.model;
  if (!model) {
    throw new ConfigError("activeMemory.model is required");
  }
  return model;
}
```

## Implementation in Active Memory

From commits `6800579e` and `00d0dcfa`:

```typescript
// extensions/active-memory/index.ts
// BEFORE
const fallbackModel = config.activeMemory?.model ?? "gpt-4o";

// AFTER  
const model = config.activeMemory?.model;
if (!model) {
  throw new ConfigError("Missing activeMemory.model configuration");
}
```

## Benefits

1. **Explicit Dependencies** - All defaults are visible in config
2. **Environment Parity** - Same config validation across dev/staging/prod
3. **Easier Debugging** - Clear errors when config is missing
4. **Prevents Drift** - No hidden defaults that diverge from config

## CodeWiki Entry

**Pattern ID:** `patterns/config-driven-fallback`  
**Related Patterns:**
- `startup-context-extraction`
- `config-validation-pattern`
- `graceful-degradation-pattern`

## Implementation Guidelines

### DO:
- Make configuration values explicit and required
- Throw descriptive errors for missing required configs
- Document all required configuration fields
- Provide example configurations in documentation

### DON'T:
- Use hard-coded fallbacks that bypass configuration
- Have silent fallbacks that hide configuration issues
- Allow partial configuration to pass validation

## Related Commits

- `6800579e` - Remove built-in fallback model
- `7fbf0b30` - Active memory fallback cleanup
- `00d0dcfa` - Active memory config schema fallback fields

## References

- Error Handling Standardization: `error_handling_standardization_guide.md`
- SQLite Fallback Strategy: `sqlite_fallback_strategy.md`
