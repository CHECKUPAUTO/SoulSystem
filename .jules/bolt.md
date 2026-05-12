## 2026-05-11 - [Initial Bolt Entry]
**Learning:** Initializing Bolt's performance journal for SoulSystem.
**Action:** Focus on perception and bridge I/O bottlenecks.
## 2026-05-11 - [Perception/HNN Parallelization]
**Learning:** Sequential HTTP calls to multiple micro-services (HNN organs) and system checks (perception) created a significant bottleneck, potentially blocking the heartbeat for 30s+ if multiple services timed out.
**Action:** Use 'tokio::join!' for fixed-size async tasks and 'tokio::task::JoinSet' for dynamic loops to parallelize I/O. Always share a single 'reqwest::Client' for connection pooling.

## 2026-05-12 - [Perception I/O Non-Blocking & Parallelization]
**Learning:** Sequential calls to 'systemctl' and redundant 'reqwest::Client' instantiation in 'SystemSnapshot' were blocking the heartbeat loop and causing unnecessary resource overhead. Using 'tokio::process::Command' and a shared client with 'JoinSet' parallelization is essential for maintaining a low-latency heartbeat.
**Action:** Always pass a shared '&reqwest::Client' to perception/bridge functions and use 'tokio::process' for any shell-based metric collection.
