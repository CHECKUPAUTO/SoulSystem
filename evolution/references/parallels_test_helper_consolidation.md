# Parallels Test Helper Consolidation

**Source:** OpenEvolve Night Cycle Report 2026-04-12 03:15  
**Priority:** P3  
**Related Commits:** e08f4c1, 247705b, 270a399, e204649, 0e3f965, 7740c4d, a60ff003, 66be8cdc

---

## Background

Peter Steinberger authored 6 commits (45% of recent activity) hardening Parallels VM test infrastructure for Windows and macOS. These commits show repeated patterns that can be consolidated into reusable test helpers.

---

## Extracted Patterns

### Pattern 1: Gateway Lifecycle Management

**Seen in:** e08f4c1, 270a399, a60ff003, 66be8cdc

```typescript
// test/helpers/parallels.ts
export async function stopGateway(): Promise<void> {
  logger.info('Stopping OpenClaw gateway...');
  
  try {
    await execAsync('openclaw gateway stop');
    await waitForPortRelease(8080, { timeout: 10000 });
  } catch (error) {
    if (error.message.includes('not running')) {
      logger.debug('Gateway was not running');
    } else {
      throw error;
    }
  }
}

export async function startGateway(options?: GatewayOptions): Promise<void> {
  logger.info('Starting OpenClaw gateway...');
  
  const args = [
    options?.config && `--config=${options.config}`,
    options?.port && `--port=${options.port}`,
  ].filter(Boolean);

  await execAsync(`openclaw gateway start ${args.join(' ')}`);
  await waitForPort(8080, { timeout: 30000 });
  
  // Additional wait for full initialization
  await sleep(1000);
}

export async function restartGateway(options?: GatewayOptions): Promise<void> {
  await stopGateway();
  await startGateway(options);
}

export async function withStoppedGateway<T>(
  fn: () => Promise<T>
): Promise<T> {
  await stopGateway();
  try {
    return await fn();
  } finally {
    await startGateway();
  }
}
```

### Pattern 2: Platform-Specific Test Setup

**Seen in:** e08f4c1 (Windows), 247705b (macOS)

```typescript
// test/helpers/parallels.ts
export interface PlatformTestContext {
  platform: 'win32' | 'darwin' | 'linux';
  isWindows: boolean;
  isMacOS: boolean;
  parallelsVmName?: string;
}

export function getPlatformContext(): PlatformTestContext {
  const platform = process.platform;
  const isWindows = platform === 'win32';
  const isMacOS = platform === 'darwin';
  
  return {
    platform,
    isWindows,
    isMacOS,
    parallelsVmName: process.env.PARALLELS_VM_NAME,
  };
}

export function skipUnlessPlatform(
  ...platforms: Array<'win32' | 'darwin' | 'linux'>
): void {
  const { platform } = getPlatformContext();
  if (!platforms.includes(platform)) {
    test.skip(`Skipping: requires ${platforms.join(' or ')}`);
  }
}

export function describePlatform(
  platform: 'win32' | 'darwin' | 'linux',
  fn: () => void
): void {
  describe(`[${platform}]`, () => {
    beforeAll(() => {
      if (process.platform !== platform) {
        // Mark all tests in this describe as skipped
      }
    });
    fn();
  });
}
```

### Pattern 3: Bounded HTTP Requests

**Seen in:** 247705b

```typescript
// test/helpers/parallels.ts
export interface BoundedCurlOptions {
  timeout?: number;
  retries?: number;
  retryDelay?: number;
  expectedStatus?: number;
}

export async function boundedCurl(
  url: string,
  options: BoundedCurlOptions = {}
): Promise<Response> {
  const {
    timeout = 5000,
    retries = 3,
    retryDelay = 1000,
    expectedStatus = 200,
  } = options;

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), timeout);

  try {
    const response = await fetch(url, {
      signal: controller.signal,
    });

    if (response.status !== expectedStatus) {
      throw new Error(`Unexpected status: ${response.status}`);
    }

    return response;
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function boundedCurlWithRetry(
  url: string,
  options: BoundedCurlOptions = {}
): Promise<Response> {
  const { retries = 3, retryDelay = 1000 } = options;
  let lastError: Error | undefined;

  for (let i = 0; i < retries; i++) {
    try {
      return await boundedCurl(url, options);
    } catch (error) {
      lastError = error as Error;
      if (i < retries - 1) {
        await sleep(retryDelay * (i + 1)); // Exponential backoff
      }
    }
  }

  throw lastError;
}
```

### Pattern 4: NPM Update with Gateway Management

**Seen in:** ada95aef, 66be8cdc

