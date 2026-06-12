# Feishu Comment System Modularization Guide

## Overview

**Scope:** Commit ebb72baba3 (Feishu document comment system)
**Current Size:** 1,373 lines across 3 core files
**Target:** Modular architecture following CodeWiki patterns
**Priority:** High (P1)

## Current Structure

```
extensions/feishu/src/
├── comment-reaction.ts        (281 lines)
├── comment-shared.ts          (331 lines)
├── comment-dispatcher.ts      (~150 lines, estimated)
└── monitor.comment.ts         (761 lines) ← MONOLITH
```

## Problems with Current Structure

1. **Cognitive Load:** `monitor.comment.ts` at 761 lines is too large
2. **Testability:** Mixed concerns make unit testing difficult
3. **Reusability:** Shared logic buried in monolithic files
4. **Auditability:** Security-sensitive reaction handling needs isolation

## Proposed Modular Structure

```
extensions/feishu/src/
├── comment/
│   ├── index.ts                      # Public API exports
│   ├── types/
│   │   ├── comment.ts                # Core comment types
│   │   ├── reaction.ts               # Reaction types
│   │   └── events.ts                 # Event type definitions
│   ├── handlers/
│   │   ├── index.ts                  # Handler registry
│   │   ├── base-handler.ts           # Abstract base class
│   │   ├── create-handler.ts         # Comment creation
│   │   ├── update-handler.ts         # Comment updates
│   │   ├── delete-handler.ts         # Comment deletion
│   │   └── reaction-handler.ts       # Reaction events
│   ├── parsers/
│   │   ├── content-parser.ts         # Rich content parsing
│   │   ├── mention-parser.ts         # User mention extraction
│   │   └── metadata-parser.ts        # Comment metadata
│   ├── reactions/
│   │   ├── index.ts                  # Reaction exports
│   │   ├── types.ts                  # Reaction type definitions
│   │   ├── validator.ts              # Reaction validation
│   │   └── processor.ts              # Reaction processing
│   └── monitor.ts                    # Refactored (300 lines max)
├── comment-shared.ts                 # Deprecated → migrate
├── comment-reaction.ts               # Deprecated → migrate
└── monitor.comment.ts                # Deprecated → migrate
```

## Migration Steps

### Phase 1: Type Extraction

Create `comment/types/` directory with extracted types:

```typescript
// comment/types/comment.ts
export interface FeishuComment {
  id: string;
  content: RichContent;
  author: UserInfo;
  createdAt: Date;
  // ... extracted from comment-shared.ts
}

export interface CommentCreateEvent {
  type: "comment.created";
  comment: FeishuComment;
  documentId: string;
}
// ... etc
```

### Phase 2: Parser Extraction

Extract parsing logic from `monitor.comment.ts`:

```typescript
// comment/parsers/content-parser.ts
export class ContentParser {
  parseRichContent(raw: unknown): RichContent {
    // Extract from lines ~200-300 of monitor.comment.ts
  }
  
  extractMentions(content: RichContent): Mention[] {
    // Extract from lines ~400-450
  }
}
```

### Phase 3: Handler Extraction

Create handler classes:

```typescript
// comment/handlers/base-handler.ts
export abstract class CommentHandler {
  abstract canHandle(event: CommentEvent): boolean;
  abstract handle(event: CommentEvent): Promise<void>;
  
  protected async validatePermissions(
    userId: string,
    commentId: string
  ): Promise<boolean> {
    // Shared validation logic
  }
}

// comment/handlers/create-handler.ts
export class CreateCommentHandler extends CommentHandler {
  canHandle(event: CommentEvent): boolean {
    return event.type === "comment.created";
  }
  
  async handle(event: CommentCreateEvent): Promise<void> {
    // Extract from lines ~100-250 of monitor.comment.ts
  }
}
```

### Phase 4: Reaction Module

Migrate `comment-reaction.ts` to `comment/reactions/`:

```typescript
// comment/reactions/types.ts
export interface Reaction {
  emoji: string;
  userId: string;
  timestamp: Date;
}

export interface ReactionEvent {
  type: "reaction.added" | "reaction.removed";
  commentId: string;
  reaction: Reaction;
}

// comment/reactions/validator.ts
export class ReactionValidator {
  private readonly ALLOWED_EMOJIS = new Set(["👍", "❤️", "😂", "🎉"]);
  
  validate(reaction: Reaction): ValidationResult {
    // XSS prevention
    if (!this.ALLOWED_EMOJIS.has(reaction.emoji)) {
      return { valid: false, error: "Invalid emoji" };
    }
    
    // Rate limiting check
    // ...
    
    return { valid: true };
  }
}
```

### Phase 5: Monitor Refactoring

Refactor `monitor.comment.ts` to use new modules:

```typescript
// comment/monitor.ts (refactored, ~250 lines)
import { HandlerRegistry } from "./handlers";
import { ContentParser } from "./parsers/content-parser";

export class CommentMonitor {
  private handlers = new HandlerRegistry();
  private parser = new ContentParser();
  
  async onEvent(rawEvent: unknown): Promise<void> {
    const event = this.parser.parseEvent(rawEvent);
    const handler = this.handlers.findHandler(event);
    await handler.handle(event);
  }
}
```

## Security Considerations

### XSS Prevention

Current code handles rich content parsing - must sanitize:

```typescript
// comment/parsers/content-parser.ts
import DOMPurify from "isomorphic-dompurify";

export class ContentParser {
  parseRichContent(raw: unknown): RichContent {
    const content = this.extractContent(raw);
    
    // Sanitize HTML content
    content.html = DOMPurify.sanitize(content.html, {
      ALLOWED_TAGS: ["b", "i", "u", "a", "br"],
      ALLOWED_ATTR: ["href"],
    });
    
    return content;
  }
}
```

### Rate Limiting

Reaction spam protection:

```typescript
// comment/reactions/processor.ts
export class ReactionProcessor {
  private rateLimiter = new Map<string, number>();
  
  async processReaction(
    userId: string,
    reaction: Reaction
  ): Promise<void> {
    // Check rate limit
    const lastReaction = this.rateLimiter.get(userId);
    if (lastReaction && Date.now() - lastReaction < 1000) {
      throw new RateLimitError("Too many reactions");
    }
    
    // Process reaction
    // ...
    
    this.rateLimiter.set(userId, Date.now());
  }
}
```

## Testing Strategy

After modularization, each component has focused tests:

```typescript
// comment/parsers/content-parser.test.ts
import { test, expect } from "vitest";
import { ContentParser } from "./content-parser";

test("sanitizes HTML in rich content", () => {
  const parser = new ContentParser();
  const raw = { html: "<script>alert('xss')</script>Hello" };
  const result = parser.parseRichContent(raw);
  expect(result.html).not.toContain("<script>");
});

// comment/reactions/validator.test.ts
test("rejects unknown emojis", () => {
  const validator = new ReactionValidator();
  const result = validator.validate({ emoji: "🤪" });
  expect(result.valid).toBe(false);
});
```

## Benefits

| Metric | Before | After | Change |
|--------|--------|-------|--------|
| Max file size | 761 lines | ~250 lines | -67% |
| Testability | Low | High | +++ |
| Security audit | Difficult | Focused | +++ |
| Code reuse | None | Modular | +++ |
| T430 Quality | 0.85 | 0.95 | +0.10 |
| T430 Security | 0.80 | 0.90 | +0.10 |

## Implementation Status

**Deferred:** This requires file reorganization and code refactoring across the Feishu extension.

**Recommendation:** Schedule for next sprint with dedicated testing phase.
