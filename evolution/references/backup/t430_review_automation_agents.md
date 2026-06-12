# T430 Review Automation for Agents

**Priority:** P2 (MEDIUM)
**Source:** Night Cycle 2026-04-13 01:15
**Status:** Proposal

## Problem

The Codex harness went through 4+ manual review-fix cycles. Each review pass is essentially manual T430 — applying semantic crossover on the fix space. This is repetitive and could be partially automated.

## Proposal

Formalize IronReview T430 integration as an automated semantic review cycle for agent harness changes.

### T430 Review Cycle

```
1. Generate initial implementation
2. T430 Phase-Shift Review:
   a. Syntax fitness: lint, type-check, format
   b. Semantic fitness: correctness, edge cases, auth patterns
   c. Quality fitness: naming, structure, documentation
   d. Security fitness: injection, authz, data flow
3. Mutate based on review (crossover + mutation operators)
4. Repeat 2-3 cycles or until fitness plateau
5. Output final implementation + review report
```

### Mutation Operators for Auth/Security

- **Scope addition mutation:** Add missing OAuth scope checks
- **Input validation mutation:** Add parameter validation gaps
- **Error path mutation:** Add error handling for currently unhandled paths
- **Race condition mutation:** Add async locking where concurrent access is possible

### Fitness Functions

```typescript
interface T430FitnessResult {
  syntax: number;    // 0-1, lint + type-check
  semantic: number;  // 0-1, correctness + edge cases
  quality: number;   // 0-1, naming + structure
  security: number;  // 0-1, auth + injection resistance
  total: number;    // weighted average
}
```

## Benefits

- Reduces manual review rounds from 4+ to 1-2
- Catches auth/truthfulness issues earlier
- Systematic coverage of security mutation operators
- Reproducible review quality across different developers

## Related References

- `ironreview_t430_integration.md` — T430 algorithm integration guide
- `ironreview_t430_algorithm_guide.md` — T430 phase-shift algorithm details
- Codex harness review cycle analysis from 01:00 report