```typescript
// test/helpers/parallels.ts
export async function updateNpmWithGatewayStop(
  packageName: string
): Promise<void> {
  await withStoppedGateway(async () => {
    logger.info(`Updating ${packageName}...`);
    await execAsync(`npm update ${packageName}`);
    
    // Verify update
    const version = await execAsync(`npm list ${packageName} --depth=0`);
    logger.info(`Updated to: ${version}`);
  });
}

export async function installNpmWithGatewayStop(
  packageName: string,
  options?: { dev?: boolean; global?: boolean }
): Promise<void> {
  const args = [
    options?.dev && '--save-dev',
    options?.global && '--global',
  ].filter(Boolean);

  await withStoppedGateway(async () => {
    logger.info(`Installing ${packageName}...`);
    await execAsync(`npm install ${packageName} ${args.join(' ')}`);
  });
}
```

### Pattern 5: Browser Substitution Avoidance

**Seen in:** e204649

```typescript
// test/helpers/parallels.ts
export function avoidHostSafariSubstitution(): void {
  // Ensure test uses VM browser, not host
  process.env.PUPPETEER_EXECUTABLE_PATH = process.env.PARALLELS_VM_BROWSER_PATH;
  
  // Disable any host browser auto-detection
  process.env.PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = '1';
  process.env.PLAYWRIGHT_BROWSERS_PATH = process.env.PARALLELS_VM_BROWSERS_PATH;
}

export function verifyBrowserIsolation(): void {
  const browserPath = process.env.PUPPETEER_EXECUTABLE_PATH || '';
  const hostPaths = [
    '/Applications/Safari.app',
    '/Applications/Google Chrome.app',
    '/Applications/Firefox.app',
  ];

  for (const hostPath of hostPaths) {
    if (browserPath.includes(hostPath)) {
      throw new Error(`Test may use host browser: ${browserPath}`);
    }
  }

  logger.debug('Browser isolation verified');
}
```

---

## Consolidated Test Helper

```typescript
// test/helpers/parallels.ts
import { exec } from 'child_process';
import { promisify } from 'util';
import { logger } from '../../src/logging';

const execAsync = promisify(exec);

// Re-export all patterns
export * from './parallels-gateway';
export * from './parallels-platform';
export * from './parallels-http';
export * from './parallels-npm';
export * from './parallels-browser';

// Utility functions
export function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

export async function waitForPort(
  port: number,
  options: { timeout?: number; interval?: number } = {}
): Promise<void> {
  const { timeout = 30000, interval = 100 } = options;
  const startTime = Date.now();

  while (Date.now() - startTime < timeout) {
    try {
      const result = await execAsync(`lsof -i:${port} -sTCP:LISTEN`);
      if (result.stdout) return;
    } catch {
      // Port not ready yet
    }
    await sleep(interval);
  }

  throw new Error(`Port ${port} did not become available within ${timeout}ms`);
}

export async function waitForPortRelease(
  port: number,
  options: { timeout?: number; interval?: number } = {}
): Promise<void> {
  const { timeout = 10000, interval = 100 } = options;
  const startTime = Date.now();

  while (Date.now() - startTime < timeout) {
    try {
      await execAsync(`lsof -i:${port}`);
    } catch {
      // Port released
      return;
    }
    await sleep(interval);
  }

  throw new Error(`Port ${port} did not release within ${timeout}ms`);
}
```

---

## Usage Example

```typescript
// test/parallels/windows-gateway.test.ts
import {
  restartGateway,
  skipUnlessPlatform,
  boundedCurlWithRetry,
  updateNpmWithGatewayStop,
} from '../helpers/parallels';

describe('Windows Gateway Tests', () => {
  skipUnlessPlatform('win32');

  beforeEach(async () => {
    await restartGateway();
  });

  it('should handle npm updates with gateway restart', async () => {
    await updateNpmWithGatewayStop('openclaw');
    
    // Verify gateway is back up
    const response = await boundedCurlWithRetry('http://localhost:8080/health', {
      retries: 5,
    });
    
    expect(response.status).toBe(200);
  });
});

// test/parallels/macos-dashboard.test.ts
import {
  skipUnlessPlatform,
  boundedCurl,
} from '../helpers/parallels';

describe('macOS Dashboard Tests', () => {
  skipUnlessPlatform('darwin');

  it('should bound dashboard curl requests', async () => {
    const response = await boundedCurl('http://localhost:8080/dashboard', {
      timeout: 5000,
    });
    
    expect(response.status).toBe(200);
  });
});
```

---

## Migration Path

1. **Create helper modules** from patterns above
2. **Refactor existing tests** to use helpers:
   - `test/parallels/windows-gateway.test.ts`
   - `test/parallels/macos-dashboard.test.ts`
   - `test/parallels/update-npm.test.ts`
3. **Remove duplication** in existing test files
4. **Add documentation** for Parallels test best practices

---

## References

- Source Report: `night_cycle_20260412_0315.md`
- Related Commits: e08f4c1, 247705b, 270a399, e204649, 0e3f965, 7740c4d, a60ff003, 66be8cdc
- Related Pattern: `parallel_test_seams_pattern.md`
