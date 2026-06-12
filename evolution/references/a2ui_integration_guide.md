# A2UI (Agent-to-UI) Integration Guide

**Source:** OpenEvolve Night Cycle Report 2026-04-12  
**Purpose:** Enable VisionClaw to push visual confirmations back to the user's phone/glasses

## Overview

The Agent-to-UI (A2UI) layer allows agents to communicate directly with the user's interface layer, enabling rich visual feedback without requiring the user to explicitly query for status updates.

## Use Cases

1. **Visual Confirmations:** Show action previews before execution
2. **Status Updates:** Push progress indicators for long-running tasks
3. **Rich Results:** Display formatted data (lists, tables, images)
4. **Interactive Elements:** Buttons for quick actions ("Yes/No", "Cancel")

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Agent     │────▶│   A2UI      │────▶│  UI Layer   │
│  (Gateway)  │     │   Gateway   │     │(Phone/Glass)│
└─────────────┘     └─────────────┘     └─────────────┘
```

## Message Types

### 1. Confirmation Push

```typescript
interface ConfirmationPush {
  type: 'confirmation';
  id: string;
  title: string;
  description?: string;
  action: string;
  preview?: {
    type: 'text' | 'image' | 'list';
    content: unknown;
  };
  buttons?: Array<{
    label: string;
    action: string;
    style: 'primary' | 'secondary' | 'danger';
  }>;
  timeout?: number; // Auto-dismiss in ms
}

// Example: List item confirmation
await a2ui.push({
  type: 'confirmation',
  id: 'list-add-123',
  title: 'Add to Shopping List?',
  description: 'Milk (2% reduced fat)',
  action: 'list.add',
  preview: {
    type: 'text',
    content: '• Milk\n• Eggs\n• Bread'
  },
  buttons: [
    { label: 'Add', action: 'confirm', style: 'primary' },
    { label: 'Cancel', action: 'dismiss', style: 'secondary' }
  ],
  timeout: 5000
});
```

### 2. Status Update

```typescript
interface StatusUpdate {
  type: 'status';
  id: string;
  status: 'pending' | 'in_progress' | 'completed' | 'failed';
  progress?: number; // 0-100
  message?: string;
}

// Example: Long-running task
await a2ui.push({
  type: 'status',
  id: 'video-render-456',
  status: 'in_progress',
  progress: 45,
  message: 'Rendering video...'
});
```

### 3. Rich Result

```typescript
interface RichResult {
  type: 'result';
  id: string;
  format: 'card' | 'list' | 'image' | 'table';
  content: unknown;
}

// Example: Weather result
await a2ui.push({
  type: 'result',
  id: 'weather-query-789',
  format: 'card',
  content: {
    title: 'San Francisco, CA',
    subtitle: 'Partly Cloudy',
    temperature: '72°F',
    high: '75°F',
    low: '62°F',
    icon: 'partly-cloudy'
  }
});
```

## Implementation

### Server-Side (Gateway)

```typescript
// src/gateway/a2ui/push.ts
import { WebSocket } from 'ws';

const clientConnections = new Map<string, WebSocket>();

export async function pushToClient(
  clientId: string,
  message: A2UIMessage
): Promise<void> {
  const ws = clientConnections.get(clientId);
  if (!ws || ws.readyState !== WebSocket.OPEN) {
    throw new Error(`Client ${clientId} not connected`);
  }
  
  ws.send(JSON.stringify({
    ...message,
    timestamp: Date.now(),
    source: 'openclaw-gateway'
  }));
}

export function registerClient(clientId: string, ws: WebSocket): void {
  clientConnections.set(clientId, ws);
}
```

### Client-Side (VisionClaw)

```typescript
// visionclaw-client/src/a2ui/receiver.ts
class A2UIReceiver {
  private ws: WebSocket;
  
  constructor(gatewayUrl: string) {
    this.ws = new WebSocket(`${gatewayUrl}/a2ui`);
    this.ws.onmessage = this.handleMessage.bind(this);
  }
  
  private handleMessage(event: MessageEvent): void {
    const message = JSON.parse(event.data) as A2UIMessage;
    
    switch (message.type) {
      case 'confirmation':
        this.showConfirmation(message);
        break;
      case 'status':
        this.showStatus(message);
        break;
      case 'result':
        this.showResult(message);
        break;
    }
  }
  
  private showConfirmation(msg: ConfirmationPush): void {
    // Display on glasses HUD
    hud.display({
      type: 'dialog',
      title: msg.title,
      content: msg.preview,
      buttons: msg.buttons,
      timeout: msg.timeout
    });
  }
}
```

## Security Considerations

1. **Authentication:** WebSocket connections must be authenticated
2. **Rate Limiting:** Prevent spam/push flooding
3. **Content Sanitization:** Validate all pushed content
4. **Timeout Handling:** Auto-dismiss old messages

```typescript
// Rate limiting
const pushLimits = new Map<string, number>();

function checkPushLimit(clientId: string): boolean {
  const now = Date.now();
  const lastPush = pushLimits.get(clientId) || 0;
  
  if (now - lastPush < 1000) { // 1 second cooldown
    return false;
  }
  
  pushLimits.set(clientId, now);
  return true;
}
```

## Integration with Fast-Path

```typescript
// Combine fast-path with A2UI
const fastPath = matchFastPath(input);

if (fastPath?.confidence > 0.85 && fastPath.confidence < 0.95) {
  // Show quick confirmation
  await a2ui.push({
    type: 'confirmation',
    title: `Confirm ${fastPath.action}?`,
    timeout: 3000
  });
}
```

## References

- Night Cycle Report: night_cycle_20260412_0100.md
- Fast-Path Pattern: visionclaw_fast_path_pattern.md
