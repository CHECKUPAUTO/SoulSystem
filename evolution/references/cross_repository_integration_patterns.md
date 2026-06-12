# Cross-Repository Integration Patterns

Design patterns for integrating OpenClaw with external repositories and services.

Based on Night Cycle analysis (2026-04-10).

---

## Pattern 1: Skill-Based Integration

**Use case**: Wrap external service/dataset as OpenClaw skill

### Structure

```
external-service-skill/
├── SKILL.md                    # High-level usage guide
├── scripts/
│   └── client.py              # Service client wrapper
└── references/
    ├── api.md                 # Complete API reference
    └── examples.md            # Usage examples
```

### Implementation

```python
# scripts/client.py
class ExternalServiceClient:
    """Wrapper for external service API."""
    
    def __init__(self, api_key: Optional[str] = None):
        self.api_key = api_key or os.getenv("SERVICE_API_KEY")
        self.base_url = "https://api.service.com/v1"
    
    def query(self, **params) -> dict:
        """Execute query against service."""
        # Implementation
        pass
    
    def health_check(self) -> bool:
        """Verify service connectivity."""
        pass
```

### Example: The Well Skill

```python
# Usage from OpenClaw:
well = WellClient()
datasets = well.list_datasets()
trajectory = well.load_trajectory("rayleigh_benard", idx=42)
```

**Benefits**:
- Natural language queries via OpenClaw
- Tool delegation from other systems (VisionClaw, etc.)
- Unified secrets management via OpenClaw

---

## Pattern 2: Gateway Delegation

**Use case**: External system delegates execution to OpenClaw skills

### Flow

```
External System (VisionClaw)
    ↓
"Execute task: query The Well dataset"
    ↓
OpenClaw Gateway (http://host:18888)
    ↓
Skill Router → the-well skill
    ↓
Result → External System
```

### Protocol

**Request**:
```json
{
  "task": "Query rayleigh_benard trajectory 42",
  "context": {
    "user_id": "...",
    "session_key": "..."
  }
}
```

**Response**:
```json
{
  "status": "success",
  "result": { "data": "...", "metadata": "..." },
  "execution_time_ms": 150
}
```

### Implementation in External System

```swift
// VisionClaw example (simplified)
func executeViaOpenClaw(task: String) async throws -> Result {
    let request = OpenClawRequest(task: task)
    let response = try await httpClient.post(
        url: "\(gatewayUrl)/execute",
        body: request,
        headers: ["Authorization": "Bearer \(token)"]
    )
    return response.result
}
```

**Security Requirements**:
- TLS required for production
- Token-based authentication
- Request timeout (10s default)
- Retry logic with exponential backoff

---

## Pattern 3: Bidirectional Event Stream

**Use case**: Real-time synchronization between systems

### Architecture

```
┌─────────────┐      WebSocket       ┌─────────────┐
│  OpenClaw   │ ←────────────────→  │   External  │
│   Gateway   │    Events + Data    │   System    │
└─────────────┘                     └─────────────┘
      ↑                                   ↑
      └────────── Event Bus ──────────────┘
```

### Event Schema

```typescript
interface OpenClawEvent {
  id: string;           // UUID v4
  type: string;         // "tool_call", "session_update", "skill_result"
  timestamp: number;    // Unix ms
  payload: unknown;
  source: string;       // Originating system
  signature?: string;   // HMAC for verification
}
```

### Example Events

```json
// tool_call event
{
  "id": "evt_123...",
  "type": "tool_call",
  "timestamp": 1712345678000,
  "payload": {
    "skill": "the-well",
    "method": "load_trajectory",
    "args": ["rayleigh_benard", 42]
  },
  "source": "visionclaw"
}

// skill_result event
{
  "id": "evt_124...",
  "type": "skill_result",
  "timestamp": 1712345678150,
  "payload": {
    "request_id": "evt_123...",
    "result": { "shape": [200, 512, 128, 2] },
    "status": "success"
  },
  "source": "openclaw"
}
```

---

## Pattern 4: Shared Secrets Service

**Use case**: Centralized secrets management across multiple systems

### Architecture

