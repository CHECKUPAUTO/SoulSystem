# HTML Fallback Narrowing Pattern

**CodeWiki Pattern ID:** `patterns/html-fallback-narrowing`  
**Classification:** Defensive Coding Pattern  
**Status:** Ready for Implementation  
**Source:** OpenEvolve Night Cycle 2026-04-12_0635 (MS Teams fix `2c211d17`)

---

## Problem Statement

Overly broad fallback logic creates unexpected side effects. When handling media attachments or rich content, blanket fallbacks to HTML format can break expected behavior and cause data loss.

**Problem Example (MS Teams):**
```typescript
// BEFORE: Broad fallback causes issues
function processAttachment(attachment: Attachment): string {
  if (!attachment) {
    return htmlFallback;  // Too broad - catches valid cases
  }
  return attachment.content;
}

// Problem: File attachments incorrectly fall back to HTML
// Result: File metadata lost, HTML placeholder shown instead
```

---

## Solution Pattern

### Core Principle: Explicitly Typed, Scope-Limited Fallbacks

Fallbacks should be **narrow**, **typed**, and **scoped** to specific failure modes.

```typescript
// AFTER: Narrow fallback with explicit type checking
function processAttachment(attachment: Attachment): AttachmentResult {
  // Explicit null check with type guard
  if (!attachment) {
    return { type: 'error', reason: 'MISSING_ATTACHMENT' };
  }
  
  // Type-specific handling
  switch (attachment.type) {
    case 'file':
      return processFileAttachment(attachment);
    case 'image':
      return processImageAttachment(attachment);
    case 'unknown':
      // ONLY use HTML fallback for truly unknown types
      return { type: 'html_fallback', content: generateHtmlFallback(attachment) };
    default:
      return { type: 'error', reason: 'UNSUPPORTED_TYPE', type: attachment.type };
  }
}
```

---

## Implementation Strategies

### Strategy 1: Result Type Pattern

```typescript
// Discriminated union for explicit handling
type AttachmentResult =
  | { type: 'file'; url: string; name: string; size: number }
  | { type: 'image'; url: string; width: number; height: number }
  | { type: 'html_fallback'; content: string; originalType: string }
  | { type: 'error'; reason: 'MISSING_ATTACHMENT' | 'UNSUPPORTED_TYPE' | 'PROCESSING_FAILED' };

function processAttachment(att: Attachment | null): AttachmentResult {
  // Explicit null handling
  if (!att) {
    return { type: 'error', reason: 'MISSING_ATTACHMENT' };
  }
  
  // Narrow type checking
  if (att.type === 'unknown' && !att.content) {
    return { 
      type: 'html_fallback', 
      content: generatePlaceholder(att),
      originalType: att.originalType || 'unknown'
    };
  }
  
  // Default: not a fallback case
  return processKnownAttachment(att);
}

// Usage forces explicit handling
const result = processAttachment(attachment);
switch (result.type) {
  case 'file':
    return sendFile(result.url, result.name);
  case 'image':
    return sendImage(result.url, result.width, result.height);
  case 'html_fallback':
    logger.warn(`HTML fallback used for ${result.originalType}`);
    return sendHtml(result.content);
  case 'error':
    logger.error(`Attachment failed: ${result.reason}`);
    return sendErrorNotification(result.reason);
}
```

### Strategy 2: Guard Pattern

```typescript
// Explicit guards for fallback conditions
function needsHtmlFallback(att: Attachment): boolean {
  return att.type === 'unknown' && 
         !att.content && 
         !att.url &&
         att.fallbackStrategy === 'html';
}

function processAttachment(att: Attachment | null): MessageContent {
  // Guard clause for early return
  if (!att) {
    return createErrorMessage('MISSING_ATTACHMENT');
  }
  
  // Narrow guard for fallback
  if (needsHtmlFallback(att)) {
    return createHtmlFallback(att);
  }
  
  // Normal processing path
  return createStandardMessage(att);
}
```

### Strategy 3: Validation Schema

```typescript
import { z } from 'zod';

// Schema defines when fallback is appropriate
const HtmlFallbackSchema = z.object({
  type: z.literal('unknown'),
  content: z.undefined().or(z.null()).or(z.literal('')),
  url: z.undefined().or(z.null()).or(z.literal('')),
  fallbackStrategy: z.literal('html')
});

function processAttachment(att: Attachment): MessageContent {
  // Validate fallback conditions
  const needsFallback = HtmlFallbackSchema.safeParse(att).success;
  
  if (needsFallback) {
    return createHtmlFallback(att);
  }
  
  // Otherwise, standard processing
  return createStandardMessage(att);
}
```

