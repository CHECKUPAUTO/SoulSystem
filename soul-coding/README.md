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
- CodingAgent is the single provider-agnostic loop: model turn, typed tool
  calls, bounded budgets, and verifier finalization.
- SessionStore persists task/worktree identity, bounded conversation context,
  budgets, and final evidence; `--resume --session-id ...` reopens an
  interrupted detached worktree without silently losing the prior transcript.
- The `soul-coding` binary provides one command-line entry point for Ollama,
  OpenAI, and Anthropic configurations without putting API keys in arguments.
- CodingFeedback records accepted, rejected, and manually edited artifacts for
  a future preference-learning layer.

The crate intentionally does not implement Command Code's proprietary
taste-1 model. It records auditable feedback events that can later feed a
transparent, project-scoped preference system.

This is the canonical coding runtime for the repository. The existing general
autonomy, daemon, browser, and MCP runtimes remain adapters or follow-up
migrations; they are not duplicated inside this loop. Subagent orchestration
and optional browser/MCP adapters remain explicit follow-up slices. The current
binary is a usable coding harness foundation, not a drop-in replacement for a
mature product such as Cline, OpenCode, or Command Code.

Example:

```text
cargo run -p soul-coding -- \
  --repo . \
  --prompt "Fix the failing parser" \
  --check 'tests=cargo test -p my-crate'
```

`--check` values are persisted as shell-free, whitespace-separated argv. Use
one `--check NAME=COMMAND` per required acceptance check.

To resume a session, use the same repository and session identifier:

```text
cargo run -p soul-coding -- \
  --repo . \
  --resume \
  --session-id session-123
```

Session metadata is local repository state and can include verifier output;
do not use it as a channel for credentials.