```
┌─────────────────────────────────────────┐
│         Secrets Service (Vault)         │
│  ┌─────────────────────────────────┐  │
│  │  /openclaw/*                    │  │
│  │  /visionclaw/*                  │  │
│  │  /external-service/*             │  │
│  └─────────────────────────────────┘  │
└─────────────────────────────────────────┘
           ↑           ↑           ↑
    ┌──────┘           └──────┐    └──────┐
    ▼                           ▼           ▼
┌──────────┐              ┌──────────┐  ┌──────────┐
│ OpenClaw │              │VisionClaw│  │  Other   │
│ Gateway  │              │          │  │ Services │
└──────────┘              └──────────┘  └──────────┘
```

### Secret Rotation Flow

```
1. Secrets Service rotates API key
2. Publishes "secret.rotated" event
3. Systems fetch new key from Vault
4. Old key expires after grace period
```

### Implementation

```python
class SecretsClient:
    def get(self, path: str) -> str:
        """Fetch secret from Vault."""
        pass
    
    def rotate(self, path: str) -> str:
        """Rotate secret and return new value."""
        pass
    
    def subscribe(self, path: str, callback: Callable):
        """Subscribe to secret rotation events."""
        pass
```

---

## Pattern 5: Circuit Breaker + Retry

**Use case**: Resilient integration with unreliable external services

### State Machine

```
          ┌──────────┐
    ┌─────┤  CLOSED  │◄──── Success threshold met
    │     │ (normal) │
    │     └────┬─────┘
    │          │ Failure
    │          ▼
    │     ┌──────────┐
    │     │   OPEN   │ Timeout: 30s
    └─────┤  (failing) │
          └────┬─────┘
               │ After timeout
               ▼
          ┌──────────┐
          │  HALF-OPEN │ Test request
          └────┬─────┘
               │
               ▼
          Success → CLOSED
          Failure → OPEN
```

### Implementation

```python
from enum import Enum, auto

class CircuitState(Enum):
    CLOSED = auto()
    OPEN = auto()
    HALF_OPEN = auto()

class CircuitBreaker:
    def __init__(
        self,
        failure_threshold: int = 5,
        timeout_seconds: int = 30,
        half_open_max_calls: int = 3
    ):
        self.failure_threshold = failure_threshold
        self.timeout_seconds = timeout_seconds
        self.half_open_max_calls = half_open_max_calls
        self.state = CircuitState.CLOSED
        self.failures = 0
        self.last_failure_time = None
    
    async def call(self, fn: Callable, *args, **kwargs):
        if self.state == CircuitState.OPEN:
            if self._should_attempt_reset():
                self.state = CircuitState.HALF_OPEN
            else:
                raise CircuitBreakerOpen()
        
        try:
            result = await fn(*args, **kwargs)
            self._record_success()
            return result
        except Exception as e:
            self._record_failure()
            raise
    
    def _record_success(self):
        self.failures = 0
        self.state = CircuitState.CLOSED
    
    def _record_failure(self):
        self.failures += 1
        self.last_failure_time = time.time()
        if self.failures >= self.failure_threshold:
            self.state = CircuitState.OPEN
```

---

## Pattern 6: Adaptive Rate Limiting

**Use case**: Dynamic request throttling based on service health

### Algorithm

```python
class AdaptiveRateLimiter:
    """Token bucket with dynamic rate adjustment."""
    
    def __init__(self, initial_rate: float = 10.0):
        self.rate = initial_rate  # requests per second
        self.tokens = initial_rate
        self.last_update = time.time()
        self.error_rate = 0.0
    
    async def acquire(self):
        self._replenish()
        if self.tokens < 1:
            wait = (1 - self.tokens) / self.rate
            await asyncio.sleep(wait)
            self.tokens = 0
        else:
            self.tokens -= 1
    
    def report_error(self):
        """Reduce rate on error."""
        self.error_rate = min(1.0, self.error_rate + 0.1)
        self.rate = max(1.0, self.rate * 0.8)
    
    def report_success(self):
        """Gradually increase rate on success."""
        self.error_rate *= 0.95  # Decay
        if self.error_rate < 0.1:
            self.rate = min(100.0, self.rate * 1.05)
```

---

