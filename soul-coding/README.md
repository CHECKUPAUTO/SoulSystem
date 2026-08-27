# soul-coding

soul-coding is the foundation of the canonical SoulSystem coding harness.

The current slice provides:

- TaskSpec requires explicit acceptance checks.
- TaskResult::completed requires a real change set and passing required checks.
- WorkspaceContext binds file resolution to a canonical worktree.
- CommandSpec keeps process arguments structured and shell-free.
- SandboxCommandRunner executes checks with the existing SoulSystem sandbox.
- GitWorkspace collects porcelain status and a reproducible diff hash.
- Verifier and CodingRuntime preserve check evidence when completion is not
  justified.
- CodingFeedback records accepted, rejected, and manually edited artifacts for
  a future preference-learning layer.

The crate intentionally does not implement Command Code's proprietary
taste-1 model. It records auditable feedback events that can later feed a
transparent, project-scoped preference system.

The model adapter, persistent sessions, typed coding tools, and the single
canonical ReAct loop are the next slices. Until that work lands, this crate
does not claim to be a complete autonomous coding client.
