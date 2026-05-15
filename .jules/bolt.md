## 2026-05-11 - [Initial Bolt Entry]
**Learning:** Initializing Bolt's performance journal for SoulSystem.
**Action:** Focus on perception and bridge I/O bottlenecks.
## 2026-05-11 - [Perception/HNN Parallelization]
**Learning:** Sequential HTTP calls to multiple micro-services (HNN organs) and system checks (perception) created a significant bottleneck, potentially blocking the heartbeat for 30s+ if multiple services timed out.
**Action:** Use 'tokio::join!' for fixed-size async tasks and 'tokio::task::JoinSet' for dynamic loops to parallelize I/O. Always share a single 'reqwest::Client' for connection pooling.

## 2026-05-12 - [Parallel Systemctl Checks]
**Learning:** Sequential execution of `systemctl is-active` using `std::process::Command` in an async context was a major bottleneck in the perception loop. Moving to `tokio::process::Command` combined with `JoinSet` reduced perception latency by ~71% (~270ms to ~78ms).
**Action:** Parallelize external process checks using `JoinSet` and `tokio::process::Command` to avoid blocking the async executor and minimize cycle time.
