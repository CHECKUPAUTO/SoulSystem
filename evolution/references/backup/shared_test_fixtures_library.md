# Shared Test Fixtures Library

**Source:** OpenEvolve Night Cycle Report 2026-04-11 (2115)
**Purpose:** Centralize channel and agent mocks to reduce duplication across tests

## Problem Statement

Multiple test files across `outbound/` and `auto-reply/` modules duplicate mock setups:

```typescript
// Before: Duplicated in test files
const mockChannel: Channel = {
  id: 'test-channel',
  type: 'telegram',
  supportsReactions: true,
  // ... 20+ lines repeated
};
```

This creates maintenance burden and test fragility.

## Solution

Create `test/fixtures/` with reusable, type-safe fixtures.

## Directory Structure

```
test/
├── fixtures/
│   ├── channel-mocks.ts      # Channel mock factories
│   ├── agent-mocks.ts        # Agent mock factories
│   ├── message-mocks.ts      # Message mock factories
│   ├── user-mocks.ts         # User mock factories
│   ├── index.ts              # Re-export all fixtures
│   └── README.md             # Usage documentation
├── helpers/
│   ├── setup.ts              # Test setup utilities
│   └── teardown.ts           # Test teardown utilities
└── utils/
    └── typing-controller.ts  # Extracted from reply tests
```

## Implementation

### Channel Mocks

```typescript
// test/fixtures/channel-mocks.ts
import type { Channel, ChannelCapabilities } from '../../src/channels/types';

export interface ChannelMockOptions {
  id?: string;
  type?: 'telegram' | 'discord' | 'whatsapp' | 'web';
  supportsReactions?: boolean;
  supportsThreads?: boolean;
  supportsPolls?: boolean;
  rateLimitPerUser?: number;
}

export function createChannelMock(options: ChannelMockOptions = {}): Channel {
  const defaults: Channel = {
    id: options.id ?? 'test-channel-001',
    type: options.type ?? 'telegram',
    name: `Test ${options.type ?? 'Telegram'} Channel`,
    capabilities: {
      supportsReactions: options.supportsReactions ?? true,
      supportsThreads: options.supportsThreads ?? true,
      supportsPolls: options.supportsPolls ?? false,
      supportsTyping: true,
      supportsEditing: true,
      supportsDeletion: true,
    },
    rateLimitPerUser: options.rateLimitPerUser ?? 0,
  };

  return defaults;
}

export const CHANNEL_FIXTURES = {
  telegram: createChannelMock({ type: 'telegram', supportsReactions: true }),
  discord: createChannelMock({ type: 'discord', supportsThreads: true }),
  whatsapp: createChannelMock({ type: 'whatsapp', supportsReactions: false }),
  web: createChannelMock({ type: 'web', supportsReactions: false }),
} as const;

// Common channel collections
export const TEXT_CHANNELS = [
  CHANNEL_FIXTURES.telegram,
  CHANNEL_FIXTURES.discord,
];

export const NO_REACTION_CHANNELS = [
  CHANNEL_FIXTURES.whatsapp,
  CHANNEL_FIXTURES.web,
];
```

### Agent Mocks

```typescript
// test/fixtures/agent-mocks.ts
import type { Agent, AgentCapabilities } from '../../src/agents/types';

export interface AgentMockOptions {
  id?: string;
  name?: string;
  model?: string;
  supportsTools?: boolean;
  supportsVision?: boolean;
  maxTokens?: number;
}

export function createAgentMock(options: AgentMockOptions = {}): Agent {
  return {
    id: options.id ?? 'agent-test-001',
    name: options.name ?? 'Test Agent',
    model: options.model ?? 'gpt-4',
    capabilities: {
      supportsTools: options.supportsTools ?? true,
      supportsVision: options.supportsVision ?? false,
      supportsStreaming: true,
    },
    config: {
      maxTokens: options.maxTokens ?? 4096,
      temperature: 0.7,
    },
  };
}

export const AGENT_FIXTURES = {
  default: createAgentMock(),
  vision: createAgentMock({ name: 'Vision Agent', supportsVision: true }),
  code: createAgentMock({ name: 'Code Agent', model: 'claude-3-opus' }),
  fast: createAgentMock({ name: 'Fast Agent', model: 'gpt-3.5-turbo' }),
} as const;
```

