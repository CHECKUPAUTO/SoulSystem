# GPT-5 Parity Gate Hardening Pattern

**Priority:** P0 (Quality Assurance)  
**Source:** Night Cycle 2026-04-13 06:15  
**Status:** Documented — upstream commit `b13844732e`  

## Problem

GPT-5.4 parity tests had a fake-progress loophole: scenarios only validated prose output, allowing models to fabricate task completion without actually calling tools.

## Solution Applied (Upstream)

Commit `b13844732e` closes this loophole by requiring:

1. **Tool call verification** — `source-docs-discovery-report` now requires at least one `read` tool call tied to the worked/failed/blocked prompt, verified via `/debug/requests` on the mock server
2. **Delegation evidence** — `subagent-handoff` requires `sessions_spawn` delegation evidence, not just labeled prose sections
3. **New scenarios added**: `instruction-followthrough-repo-contract.md` (127 lines), `config-restart-capability-flip.md`, plus updates to `memory-recall.md`, `image-understanding-attachment.md`, `subagent-fanout-synthesis.md`

## Pattern: Tool-Call Assertion via Debug Seam

```typescript
// After test execution, query mock server's debug endpoint
const requests = await fetch('/debug/requests');
const toolCalls = requests.filter(r => r.plannedToolName === expectedTool);

assert(toolCalls.length >= 1, 
  `Expected at least one ${expectedTool} call, got ${toolCalls.length}`);
```

This pattern uses the `/debug/requests` seam to assert `plannedToolName`, providing objective evidence that the model invoked tools rather than fabricating results in prose.

## Recommendations for Extension

1. **Apply to all parity scenarios** — Scenarios that only check prose output should be flagged as `prose-only: true` in frontmatter and excluded from hard parity gates
2. **Negative test cases** — Verify that a model fabricating completion (responding "Worked" without tool calls) actually fails assertions
3. **Scenario documentation** — New scenarios should include header summaries for discoverability

## Cross-References

- `codex_harness_integration_guide.md` — Related harness patterns
- `codex_harness_circuit_breaker.md` — Resilience patterns for agent execution
- `barrel_bypass_campaign_tracker.md` — Concurrent structural improvement campaign

## T430 Fitness Score

- Syntax: 0.88
- Semantic: 0.82
- Quality: 0.90
- Security: 0.95
- **Total: 0.89**