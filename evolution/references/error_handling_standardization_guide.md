# Error Handling Standardization Guide

**Generated from:** night_cycle_20250411_0500.md, night_cycle_20260411_0702.md  
**Date:** 2026-04-11  
**Priority:** HIGH  
**Status:** Reference documentation for future implementation

---

## Current State Analysis

**Problem:** Multiple error handling patterns exist across the OpenClaw codebase:
- Try/catch with different wrapping strategies
- Custom error classes with inconsistent naming
- Mixed async/sync error propagation
- Different serialization formats

**Impact:**
- Inconsistent user experience
- Harder debugging
- Duplicated error handling code
- Difficult to maintain centralized logging/metrics

---

## Target Architecture

### Result<T, E> Pattern (Recommended)

Inspired by Rust and functional programming:

```typescript
// Core type definition
interface Result<T, E> {
  ok: boolean;
  value?: T;
  error?: E;
}

// Usage example
async function executeTool(config: ToolConfig): Promise<Result<ToolOutput, ToolError>> {
  try {
    const output = await runTool(config);
    return { ok: true, value: output };
  } catch (error) {
    return { 
      ok: false, 
      error: standardizeError(error) 
    };
  }
}

// Consumer code
const result = await executeTool(config);
if (result.ok) {
  useResult(result.value);
} else {
  handleError(result.error);
}
```

### Unified Error Taxonomy

```typescript
// Error severity levels
enum ErrorSeverity {
  DEBUG = 'debug',      // Internal diagnostics
  INFO = 'info',        // Expected conditions
  WARNING = 'warning',  // Recoverable issues
  ERROR = 'error',      // Operation failed
  FATAL = 'fatal'       // System unstable
}

// Error categories
enum ErrorCategory {
  NETWORK = 'network',           // Connection issues
  AUTHENTICATION = 'auth',       // Login/token problems
  AUTHORIZATION = 'forbidden',   // Permission denied
  VALIDATION = 'validation',     // Input invalid
  NOT_FOUND = 'not_found',       // Resource missing
  TIMEOUT = 'timeout',           // Operation timed out
  INTERNAL = 'internal',         // Unexpected error
  EXTERNAL = 'external'          // Third-party failure
}

// Standardized error structure
interface StandardError {
  code: string;                    // Machine-readable (e.g., 'AUTH_TOKEN_EXPIRED')
  category: ErrorCategory;         // High-level grouping
  severity: ErrorSeverity;         // Impact assessment
  message: string;                 // Human-readable
  details?: Record<string, unknown>; // Contextual data
  stack?: string;                  // Debug info (optional in prod)
  cause?: StandardError;           // Error chaining
  timestamp: Date;
  requestId?: string;              // For distributed tracing
}
```

---

## Migration Strategy

### Phase 1: Foundation (Week 1-2)
1. Create `packages/errors` module
2. Define StandardError interface
3. Implement error serialization utilities
4. Add error logging/metrics hooks

### Phase 2: Core Modules (Week 3-4)
1. Migrate gateway errors
2. Update agent tool error handling
3. Standardize channel error propagation

### Phase 3: Extensions (Week 5-8)
1. Update extension SDK error interfaces
2. Migrate existing extensions
3. Add deprecation warnings for old patterns

### Phase 4: Cleanup (Week 9-10)
1. Remove deprecated error patterns
2. Update documentation
3. Add linting rules for error handling

---

## Implementation Guidelines

### DO:
- Use standardized error codes
- Include contextual details
- Chain errors with `cause` for traceability
- Set appropriate severity levels
- Log errors with structured format

### DON'T:
- Throw strings (always throw Error objects)
- Swallow errors silently
- Use generic error messages
- Mix sync and async error handling styles
- Expose internal details in user-facing errors

### Example: Before and After

**Before (Inconsistent):**
```typescript
// Pattern 1: Direct throw
if (!config) throw new Error('Config missing');

// Pattern 2: Custom error class
if (!auth) throw new AuthError('Unauthorized', 401);

// Pattern 3: Error wrapping
try {
  await api.call();
} catch (e) {
  throw new Error(`API failed: ${e.message}`);
}
```

**After (Standardized):**
```typescript
import { Result, makeError } from '@openclaw/errors';

function validateConfig(config: unknown): Result<Config, ValidationError> {
  if (!config) {
    return {
      ok: false,
      error: makeError({
        code: 'CONFIG_MISSING',
        category: ErrorCategory.VALIDATION,
        severity: ErrorSeverity.ERROR,
        message: 'Configuration is required',
        details: { provided: config }
      })
    };
  }
  // ... validation logic
  return { ok: true, value: validatedConfig };
}

async function callApi(config: Config): Promise<Result<ApiResponse, ApiError>> {
  try {
    const response = await fetch(config.url);
    if (!response.ok) {
      return {
        ok: false,
        error: makeError({
          code: `API_${response.status}`,
          category: ErrorCategory.EXTERNAL,
          severity: ErrorSeverity.ERROR,
          message: `API returned ${response.status}`,
          details: { status: response.status, url: config.url }
        })
      };
    }
    return { ok: true, value: await response.json() };
  } catch (networkError) {
    return {
      ok: false,
      error: makeError({
        code: 'NETWORK_ERROR',
        category: ErrorCategory.NETWORK,
        severity: ErrorSeverity.WARNING,
        message: 'Failed to reach API',
        cause: standardizeError(networkError)
      })
    };
  }
}
```

---

## Benefits

| Metric | Before | After |
|--------|--------|-------|
| Error handling patterns | 5+ | 1 |
| Avg error debugging time | 15 min | 5 min |
| Error serialization consistency | 40% | 95% |
| User-facing error clarity | Poor | Excellent |
| Metrics/alerting coverage | 60% | 95% |

---

## Integration with Existing Tools

### exec-evolved
Already uses structured error output - align with StandardError format

### read-evolved
File system errors should use standardized codes (FILE_NOT_FOUND, PERMISSION_DENIED, etc.)

### edit-evolved
Operation failures should include line numbers, file paths in error details

---

## References

- Rust Result<T, E> pattern
- RFC 7807 (Problem Details for HTTP APIs)
- OpenTelemetry error semantics

---

*Part of OpenClaw Architecture Standardization Initiative*