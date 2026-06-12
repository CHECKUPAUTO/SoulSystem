# Codex App-Server Integration Guide

**Source:** OpenEvolve Night Cycle Analysis (2026-04-11)  
**Scope:** OpenClaw Codex Extension Architecture

---

## Overview

This document describes the Codex app-server integration pattern added to OpenClaw, enabling programmatic control of Codex instances via the OpenClaw gateway.

## Architecture

### Component Overview

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   OpenClaw      │────▶│  Codex Extension │────▶│  Codex App      │
│   Gateway       │◀────│  (app-server)    │◀────│  Server         │
└─────────────────┘     └──────────────────┘     └─────────────────┘
         │                       │                       │
         ▼                       ▼                       ▼
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Commands.ts    │     │  Client.ts       │     │  Config.ts      │
│  (slash cmds)   │     │  (HTTP client)   │     │  (server mgmt)  │
└─────────────────┘     └──────────────────┘     └─────────────────┘
```

### File Structure

```
extensions/codex/
├── src/
│   ├── commands.ts          # Slash command handlers
│   ├── app-server/
│   │   ├── client.ts        # HTTP client for app-server
│   │   └── config.ts        # Server configuration (195 lines)
│   └── ...
├── docs/
│   └── plugins/
│       └── codex-harness.md # Documentation (103 lines)
└── openclaw.plugin.json     # Plugin manifest (114 lines)
```

---

## Configuration Schema

### Server Configuration (config.ts)

```typescript
interface CodexServerConfig {
  // Server lifecycle
  autoStart: boolean;           // Start server on OpenClaw boot
  restartPolicy: 'always' | 'on-failure' | 'never';
  
  // Connection settings
  host: string;                 // Default: 127.0.0.1
  port: number;                 // Default: 8080
  
  // Security
  authToken?: string;           // Bearer token for API
  allowedOrigins: string[];    // CORS origins
  
  // Execution
  workspacePath: string;       // Codex workspace directory
  maxConcurrentSessions: number; // Default: 10
  
  // Logging
  logLevel: 'debug' | 'info' | 'warn' | 'error';
  logPath?: string;
}
```

### Plugin Manifest

```json
{
  "id": "codex",
  "name": "Codex Integration",
  "version": "1.0.0",
  "exports": {
    "commands": "./dist/commands.js",
    "client": "./dist/app-server/client.js"
  },
  "config": {
    "schema": "./config.schema.json"
  }
}
```

---

## Commands

### Available Slash Commands

| Command | Description | Parameters |
|---------|-------------|------------|
| `/codex start` | Start the Codex app-server | `--port`, `--workspace` |
| `/codex stop` | Stop the app-server gracefully | `--timeout` |
| `/codex restart` | Restart with new configuration | `--config` |
| `/codex status` | Check server health and sessions | - |
| `/codex exec` | Execute a command via Codex | `<command>`, `--session` |

### Command Example

```typescript
// commands.ts
export const codexCommands = {
  start: async (options: StartOptions): Promise<Result<void>> => {
    const config = await loadConfig(options.configPath);
    const client = new CodexClient(config);
    
    const result = await client.startServer({
      port: options.port ?? config.port,
      workspace: options.workspace ?? config.workspacePath,
    });
    
    if (result.isErr()) {
      return Err(new CodexError(`Failed to start: ${result.error}`));
    }
    
    // Store server PID for lifecycle management
    await saveServerState(result.value);
    return Ok(void 0);
  },
  
  // ... other commands
};
```

---

## Client API

### CodexClient

```typescript
class CodexClient {
  private baseUrl: string;
  private authToken?: string;
  
  constructor(config: CodexServerConfig) {
    this.baseUrl = `http://${config.host}:${config.port}`;
    this.authToken = config.authToken;
  }
  
  // Server lifecycle
  async startServer(options: StartOptions): Promise<Result<ServerState>>;
  async stopServer(timeout?: number): Promise<Result<void>>;
  async getStatus(): Promise<Result<ServerStatus>>;
  
