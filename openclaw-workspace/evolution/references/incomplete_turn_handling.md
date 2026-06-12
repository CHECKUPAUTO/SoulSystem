# Incomplete Turn Handling Pattern

**Source:** Night Cycle 2026-04-13 01:47 (GPT-5.4 runtime rollup #65219)
**Status:** Reference documentation
**Priority:** P1
**Updated:** 2026-04-13 (Night Cycle 01:47 — GPT-5.4 runtime rollup #65219 analysis)

## Upstream Reference

- Commit `26945dd`: agents: GPT-5.4 runtime completion rollup (#65219)
- Added `pi-embedded-runner/run.incomplete-turn.test.ts` (+135 lines) — incomplete turn handling
- Changes to `pi-embedded-runner/run.ts` (+26/-10) — agent runner improvements
- Test:impl ratio ~2.6:1 (healthy coverage)

## Cross-Reference

- See `codex_harness_integration_guide.md` for agent harness patterns
- See `circuit_breaker_pattern.md` for resilience patterns applicable to incomplete turn recovery
- The incomplete-turn pattern complements the agent idle watchdog system

## Pattern: Incomplete Turn Recovery

When an LLM agent fails to complete a turn (timeout, error, interrupted response), the system must handle partial output gracefully rather than discarding it or crashing.

### Key Principles

1. **Detect incompleteness**: Track turn completion status explicitly (not just "no error")
2. **Preserve partial work**: Buffer partial responses for potential resumption
3. **Communicate state**: Surface incomplete-turn status to callers
4. **Resume intelligently**: On retry, consider whether to resume from checkpoint or restart

### Implementation Notes (from Pi embedded runner)

- `run.incomplete-turn.test.ts` — 135 lines of coverage for incomplete turn handling
- Agent runner improvements for graceful partial-turn handling
- The Pi embedded runner (`pi-embedded-runner/run.ts`) now tracks turn completion status

### Cross-References

- Circuit breaker pattern (`circuit_breaker_pattern.md`) — for retry logic
- Session state management (`session_state_management_patterns.md`) — for checkpoint/resume
- Codex harness integration (`codex_harness_integration_guide.md`) — similar turn completion tracking

### Applicability

Any agent harness that processes multi-turn conversations should implement incomplete-turn detection and recovery. This applies to:
- Codex harness
- Pi embedded runner
- Claude Code harness
- Custom ACP harnesses