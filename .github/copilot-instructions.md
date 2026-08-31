# SoulSystem repository agent instructions

Before repository changes, fetch and read the persistent off-main roadmap:

```bash
git fetch origin agent/ecosystem-roadmap && \
git show origin/agent/ecosystem-roadmap:.agent/SOULSYSTEM_ECOSYSTEM_ROADMAP.yaml
```

Treat root `AGENTS.md` as mandatory bootstrap policy. If the roadmap is unavailable, fail closed for major coding-harness, security, session-format, cross-repository integration, or merge decisions.

Preserve the canonical detached-worktree coding harness, explicit acceptance checks, repository-policy loading, real isolation boundaries, and exact-head CI gates. Do not create a second competing coding runtime merely to add features.