---

## Platform-Specific Examples

### MS Teams File Attachments

```typescript
// BEFORE (problematic):
function handleTeamsAttachment(att: TeamsAttachment) {
  if (!att.contentUrl) {
    return { type: 'html', content: '<div>File unavailable</div>' };
  }
  return { type: 'file', url: att.contentUrl };
}

// AFTER (narrow):
function handleTeamsAttachment(att: TeamsAttachment): TeamsResult {
  // Check SharePoint-specific failure mode
  if (att.contentType === 'application/vnd.microsoft.teams.file' && 
      !att.contentUrl && 
      att.sharePointMetadata?.accessDenied) {
    return { 
      type: 'html_fallback',
      content: generateAccessDeniedHtml(att),
      reason: 'SHAREPOINT_ACCESS_DENIED'
    };
  }
  
  // Check Node 24+ compatibility issue
  if (att.contentType?.includes('vnd.microsoft') && 
      process.version.startsWith('v24') &&
      !att.contentUrl?.includes('sharepoint.com')) {
    return {
      type: 'html_fallback',
      content: generateCompatibilityHtml(att),
      reason: 'NODE_24_COMPATIBILITY'
    };
  }
  
  // Standard file processing
  if (att.contentUrl) {
    return { type: 'file', url: att.contentUrl };
  }
  
  // Unknown case - explicit error
  return { type: 'error', reason: 'UNKNOWN_ATTACHMENT_STATE' };
}
```

### Discord Rich Embeds

```typescript
function processDiscordEmbed(embed: Embed | null): EmbedResult {
  // Explicit null handling
  if (!embed) {
    return { type: 'error', reason: 'MISSING_EMBED' };
  }
  
  // Narrow fallback condition
  if (embed.type === 'rich' && 
      !embed.title && 
      !embed.description && 
      !embed.image?.url) {
    return { 
      type: 'html_fallback',
      content: '<div class="empty-embed"></div>',
      reason: 'EMPTY_RICH_EMBED'
    };
  }
  
  // Standard processing
  return { type: 'embed', data: embed };
}
```

---

## Testing Patterns

### Unit Tests

```typescript
describe('HTML Fallback Narrowing', () => {
  it('should NOT use HTML fallback for valid attachments', () => {
    const fileAtt = {
      type: 'file',
      contentUrl: 'https://example.com/file.pdf',
      name: 'document.pdf'
    };
    
    const result = processAttachment(fileAtt);
    expect(result.type).not.toBe('html_fallback');
    expect(result.type).toBe('file');
  });
  
  it('should use HTML fallback ONLY for unknown type with no content', () => {
    const unknownAtt = {
      type: 'unknown',
      content: null,
      url: null,
      fallbackStrategy: 'html'
    };
    
    const result = processAttachment(unknownAtt);
    expect(result.type).toBe('html_fallback');
  });
  
  it('should return error for missing attachment', () => {
    const result = processAttachment(null);
    expect(result.type).toBe('error');
    expect(result.reason).toBe('MISSING_ATTACHMENT');
  });
  
  it('should log when HTML fallback is used', () => {
    const loggerSpy = jest.spyOn(logger, 'warn');
    
    processAttachment({
      type: 'unknown',
      content: null,
      fallbackStrategy: 'html',
      originalType: 'custom/proprietary'
    });
    
    expect(loggerSpy).toHaveBeenCalledWith(
      expect.stringContaining('custom/proprietary')
    );
  });
});
```

---

## Metrics and Monitoring

Track fallback usage to identify patterns:

```typescript
interface FallbackMetrics {
  totalAttachments: number;
  htmlFallbacks: number;
  errorRate: number;
  fallbackByType: Record<string, number>;
}

// Alert on high fallback rates
if (metrics.htmlFallbacks / metrics.totalAttachments > 0.05) {
  alert('High HTML fallback rate - review attachment handling');
}
```

---

## Related Patterns

- [Security Pipeline Pattern](security_pipeline_pattern.md) - For defensive validation
- [Error Handling Standardization](error_handling_standardization_guide.md) - For consistent error patterns
- [Model Resolution Cascade](model_resolution_cascade.md) - For narrow fallback chains

---

## References

- **Source Commit:** `2c211d17` - MS Teams channel file attachments fix
- **Issue:** SharePoint media fetch fails on Node 24+
- **Impact:** Prevents data loss from overly broad fallbacks

---

*Generated by OpenEvolve Night Cycle Analysis*  
*Report ID: night_cycle_20260412_0635*
