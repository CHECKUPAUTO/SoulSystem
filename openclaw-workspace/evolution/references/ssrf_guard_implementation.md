# SSRF Guard Implementation

**Source:** OpenEvolve Night Cycle Report 2026-04-12 01:15 UTC  
**Priority:** P0 - Critical Security  
**Risk:** Metadata Service Attacks, Internal Network Scanning

---

## Problem Statement

Current OpenClaw tool execution paths lack SSRF (Server-Side Request Forgery) guards. This allows:

1. **Metadata Service Attacks:** Access to `169.254.169.254` (AWS/GCP/Azure IMDS)
2. **Internal Network Scanning:** Probing `10.0.0.0/8`, `192.168.0.0/16`, etc.
3. **Local Service Exploitation:** Access to `localhost`, `127.0.0.1`

**T430 Security Score:** N/A (Not implemented)  
**IronReview Priority:** P0 - Critical

---

## Threat Model

| Attack Vector | Target | Impact |
|--------------|--------|--------|
| AWS IMDS | `169.254.169.254/latest/meta-data/` | Credential theft |
| GCP Metadata | `metadata.google.internal` | Service account keys |
| Azure IMDS | `169.254.169.254/metadata/instance` | Instance enumeration |
| Kubernetes API | `10.0.0.1:443` | Cluster compromise |
| Local Services | `127.0.0.1:*` | Internal service exploitation |

---

## Implementation

### Core SSRF Guard

