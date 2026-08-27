# soul-coding

soul-coding is the foundation of the canonical SoulSystem coding harness.

This first slice defines contracts without executing models, commands, or Git
operations:

- TaskSpec requires explicit acceptance checks.
- TaskResult::completed requires a real change set and passing required checks.
- WorkspaceContext binds file resolution to a canonical worktree.
- CodingFeedback records accepted, rejected, and manually edited artifacts for
  a future preference-learning layer.

The crate intentionally does not implement Command Code's proprietary
taste-1 model. It records auditable feedback events that can later feed a
transparent, project-scoped preference system.

Next slices will add Git worktrees, typed coding tools, sandboxed checks,
persistent sessions, and the model adapter.