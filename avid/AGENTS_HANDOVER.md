### AVID Core Architectural Handover Prompt

**Target Agent:** Senior Software Engineer / Autonomous Systems Architect
**Context:** This prompt informs the agent about the completed evolution of the AVID (Autonomous Verification & Intelligent Development) core architecture.

#### 1. Core Architecture: Trait-Based Provider Pattern
The system has transitioned from concrete implementations to a trait-based provider pattern to ensure "production-grade" testability and decoupling.
- **`LlmProvider` (`avid-core/src/llm.rs`)**: Abstracted Ollama/Local-LLM interactions. Supports `chat`, `ping`, and `model` discovery. Includes a **Circuit Breaker** (5 failures/30s) and exponential backoff retries.
- **`KubernetesProvider` (`avid-core/src/integrations/kubernetes.rs`)**: Abstracted cluster diagnostics. Real implementation uses `kube-rs`.
- **`DatabaseProvider` (`avid-core/src/integrations/database.rs`)**: Abstracted schema analysis. Real implementation uses `sqlx` (Any driver). Supports `describe_table` and `check_performance`.

#### 2. Specialized Autonomous Agents
Routing logic in the `Orchestrator` now directs tasks based on `TaskType`.
- **`InfraAgent`**:
  - **Autonomous Diagnostic**: If the task goal includes "diagnose" or "health", it automatically fetches the real pod status from the "default" namespace (or via provider) to augment the prompt.
- **`SqlAgent`**:
  - **Dynamic Schema Injection**: Uses heuristic extraction to find potential table names in the user request.
  - **Context Augmentation**: Fetches real schemas (columns/types) for identified tables using `DATABASE_URL` before generating SQL.
- **`SecurityAgent` & `CodingAgent`**: Basic specialized routing structures are active.

#### 3. Operational Engines (Zero Stubs)
- **`ContextEngine`**: Implements recursive directory walking via `walkdir` to index project files into the `MemoryStore`. Hidden directories (e.g., `.git`) are ignored.
- **`PolicyEngine`**: Evaluates `Plan` objects against safety heuristics (e.g., forbidding data deletion or credential modification) before execution.

#### 4. Testing & Verification
- **100% Offline Capability**: All agents support `execute_with_provider`, allowing injection of `MockLlmClient`, `MockKubernetesClient`, and `MockDatabaseClient`.
- **Unit Tests**: Found in `crates/avid-core/src/agents/tests/mod.rs`. Use these as templates for new agent features.
- **CI Readiness**: Workspace is Clippy-clean. Use `cargo test --workspace` for verification.

#### 5. Integration Details
- **`avid-server`**: Updated `AppState` and handlers to handle trait objects (`Arc<dyn LlmProvider>`).
- **`avid-orchestrator`**: Updated pipeline to coordinate specialized agent execution with explicit timeouts (60s-90s).

**Current Status:** All mandatory constraints (NO STUBS, NO DEAD CODE, Rust-first) have been met. The system is ready for Phase 3 (IDE Integration) and Phase 6 (Incident Response depth).