  // Session management
  async createSession(context?: string): Promise<Result<Session>>;
  async listSessions(): Promise<Result<Session[]>>;
  async closeSession(sessionId: string): Promise<Result<void>>;
  
  // Execution
  async executeCommand(
    sessionId: string,
    command: string,
    options?: ExecOptions
  ): Promise<Result<ExecResult>>;
}
```

---

## Integration Patterns

### With OpenClaw Agent Runtime

```typescript
// Agent execution via Codex
export async function runWithCodex(
  agentRequest: AgentRequest,
  codexConfig: CodexServerConfig
): Promise<AgentResponse> {
  const client = new CodexClient(codexConfig);
  
  // Ensure server is running
  const status = await client.getStatus();
  if (status.isErr() || !status.value.running) {
    await client.startServer({});
  }
  
  // Create isolated session
  const session = await client.createSession(agentRequest.context);
  if (session.isErr()) {
    throw new Error(`Failed to create session: ${session.error}`);
  }
  
  // Execute agent
  const result = await client.executeCommand(
    session.value.id,
    agentRequest.command,
    {
      timeout: agentRequest.timeout,
      workspace: agentRequest.workspace,
    }
  );
  
  // Cleanup
  await client.closeSession(session.value.id);
  
  return {
    output: result.value?.stdout ?? '',
    exitCode: result.value?.exitCode ?? 1,
  };
}
```

---

## Monitoring & Observability

### Health Check Endpoint

```typescript
// Server health
GET /health
{
  "status": "healthy",
  "version": "1.0.0",
  "uptime": 3600,
  "sessions": {
    "active": 5,
    "max": 10
  }
}
```

### Metrics to Track

| Metric | Type | Alert Threshold |
|--------|------|-----------------|
| Server uptime | Counter | < 99.9% over 24h |
| Session count | Gauge | > 80% of max |
| Command latency | Histogram | p99 > 5s |
| Error rate | Counter | > 1% of total |

---

## Security Considerations

### Authentication

1. **Bearer Token**: Optional but recommended for production
2. **Origin Validation**: CORS whitelist for browser clients
3. **Session Isolation**: Each Codex session runs in isolated context

### Execution Restrictions

```typescript
// Strict agentic execution
interface StrictAgenticConfig {
  // Block dangerous commands
  blockedCommands: ['rm -rf /', 'mkfs', 'dd if=/dev/zero'];
  
  // Require approval for
  requireApproval: [
    'git push',
    'docker run',
    'npm publish',
  ];
  
  // Working directory constraints
  allowedPaths: ['/workspace/*', '/tmp/*'];
}
```

---

## Error Handling

### Error Types

```typescript
type CodexError =
  | { type: 'SERVER_NOT_RUNNING'; message: string }
  | { type: 'SESSION_EXPIRED'; sessionId: string }
  | { type: 'COMMAND_TIMEOUT'; command: string; timeout: number }
  | { type: 'EXECUTION_FAILED'; exitCode: number; stderr: string }
  | { type: 'CONFIG_INVALID'; path: string; reason: string };
```

### Retry Policy

- Connection errors: 3 retries with exponential backoff
- Timeout errors: No retry, propagate to caller
- Server errors (5xx): 2 retries with jitter
- Client errors (4xx): No retry, fail fast

---

## Deployment Recommendations

### Development

```bash
# Local development
openclaw codex start --workspace ./workspace --port 8080
```

### Production

1. Use systemd/supervisor for process management
2. Configure log rotation
3. Set up health check monitoring
4. Enable authentication
5. Use reverse proxy (nginx/caddy) for TLS

---

## References

- Commit 31a0b7bd: Codex app-server controls
- Commit dd26e8c4: Strict agentic execution
- Plugin SDK Documentation
- Codex API Reference

---

*Generated by OpenEvolve Auto-Apply*  
*Timestamp: 2026-04-11T04:27:00Z*
