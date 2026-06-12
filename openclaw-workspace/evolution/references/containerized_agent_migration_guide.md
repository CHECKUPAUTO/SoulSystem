# Containerized Agent Migration Guide

**Source:** OpenEvolve Night Cycle Report 2026-04-12 01:15 UTC  
**Priority:** P0 - Critical Production Readiness  
**Risk:** Fork Bomb, Zombie Processes, Cold Start Latency

---

## Problem Statement

Current OpenClaw subagent isolation relies on process-based separation without containerization. This creates several production risks:

| Risk | Impact | Current Mitigation |
|------|--------|-------------------|
| Fork Bomb | PID exhaustion under high load | None (single supervisor) |
| Zombie Processes | Resource leaks from crashes | Supervisor restart only |
| Cold Start Latency | 500-2000ms per spawn | No warm pool |
| Resource Limits | No cgroup enforcement | OS defaults only |

---

## Target Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    OpenClaw Gateway                         │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐       │
│  │   Warm Pool │    │   Active    │    │   Draining  │       │
│  │  (pre-warmed│    │ Containers  │    │  (cleanup) │       │
│  │ containers) │    │             │    │             │       │
│  └─────────────┘    └─────────────┘    └─────────────┘       │
├─────────────────────────────────────────────────────────────┤
│                    containerd / runc                          │
├─────────────────────────────────────────────────────────────┤
│              Linux cgroups + namespaces                     │
└─────────────────────────────────────────────────────────────┘
```

---

## Implementation

### Phase 1: ContainerizedAgent Core

```rust
// src/oci/containerized-agent.rs
use containerd_client::Client;
use tokio::sync::Mutex;

pub struct ContainerizedAgent {
    container_id: String,
    warm_pool: Arc<Mutex<Vec<Container>>>,
    resource_limits: CgroupLimits,
    client: Client,
}

#[derive(Clone)]
struct Container {
    id: String,
    status: ContainerStatus,
    created_at: DateTime<Utc>,
    last_used: DateTime<Utc>,
}

#[derive(Debug)]
struct CgroupLimits {
    cpu_quota: i64,      // microseconds per period
    cpu_period: u64,     // microseconds
    memory_limit: i64, // bytes
    pids_limit: i64,     // max processes
}

impl ContainerizedAgent {
    pub async fn new(config: AgentConfig) -> Result<Self> {
        let client = Client::connect("/run/containerd/containerd.sock").await?;
        
        Ok(Self {
            container_id: generate_id(),
            warm_pool: Arc::new(Mutex::new(Vec::with_capacity(config.warm_pool_size))),
            resource_limits: config.limits,
            client,
        })
    }
    
    pub async fn spawn(&mut self, task: Task) -> Result<AgentHandle> {
        // Try warm pool first
        let container = {
            let mut pool = self.warm_pool.lock().await;
            pool.pop()
        };
        
        let container = match container {
            Some(c) => {
                tracing::info!("Using warmed container {}", c.id);
                c
            }
            None => {
                tracing::info!("Creating new container (cold start)");
                self.create_container().await?
            }
        };
        
        // Apply resource limits
        self.apply_limits(&container).await?;
        
        // Execute task
        let handle = AgentHandle {
            container_id: container.id.clone(),
            task_id: task.id.clone(),
        };
        
        // Spawn background task
        tokio::spawn(async move {
            // Task execution logic
        });
        
        Ok(handle)
    }
    
    async fn create_container(&self) -> Result<Container> {
        // Create container via containerd
        let spec = self.build_spec().await?;
        let id = format!("openclaw-agent-{}", uuid::Uuid::new_v4());
        
        // containerd API call
        let response = self.client.create_container(id.clone(), spec).await?;
        
        Ok(Container {
            id,
            status: ContainerStatus::Created,
            created_at: Utc::now(),
            last_used: Utc::now(),
        })
    }
    
    async fn apply_limits(&self, container: &Container) -> Result<()> {
        let cgroup_path = format!("/sys/fs/cgroup/openclaw/{}", container.id);
        
        // Write cgroup limits
        tokio::fs::write(
            format!("{}/cpu.max", cgroup_path),
            format!("{} {}", self.resource_limits.cpu_quota, self.resource_limits.cpu_period)
        ).await?;
        
        tokio::fs::write(
            format!("{}/memory.max", cgroup_path),
            self.resource_limits.memory_limit.to_string()
        ).await?;
        
        tokio::fs::write(
            format!("{}/pids.max", cgroup_path),
            self.resource_limits.pids_limit.to_string()
        ).await?;
        
        Ok(())
    }
}
```

### Phase 2: Warm Pool Management

```rust
// src/oci/warm-pool.rs
pub struct WarmPool {
    containers: Arc<Mutex<Vec<Container>>>,
    target_size: usize,
    shutdown: Arc<AtomicBool>,
}

