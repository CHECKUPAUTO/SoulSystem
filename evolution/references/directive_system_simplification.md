# Directive System Simplification

**Priority:** Medium (from 0230 report, S1)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0230

## Problem

The directive system has 8+ "narrow" test commits, suggesting the system is complex enough to warrant many edge cases. Imperative validation paths are numerous and hard to maintain.

## Proposal: Directive DSL / Schema-Driven Approach

Instead of imperative validation functions, use a declarative schema:

```typescript
// Current: scattered imperative validation
if (directive.type === 'model' && directive.value.includes(':')) {
  // complex validation logic
}

// Proposed: schema-driven validation
const directiveSchema = {
  model: { pattern: /^[a-z][\w.-]*\/[\w.-]+(:[\w.-]+)?$/, 
           examples: ['ollama/qwen3:cloud', 'openai/gpt-5'] },
  timeout: { type: 'number', min: 1, max: 3600, unit: 'seconds' },
  thinking: { enum: ['low', 'medium', 'high'] },
};
```

## Benefits

- Single source of truth for directive validation
- Auto-generatable help text and error messages
- Easier to add new directives without scattered code changes
- Testable via schema validation rather than imperative paths

## Migration Strategy

1. Define schemas for all current directives
2. Implement schema validator alongside existing imperative validation
3. Run both in parallel, compare results
4. Remove imperative validation once confidence is established

## Related References

- `pure_test_coverage_map.md` — Pure test migration tracking
- `test_ownership_map.md` — Test ownership mapping