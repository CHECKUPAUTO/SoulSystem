## 2026-05-11 - [Initial Bolt Entry]
**Learning:** Initializing Bolt's performance journal for SoulSystem.
**Action:** Focus on perception and bridge I/O bottlenecks.
## 2026-05-11 - [Perception/HNN Parallelization]
**Learning:** Sequential HTTP calls to multiple micro-services (HNN organs) and system checks (perception) created a significant bottleneck, potentially blocking the heartbeat for 30s+ if multiple services timed out.
**Action:** Use 'tokio::join!' for fixed-size async tasks and 'tokio::task::JoinSet' for dynamic loops to parallelize I/O. Always share a single 'reqwest::Client' for connection pooling.

## 2026-05-11 - [Blocking I/O in Async Context]
**Learning:** Using `std::process::Command` in an async heartbeat loop blocks the executor thread.
**Action:** Use `tokio::process::Command` and `.await` the output to ensure the async runtime remains responsive.

## 2026-05-11 - [Connection Pooling]
**Learning:** Components like `WeaviateMemory` and `OnaeuBridge` were creating their own `reqwest::Client`, leading to socket exhaustion and high handshake latency.
**Action:** Inject a shared `reqwest::Client` into all components that perform network I/O to leverage TCP connection pooling.
