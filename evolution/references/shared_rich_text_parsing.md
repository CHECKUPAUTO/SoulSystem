# Shared Rich-Text Parsing Utility Proposal

**Created:** 2026-04-13 (Night Cycle 00:34)
**Source:** OpenEvolve Night Cycle Report 2026-04-13 00:34
**Status:** Proposal
**Priority:** P2

## Context

The Feishu integration (`ebb72baba3`, #63785) introduced markdown→rich-text conversion for document comment sessions. Other channels (MS Teams, Slack, Discord) may need similar conversion. Currently this logic is embedded in the Feishu plugin.

## Proposal: Extract to Shared Module

```
src/formatting/
├── markdown-to-rich.ts      # Core converter
├── channel-adapters/
│   ├── feishu.ts            # Feishu-specific rendering
│   ├── msteams.ts           # MS Teams AdaptiveCard mapping
│   └── discord.ts           # Discord embed formatting
└── types.ts                 # RichTextNode, RichTextBlock types
```

### Interface

```typescript
interface RichTextConverter {
  toChannelBlocks(markdown: string, channel: ChannelType): ChannelBlock[];
  toPlainText(markdown: string): string;
  stripFormatting(text: string): string;
}
```

### Benefits

- Reuse across channels (Feishu, MS Teams, Slack, Discord)
- Single place to fix markdown parsing bugs
- Type-safe channel-specific rendering
- Testable in isolation (pure functions)

## References

- Feishu rich parsing commit: `ebb72baba3` (#63785)
- MS Teams media handling: `4fc5016f8f`, `783891f02`