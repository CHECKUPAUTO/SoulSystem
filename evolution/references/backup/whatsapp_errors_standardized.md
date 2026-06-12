# WhatsApp Error Response Standardization

## Overview
Centralized error handling patterns for consistent user-facing error messages and retry behavior.

## Error Class Pattern

```typescript
// src/errors/whatsapp-errors.ts
export class WhatsAppConnectionError extends Error {
  constructor(
    public code: string,
    public retryable: boolean,
    public recoveryHint?: string
  ) {
    super();
  }
}

export class WhatsAppTokenError extends WhatsAppConnectionError {
  constructor(message: string) {
    super('TOKEN_ERROR', false, 'Please re-authenticate via QR code');
    this.message = message;
  }
}

export class WhatsAppNetworkError extends WhatsAppConnectionError {
  constructor(message: string) {
    super('NETWORK_ERROR', true, 'Connection will be retried automatically');
    this.message = message;
  }
}
```

## Benefits
- Consistent error handling
- Better retry logic
- Improved user-facing error messages