impl WarmPool {
    pub fn new(target_size: usize) -> Self {
        Self {
            containers: Arc::new(Mutex::new(Vec::with_capacity(target_size))),
            target_size,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }
    
    pub async fn start_warming(&self, client: Client) {
        let containers = self.containers.clone();
        let target = self.target_size;
        let shutdown = self.shutdown.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                
                let current = containers.lock().await.len();
                if current < target {
                    let to_create = target - current;
                    tracing::info!("Warming {} containers", to_create);
                    
                    for _ in 0..to_create {
                        match Self::create_warmed(&client).await {
                            Ok(c) => containers.lock().await.push(c),
                            Err(e) => tracing::error!("Failed to warm container: {}", e),
                        }
                    }
                }
            }
        });
    }
    
    async fn create_warmed(client: &Client) -> Result<Container> {
        // Pre-pull image, pre-initialize runtime
        // Return container ready for immediate use
        todo!()
    }
    
    pub async fn acquire(&self) -> Option<Container> {
        self.containers.lock().await.pop()
    }
    
    pub async fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
        
        let mut containers = self.containers.lock().await;
        for container in containers.drain(..) {
            // Graceful cleanup
            let _ = container.destroy().await;
        }
    }
}
```

### Phase 3: Health Checks & Cleanup

```rust
// src/oci/health-monitor.rs
pub struct HealthMonitor {
    containers: Arc<Mutex<HashMap<String, ContainerHealth>>>,
}

#[derive(Debug)]
struct ContainerHealth {
    last_ping: DateTime<Utc>,
    exit_code: Option<i32>,
    oom_killed: bool,
}

impl HealthMonitor {
    pub async fn start_monitoring(&self) {
        let containers = self.containers.clone();
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                let mut guard = containers.lock().await;
                let now = Utc::now();
                
                // Find dead/stuck containers
                let dead: Vec<String> = guard
                    .iter()
                    .filter(|(_, h)| now.signed_duration_since(h.last_ping) > Duration::minutes(5))
                    .map(|(id, _)| id.clone())
                    .collect();
                
                // Clean up
                for id in dead {
                    tracing::warn!("Cleaning up unresponsive container {}", id);
                    guard.remove(&id);
                    // Force kill via containerd
                }
            }
        });
    }
}
```

---

## Configuration

```yaml
# config/containerized-agent.yaml
containerized_agent:
  enabled: true
  runtime: containerd
  
  warm_pool:
    size: 10
    refill_interval: 30s
    max_age: 1h
  
  limits:
    cpu_quota: 100000    # 100ms per 100ms period = 1 CPU
    cpu_period: 100000   # 100ms
    memory_limit: 536870912  # 512MB
    pids_limit: 50
  
  health_check:
    interval: 10s
    timeout: 5s
    unhealthy_threshold: 3
  
  cleanup:
    grace_period: 30s
    force_kill_after: 60s
```

---

## Migration Path

### Step 1: Feature Flag

```typescript
// src/agents/spawn.ts
export async function spawnAgent(task: Task): Promise<AgentHandle> {
  if (config.features.containerizedAgents) {
    return spawnContainerized(task);
  }
  return spawnProcessBased(task);  // Legacy
}
```

### Step 2: Gradual Rollout

| Phase | Percentage | Criteria to Advance |
|-------|------------|---------------------|
| Canary | 5% | No increased error rate for 24h |
| Beta | 25% | Latency p99 < 100ms cold start |
| GA | 100% | Resource usage stable, no zombie processes |

### Step 3: Legacy Deprecation

```typescript
// src/agents/legacy-spawn.ts
/** @deprecated Use ContainerizedAgent instead */
export async function spawnProcessBased(task: Task): Promise<AgentHandle> {
  console.warn("Process-based isolation is deprecated. Migrate to ContainerizedAgent.");
  // ... legacy implementation
}
```

---

## Expected Improvements

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Cold Start | 500-2000ms | 50-100ms | 10-40x faster |
| PID Usage | Unbounded | Limited | Prevents exhaustion |
| Memory Leaks | Possible | Contained | cgroup limits |
| Zombie Processes | Risk | Eliminated | Health monitoring |
| Resource Isolation | None | Full | cgroups + namespaces |

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| containerd dependency | Fallback to process-based if unavailable |
| Image size bloat | Use distroless base images |
| Privilege escalation | Run rootless containers |
| Network isolation | Use CNI plugins |

---

## References

- Night Cycle Report: night_cycle_20260412_0115.md
- IronReview T430 Analysis
- containerd Documentation: https://containerd.io/docs/