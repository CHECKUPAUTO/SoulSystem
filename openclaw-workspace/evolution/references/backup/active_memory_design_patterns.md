# Active Memory Design Patterns

*Created: 2026-04-13 (Night Cycle)*
*Source: night_cycle_20260413_0001.md*

## Overview

Active Memory plugin provides opt-in sub-agent context retrieval before main reply generation. Three modes: `message`, `recent`, `full` with configurable prompt/thinking overrides.

## Configuration Surface

| Mode | Description | Recommended Preset |
|------|-------------|-------------------|
| `message` | Context from recent messages | Default prompt, low thinking |
| `recent` | Context from recent session history | Balanced prompt, medium thinking |
| `full` | Full context retrieval | Detailed prompt, high thinking |

## Design Recommendations

### 1. Preset Configurations
Ship with recommended presets per mode to reduce configuration surface:

```yaml
# Recommended presets
activeMemory:
  message:
    prompt: "Extract key facts and decisions from recent messages"
    thinking: low
  recent:
    prompt: "Summarize recent context including decisions and open questions"
    thinking: medium
  full:
    prompt: "Comprehensive context retrieval covering all relevant history"
    thinking: high
```

### 2. Auto-Tuning (`/memory tune`)
Consider a `/memory tune` command that auto-adjusts settings based on:
- Conversation length and complexity
- Token budget usage patterns
- Retrieval hit/miss ratios

### 3. Integration Points
- **VisionClaw**: Active Memory for context-aware glasses responses
- **the_well**: Scientific dataset context injection for domain-specific queries
- **IronReview**: Evolve memory retrieval strategies based on coverage metrics

## Cross-References
- `active_memory_integration_testing_guide.md` — Testing patterns
- `context_rehydration_pattern.md` — Session state restoration
- `codex_harness_integration_guide.md` — ACP harness memory considerations