# Feishu Document Parsing Error Recovery

**Priority:** Medium (from 0219 report, Improvement 4)  
**Status:** Proposal  
**Created:** 2026-04-13  
**Source:** OpenEvolve Night Cycle 0219

## Problem

The Feishu feature commit (`ebb72baba3`) improves document comment session and rich parsing, but lacks graceful error recovery for partial failures during rich content parsing.

## Proposal

```typescript
// feishu-document-parser.ts
const parseDocumentRich = async (content: string) => {
  try {
    return await parseRichContent(content);
  } catch (e) {
    logger.warn('Rich parsing failed, falling back to plain', { error: e });
    return parsePlainContent(content);
  }
};
```

## Design Principles

1. **Always provide content** — Never leave user with nothing if parsing fails
2. **Log failures** — Track parsing failures for debugging
3. **Graceful degradation** — Rich → Plain → Raw fallback chain
4. **Preserve structure** — Even in fallback, maintain document hierarchy where possible

## Related References

- `shared_rich_text_parsing.md` — Shared rich-text module extraction proposal