## Pattern 7: Health Check + Auto-Failover

**Use case**: Multi-instance deployment with automatic failover

### Architecture

```
┌─────────────────────────────────┐
│         Load Balancer            │
│    ┌─────────────────────┐      │
│    │   Health Check Poll  │      │
│    │   Every 5 seconds    │      │
│    └─────────────────────┘      │
└──────────┬──────────┬───────────┘
           │          │
    ┌──────▼──┐  ┌───▼──────┐
    │Gateway 1│  │Gateway 2 │
    │(active) │  │(standby) │
    └─────────┘  └──────────┘
```

### Health Check Endpoint

```python
@app.get("/health")
async def health_check():
    """Comprehensive health check."""
    checks = {
        "database": await check_database(),
        "ollama": await check_ollama(),
        "skills": await check_skills(),
        "memory": get_memory_usage()
    }
    
    healthy = all(c["status"] == "ok" for c in checks.values())
    
    return {
        "status": "healthy" if healthy else "degraded",
        "checks": checks,
        "timestamp": datetime.utcnow().isoformat()
    }
```

### Failover Logic

```python
class GatewayPool:
    def __init__(self, endpoints: List[str]):
        self.endpoints = endpoints
        self.healthy = set(endpoints)
    
    async def execute(self, task: str) -> Result:
        for endpoint in self._prioritized_endpoints():
            try:
                return await self._call(endpoint, task)
            except Exception as e:
                self.healthy.discard(endpoint)
                logger.warning(f"Gateway {endpoint} failed: {e}")
        
        raise AllGatewaysFailed()
    
    def _prioritized_endpoints(self) -> List[str]:
        """Return endpoints in priority order (healthy first)."""
        healthy = [e for e in self.endpoints if e in self.healthy]
        unhealthy = [e for e in self.endpoints if e not in self.healthy]
        return healthy + unhealthy
```

---

## Pattern 8: Observability Integration

**Use case**: Unified metrics, logging, and tracing across systems

### Metrics Schema

```python
# Prometheus-compatible metrics
class IntegrationMetrics:
    def __init__(self):
        self.request_duration = Histogram(
            'integration_request_duration_seconds',
            'Request duration',
            ['service', 'method']
        )
        self.request_count = Counter(
            'integration_requests_total',
            'Total requests',
            ['service', 'method', 'status']
        )
        self.error_count = Counter(
            'integration_errors_total',
            'Total errors',
            ['service', 'error_type']
        )
```

### Distributed Tracing

```python
from opentelemetry import trace

tracer = trace.get_tracer(__name__)

class TracedClient:
    async def execute(self, task: str):
        with tracer.start_as_current_span("integration.execute") as span:
            span.set_attribute("task.length", len(task))
            span.set_attribute("service", "the-well")
            
            try:
                result = await self._execute(task)
                span.set_status(Status(StatusCode.OK))
                return result
            except Exception as e:
                span.set_status(Status(StatusCode.ERROR))
                span.record_exception(e)
                raise
```

---

## Integration Checklist

Before deploying any cross-repository integration:

### Security
- [ ] TLS enforced for production
- [ ] Secrets in Vault/Keychain, not code
- [ ] Token rotation mechanism
- [ ] Request signing for sensitive ops
- [ ] Rate limiting configured

### Reliability
- [ ] Retry logic with backoff
- [ ] Circuit breaker for external calls
- [ ] Timeout handling (10s default)
- [ ] Health check endpoint
- [ ] Graceful degradation

### Observability
- [ ] Metrics export (Prometheus)
- [ ] Structured logging
- [ ] Distributed tracing
- [ ] Error tracking (Sentry)
- [ ] Performance dashboards

### Documentation
- [ ] API reference
- [ ] Integration patterns
- [ ] Error handling guide
- [ ] Security considerations
- [ ] Troubleshooting playbook

---

## References

- [OpenClaw Gateway Docs](https://docs.openclaw.ai)
- [The Well Dataset Paper](https://arxiv.org/abs/...)
- [VisionClaw Integration](https://github.com/Intent-Lab/VisionClaw)
- [Circuit Breaker Pattern](https://martinfowler.com/bliki/CircuitBreaker.html)