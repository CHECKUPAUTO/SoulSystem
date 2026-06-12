# Config Metadata Priority Field

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P2
**Source Reports:** night_cycle_20260413_0102.md
**Status:** Proposal — requires gateway implementation

## Problem

Channel metadata loading relies on implicit filesystem ordering, causing intermittent race conditions. The fix for "stabilize bundled channel metadata loading" addressed this, but a more robust solution would make ordering explicit.

## Proposed Pattern: Priority Field in Metadata Manifests

```typescript
interface ChannelMetadataManifest {
  id: string;
  priority: number; // Explicit load order (lower = earlier)
  // ... existing fields
}

// Loading becomes deterministic:
const orderedMetadata = manifests
  .sort((a, b) => a.priority - b.priority)
  .map(loadMetadata);
```

### Priority Defaults

| Channel | Priority | Rationale |
|---------|----------|-----------|
| telegram | 100 | Primary channel, load first |
| discord | 200 | Secondary, high priority |
| whatsapp | 300 | Messaging channel |
| slack | 400 | Enterprise channel |
| webchat | 500 | Internal channel |
| feishu | 600 | Integration channel |
| msteams | 700 | Integration channel |

### Benefits

1. **Deterministic load ordering** — no more FS-dependent behavior
2. **Testable** — can verify load order in unit tests
3. **Configurable** — users can override priorities for their deployment
4. **Debuggable** — priority conflicts are visible in logs

## Related References

- OpenClaw commit: "Config: stabilize bundled channel metadata loading"
- `evolution/references/config_driven_fallback_pattern.md`