```typescript
// src/security/ssrf-guard.ts

// Blocked IP ranges and hosts
const FORBIDDEN_HOSTS = [
  // Localhost variants
  'localhost',
  '127.0.0.1',
  '::1',
  '0.0.0.0',
  
  // Link-local
  '169.254.169.254',  // AWS/GCP/Azure metadata
  'fe80::1',
  
  // Cloud metadata
  'metadata.google.internal',
  'metadata.google.internal.',
  'metadata.ec2.internal',
  'instance-data',
];

const FORBIDDEN_IP_RANGES = [
  { start: BigInt('0x0A000000'), end: BigInt('0x0AFFFFFF') },  // 10.0.0.0/8
  { start: BigInt('0xAC100000'), end: BigInt('0xAC1FFFFF') },  // 172.16.0.0/12
  { start: BigInt('0xC0A80000'), end: BigInt('0xC0A8FFFF') },  // 192.168.0.0/16
  { start: BigInt('0x7F000000'), end: BigInt('0x7FFFFFFF') },  // 127.0.0.0/8
  { start: BigInt('0xA9FE0000'), end: BigInt('0xA9FEFFFF') },  // 169.254.0.0/16
];

const FORBIDDEN_PORTS = [
  22,    // SSH
  23,    // Telnet
  25,    // SMTP
  53,    // DNS (exfiltration risk)
  110,   // POP3
  135,   // MS-RPC
  139,   // NetBIOS
  143,   // IMAP
  445,   // SMB
  1433,  // MSSQL
  1521,  // Oracle
  3306,  // MySQL
  3389,  // RDP
  5432,  // PostgreSQL
  6379,  // Redis
  27017, // MongoDB
];

export interface SSRFValidationResult {
  valid: boolean;
  reason?: string;
  blockedHost?: string;
  blockedPort?: number;
}

export class SSRFGuard {
  private allowedSchemes: Set<string>;
  private allowedHosts: Set<string>;
  
  constructor(config?: {
    allowedSchemes?: string[];
    allowedHosts?: string[];
  }) {
    this.allowedSchemes = new Set(config?.allowedSchemes ?? ['http', 'https']);
    this.allowedHosts = new Set(config?.allowedHosts ?? []);
  }
  
  validateUrl(urlString: string): SSRFValidationResult {
    let parsed: URL;
    
    try {
      parsed = new URL(urlString);
    } catch {
      return { valid: false, reason: 'Invalid URL format' };
    }
    
    // Check scheme
    if (!this.allowedSchemes.has(parsed.protocol.slice(0, -1))) {
      return { 
        valid: false, 
        reason: `Scheme '${parsed.protocol}' not allowed` 
      };
    }
    
    // Check hostname
    const hostname = parsed.hostname.toLowerCase();
    
    // Allow explicitly whitelisted hosts
    if (this.allowedHosts.has(hostname)) {
      return { valid: true };
    }
    
    // Check forbidden hosts
    if (FORBIDDEN_HOSTS.some(h => hostname === h || hostname.endsWith(`.${h}`))) {
      return { 
        valid: false, 
        reason: 'Forbidden host',
        blockedHost: hostname
      };
    }
    
    // Check IP ranges
    if (this.isForbiddenIp(hostname)) {
      return { 
        valid: false, 
        reason: 'IP in forbidden range',
        blockedHost: hostname
      };
    }
    
    // Check port
    const port = parseInt(parsed.port) || (parsed.protocol === 'https:' ? 443 : 80);
    if (FORBIDDEN_PORTS.includes(port)) {
      return { 
        valid: false, 
        reason: `Port ${port} is forbidden`,
        blockedPort: port
      };
    }
    
    return { valid: true };
  }
  
  private isForbiddenIp(hostname: string): boolean {
    // Check if hostname is an IP
    const ip = this.parseIp(hostname);
    if (ip === null) return false;
    
    return FORBIDDEN_IP_RANGES.some(
      range => ip >= range.start && ip <= range.end
    );
  }
  
  private parseIp(ip: string): bigint | null {
    // IPv4
    const ipv4Match = ip.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
    if (ipv4Match) {
      const [_, a, b, c, d] = ipv4Match;
      return (BigInt(a) << 24n) | (BigInt(b) << 16n) | 
             (BigInt(c) << 8n) | BigInt(d);
    }
    
    // IPv6 (simplified - would need full implementation)
    if (ip.includes(':')) {
      // For SSRF, any IPv6 localhost/link-local is suspicious
      if (ip === '::1' || ip.startsWith('fe80:')) {
        return BigInt(1);  // Return any non-null to trigger block
      }
    }
    
    return null;
  }
  
  async safeFetch(
    url: string, 
    options?: RequestInit
  ): Promise<Response> {
    const validation = this.validateUrl(url);
    if (!validation.valid) {
      throw new SSRFError(
        `SSRF guard blocked request: ${validation.reason}`,
        validation
      );
    }
    
    // Additional protection: DNS rebind check
    const resolved = await this.safeResolve(url);
    if (!resolved.valid) {
      throw new SSRFError(
        `SSRF guard blocked after DNS resolution: ${resolved.reason}`,
        resolved
      );
    }
    
    return fetch(url, options);
  }
  
  private async safeResolve(url: string): Promise<SSRFValidationResult> {
    const parsed = new URL(url);
    const hostname = parsed.hostname;
    
    // Skip DNS check for already-validated IPs
    if (this.parseIp(hostname) !== null) {
      return { valid: true };
    }
    
    try {
      // Resolve and check each IP
      const { lookup } = await import('dns');
      const addresses = await new Promise<string[]>((resolve, reject) => {
        lookup(hostname, { all: true }, (err, result) => {
          if (err) reject(err);
          else resolve(result.map(r => r.address));
        });
      });
      
      for (const addr of addresses) {
        if (this.isForbiddenIp(addr)) {
          return {
            valid: false,
            reason: `DNS resolution returned forbidden IP: ${addr}`
          };
        }
      }
      
      return { valid: true };
    } catch {
      return { valid: false, reason: 'DNS resolution failed' };
    }
  }
}

export class SSRFError extends Error {
  constructor(
    message: string,
    public validation: SSRFValidationResult
  ) {
    super(message);
    this.name = 'SSRFError';
  }
}
```

### Tool Execution Integration

```typescript
// src/tools/ssrf-wrapper.ts
import { SSRFGuard } from '../security/ssrf-guard';

const ssrfGuard = new SSRFGuard({
  allowedSchemes: ['http', 'https'],
  allowedHosts: [
    // Whitelist safe external APIs
    'api.openweathermap.org',
    'api.github.com',
    // ... etc
  ]
});

export async function safeToolExecution(
  toolCall: ToolCall
): Promise<ToolResult> {
  // Extract URLs from tool parameters
  const urls = extractUrls(toolCall.parameters);
  
  for (const url of urls) {
    const validation = ssrfGuard.validateUrl(url);
    if (!validation.valid) {
      return {
        success: false,
        error: `SSRF guard blocked URL: ${validation.reason}`,
        blocked: true
      };
    }
  }
  
  // Proceed with execution
  return executeTool(toolCall);
}

function extractUrls(params: Record<string, unknown>): string[] {
  const urls: string[] = [];
  
  for (const value of Object.values(params)) {
    if (typeof value === 'string') {
      // Simple URL detection
      if (value.match(/^https?:\/\//)) {
        urls.push(value);
      }
    }
  }
  
  return urls;
}
```