### Message Mocks

```typescript
// test/fixtures/message-mocks.ts
import type { Message, MessageContent } from '../../src/messages/types';

export interface MessageMockOptions {
  id?: string;
  content?: string;
  authorId?: string;
  channelId?: string;
  timestamp?: Date;
}

export function createMessageMock(options: MessageMockOptions = {}): Message {
  return {
    id: options.id ?? 'msg-test-001',
    content: {
      type: 'text',
      text: options.content ?? 'Hello, test message!',
    } as MessageContent,
    authorId: options.authorId ?? 'user-test-001',
    channelId: options.channelId ?? 'channel-test-001',
    timestamp: options.timestamp ?? new Date('2026-04-11T00:00:00Z'),
    edited: false,
    attachments: [],
  };
}

export const MESSAGE_FIXTURES = {
  text: createMessageMock({ content: 'Simple text message' }),
  command: createMessageMock({ content: '/help' }),
  mention: createMessageMock({ content: '@clawd hello!' }),
  long: createMessageMock({ content: 'a'.repeat(2000) }),
} as const;
```

### Typing Controller Helper

```typescript
// test/fixtures/typing-controller.ts
// Extracted from get-reply-run.*.test.ts files

export interface TypingControllerMock {
  start: jest.Mock<Promise<void>, []>;
  stop: jest.Mock<Promise<void>, []>;
  isActive: jest.Mock<boolean, []>;
}

export function createTypingControllerMock(): TypingControllerMock {
  return {
    start: jest.fn().mockResolvedValue(undefined),
    stop: jest.fn().mockResolvedValue(undefined),
    isActive: jest.fn().mockReturnValue(false),
  };
}

export function simulateTypingDelay(
  controller: TypingControllerMock,
  duration: number = 100
): Promise<void> {
  controller.isActive.mockReturnValue(true);
  return new Promise(resolve => {
    setTimeout(() => {
      controller.isActive.mockReturnValue(false);
      resolve();
    }, duration);
  });
}
```

## Usage Examples

### In Tests

```typescript
// Before: Duplicated setup
import { something } from './module';

describe('My Test', () => {
  const mockChannel = { /* 20 lines */ };
  const mockAgent = { /* 15 lines */ };

  it('works', () => {
    // test
  });
});

// After: Using fixtures
import { createChannelMock, CHANNEL_FIXTURES } from '../../../test/fixtures';
import { createAgentMock } from '../../../test/fixtures';

describe('My Test', () => {
  const mockChannel = CHANNEL_FIXTURES.telegram;
  const mockAgent = createAgentMock({ name: 'Custom Agent' });

  it('works', () => {
    // test
  });
});
```

### Vitest Setup

```typescript
// test/setup.ts
import { beforeAll } from 'vitest';
import { resetFixtures } from './fixtures';

beforeAll(() => {
  // Reset any mutable fixture state
  resetFixtures();
});
```

## Migration Path

1. Create `test/fixtures/` directory
2. Implement core factories (channel, agent, message)
3. Update one test file to use fixtures
4. Run tests to verify
5. Repeat for remaining files
6. Delete duplicate mock code

## Checklist

- [ ] Create `test/fixtures/channel-mocks.ts`
- [ ] Create `test/fixtures/agent-mocks.ts`
- [ ] Create `test/fixtures/message-mocks.ts`
- [ ] Create `test/fixtures/index.ts` with exports
- [ ] Extract `typing-controller.ts` from reply tests
- [ ] Update `vitest.config.ts` with setup file
- [ ] Migrate tests in `outbound/` module
- [ ] Migrate tests in `auto-reply/` module
- [ ] Remove duplicate mock code
- [ ] Document usage in README

## Benefits

1. **DRY Principle:** No more duplicated mock code
2. **Consistency:** Standard fixtures across all tests
3. **Maintainability:** Change one place, update everywhere
4. **Discoverability:** Central location for test helpers

## References

- Night Cycle Report: night_cycle_20260411_2115.md
- Related: pure_test_migration_pattern.md
