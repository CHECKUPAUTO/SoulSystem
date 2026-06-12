# Target-Entry Preference Migration

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P1
**Source Reports:** night_cycle_20260413_0045.md
**Status:** Proposal — architectural tracking document

## Context

Tak Hoffman's 10+ commit sweep refactoring command handlers to "prefer target entry" pattern represents a major architectural shift in OpenClaw: from implicit session targeting to explicit target routing.

## Before vs After

### Before: Implicit Session
```typescript
// Commands operate on "current session" implicitly
async function handleStatusCommand(ctx: CommandContext) {
  const session = getSession(); // Which session? Unclear!
  return session.status;
}
```

### After: Explicit Target Entry
```typescript
// Commands explicitly resolve which session/context they target
async function handleStatusCommand(ctx: CommandContext, target: SessionTarget) {
  const session = resolveTarget(target); // Explicit target resolution
  return session.status;
}
```

## Migration Status: ~70%

### Completed (from recent commits)
- Usage footer, fast status, inline status
- Tools wrapper, status wrapper
- Compact counters
- Subagent spawn/info
- Reset hooks
- BTW command, models command
- Plugin commands, command system prompt

### Remaining Targets
- Session export/import commands
- Heartbeat dispatch paths
- Any remaining `getContext()` calls that don't validate target
- Agent binding commands

## Why This Matters

**Bug Class Eliminated:** Cross-session contamination where subagents, compacted sessions, or cross-session operations hit wrong state.

**Security Improvement:** Explicit target validation prevents the same class of bugs as the device-pairing wrong-requester issue (#64160).

## Related References

- `evolution/references/auth_pattern_audit.md`
- `evolution/references/narrow_surface_pattern.md`
- `evolution/references/explicit_seams_pattern.md`
- Issue #64160: Device pairing wrong requester