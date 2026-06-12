# Group-Scoped Action Participant Targeting Audit

**Created:** 2026-04-13 (Night Cycle auto-apply)
**Priority:** P1
**Source Reports:** night_cycle_20260413_0102.md
**Status:** Proposal — requires manual audit across channels

## Problem

The WhatsApp reaction fix (#65512) revealed a class of bug where group-scoped operations drop participant context. In group contexts, actions must carry explicit target participant metadata, but some code paths don't.

## Pattern: Participant-Targeted Actions

**Correct:** All group/channel-scoped actions should require an explicit `participant` or `targetAuthor` parameter.

```typescript
// ❌ Vulnerable — no participant targeting
async function sendReaction(channelId: string, emoji: string): Promise<void>

// ✅ Safe — explicit participant targeting
async function sendReaction(
  channelId: string,
  emoji: string,
  targetParticipant: string  // Required in group context
): Promise<void>
```

## Audit Targets

### WhatsApp
- ✅ Reaction sends (fixed in #65512)
- ❓ Message deletes in groups
- ❓ Pin/unpin operations
- ❓ Read receipts

### Discord
- ❓ Reaction adds/removes with target user
- ❓ Thread message routing
- ❓ Slash command responses in group context

### Slack
- ❓ Reaction adds with target user
- ❓ Thread reply routing
- ❓ Channel-wide broadcasts vs DMs

### Feishu/MS Teams
- ❓ Typing indicators with target participant
- ❓ Message action routing (pin, react, etc.)

## Recommendation

1. Add a lint rule or type constraint ensuring all group-scoped message methods require `participant` or `targetAuthor`
2. Create a shared `GroupActionContext` type:
   ```typescript
   interface GroupActionContext {
     channelId: string;
     participant: string; // Required for group context
     threadId?: string;
   }
   ```
3. Audit all channel plugins for missing participant targeting

## Related References

- Issue #65512: WhatsApp group reaction target participant fix
- `evolution/references/auth_pattern_audit.md`