### Gateway Middleware

```typescript
// src/gateway/ssrf-middleware.ts
import { SSRFGuard } from '../security/ssrf-guard';

const ssrfGuard = new SSRFGuard();

export async function ssrfMiddleware(
  request: Request,
  next: () => Promise<Response>
): Promise<Response> {
  // Check request URL if it contains user-controlled input
  const url = extractUrlFromRequest(request);
  
  if (url) {
    const validation = ssrfGuard.validateUrl(url);
    if (!validation.valid) {
      return new Response(
        JSON.stringify({
          error: 'SSRF Guard Blocked',
          reason: validation.reason
        }),
        { status: 403, headers: { 'Content-Type': 'application/json' } }
      );
    }
  }
  
  return next();
}
```

---

## Configuration

```yaml
# config/security.yaml
ssrf_guard:
  enabled: true
  mode: enforce  # enforce | warn | off
  
  allowed_schemes:
    - http
    - https
  
  allowed_hosts:
    - api.github.com
    - api.openai.com
    - api.anthropic.com
  
  blocked_ports:
    - 22   # SSH
    - 23   # Telnet
    - 25   # SMTP
    - 53   # DNS
    - 3306 # MySQL
    - 5432 # PostgreSQL
  
  # Additional IP ranges to block
  custom_blocked_ranges:
    - 10.0.0.0/8
    - 172.16.0.0/12
  
  logging:
    level: warn
    alert_on_block: true
```

---

## Testing

```typescript
// src/security/ssrf-guard.test.ts

describe('SSRFGuard', () => {
  const guard = new SSRFGuard();
  
  it('should allow valid external URLs', () => {
    expect(guard.validateUrl('https://api.github.com/users/octocat').valid).toBe(true);
    expect(guard.validateUrl('https://example.com/path').valid).toBe(true);
  });
  
  it('should block localhost', () => {
    const result = guard.validateUrl('http://localhost/admin');
    expect(result.valid).toBe(false);
    expect(result.blockedHost).toBe('localhost');
  });
  
  it('should block 127.0.0.1', () => {
    const result = guard.validateUrl('http://127.0.0.1:8080/');
    expect(result.valid).toBe(false);
  });
  
  it('should block AWS metadata', () => {
    const result = guard.validateUrl('http://169.254.169.254/latest/meta-data/');
    expect(result.valid).toBe(false);
    expect(result.reason).toContain('forbidden');
  });
  
  it('should block internal IPs', () => {
    expect(guard.validateUrl('http://10.0.0.1/').valid).toBe(false);
    expect(guard.validateUrl('http://192.168.1.1/').valid).toBe(false);
    expect(guard.validateUrl('http://172.16.0.1/').valid).toBe(false);
  });
  
  it('should block forbidden ports', () => {
    const result = guard.validateUrl('http://example.com:22/');
    expect(result.valid).toBe(false);
    expect(result.blockedPort).toBe(22);
  });
  
  it('should block file:// scheme', () => {
    const result = guard.validateUrl('file:///etc/passwd');
    expect(result.valid).toBe(false);
    expect(result.reason).toContain('Scheme');
  });
  
  it('should respect allowlist', () => {
    const guarded = new SSRFGuard({
      allowedHosts: ['internal.corp.local']
    });
    
    expect(guarded.validateUrl('http://internal.corp.local/').valid).toBe(true);
    expect(guarded.validateUrl('http://10.0.0.1/').valid).toBe(false);  // Still blocked
  });
});
```

---

## Deployment Checklist

- [ ] Enable SSRF guard in all tool execution paths
- [ ] Configure allowlist for legitimate internal APIs
- [ ] Set up alerting for blocked requests
- [ ] Review all existing tools for URL handling
- [ ] Add SSRF tests to CI/CD pipeline
- [ ] Document bypass procedures for emergencies

---

## References

- Night Cycle Report: night_cycle_20260412_0115.md
- OWASP SSRF Prevention Cheat Sheet
- AWS IMDS Documentation
- GCP Metadata Server Documentation