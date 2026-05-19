## 2026-05-11 - [Initial Bolt Entry]
**Learning:** Initializing Bolt's performance journal for SoulSystem.
**Action:** Focus on perception and bridge I/O bottlenecks.
## 2026-05-11 - [Perception/HNN Parallelization]
**Learning:** Sequential HTTP calls to multiple micro-services (HNN organs) and system checks (perception) created a significant bottleneck, potentially blocking the heartbeat for 30s+ if multiple services timed out.
**Action:** Use 'tokio::join!' for fixed-size async tasks and 'tokio::task::JoinSet' for dynamic loops to parallelize I/O. Always share a single 'reqwest::Client' for connection pooling.

<<<<<<< HEAD
## 2026-05-11 - [Blocking I/O in Async Context]
**Learning:** Using `std::process::Command` in an async heartbeat loop blocks the executor thread.
**Action:** Use `tokio::process::Command` and `.await` the output to ensure the async runtime remains responsive.

## 2026-05-11 - [Connection Pooling]
**Learning:** Components like `WeaviateMemory` and `OnaeuBridge` were creating their own `reqwest::Client`, leading to socket exhaustion and high handshake latency.
**Action:** Inject a shared `reqwest::Client` into all components that perform network I/O to leverage TCP connection pooling.
=======
## 2026-05-12 - [Perception I/O Non-Blocking & Parallelization]
**Learning:** Sequential calls to 'systemctl' and redundant 'reqwest::Client' instantiation in 'SystemSnapshot' were blocking the heartbeat loop and causing unnecessary resource overhead. Using 'tokio::process::Command' and a shared client with 'JoinSet' parallelization is essential for maintaining a low-latency heartbeat.
**Action:** Always pass a shared '&reqwest::Client' to perception/bridge functions and use 'tokio::process' for any shell-based metric collection.

## 2026-05-13 - [Search Database O(N*M) Anti-pattern]
**Learning:** Iterating through search results and performing a full `get_all_entities()` call inside the loop creates an $O(N \times M)$ bottleneck that scales poorly as the database grows.
**Action:** Always implement and use primary-key indexed lookups (e.g., `get_by_id`) for entity resolution in loops. Use database-side `COUNT(*)` instead of fetching full collection to get length.
