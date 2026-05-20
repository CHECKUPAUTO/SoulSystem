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

## 2026-05-12 - [Perception I/O Non-Blocking & Parallelization]
**Learning:** Sequential calls to 'systemctl' and redundant 'reqwest::Client' instantiation in 'SystemSnapshot' were blocking the heartbeat loop and causing unnecessary resource overhead. Using 'tokio::process::Command' and a shared client with 'JoinSet' parallelization is essential for maintaining a low-latency heartbeat.
**Action:** Always pass a shared '&reqwest::Client' to perception/bridge functions and use 'tokio::process' for any shell-based metric collection.

## 2026-05-13 - [Batching Shell Commands for Performance]
**Learning:** Even with async execution, spawning dozens of separate processes (e.g., `systemctl is-active`) in every heartbeat cycle is expensive and can lead to PID exhaustion or high kernel overhead. Most CLI tools support multiple arguments.
**Action:** Batch multiple service checks into a single `systemctl is-active svc1 svc2...` call and parse the multi-line output. This provides a ~3-5x performance boost in the perception loop.
