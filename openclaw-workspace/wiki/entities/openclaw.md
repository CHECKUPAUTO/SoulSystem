# OpenClaw

_Entity page — the platform I run on._

## Overview
- **Type**: Personal AI agent platform
- **Stack**: Node.js, TypeScript, bundled dist
- **Version**: 2026.4.12
- **Gateway**: Port 18890, loopback, token auth
- **Channels**: Telegram (active), WhatsApp (active)
- **Default model**: ollama/glm-5.1:cloud (203k ctx)

## Architecture
- **Gateway**: HTTP/WS server (axum-style routing in Node)
- **Plugin system**: Manifest registry, provider auth choices, dynamic loading
- **Providers**: Registered via plugins (ollama, google, anthropic, openai, mistral, etc.)
- **Sessions**: 41 active, 15 stores
- **Workspace**: /root/.openclaw/workspace

## Known Issues
- **pi-tools module missing**: `pi-tools.before-tool-call.runtime-0rjjLwul.js` not found — blocks all exec operations
- **Stale plugins**: serve, onboard, doctor in `plugins.allow` — not found, cause warnings
- **CDP duplicate tabs**: #13851, #12317 (P1)
- **PR #63680 stale**: Security fix CVSS 8.5, 5+ days unmerged
- **Issue #63686**: Discord ACP regression, 5+ days stale
- **Config world-readable**: openclaw.json mode 644 (CRITICAL)

## Plugin System
- Built-in plugins: ollama (enabled), memory-core, device-pair, telegram, whatsapp
- Installed: openclaw-web-search (unpinned @ollama/openclaw-web-search)
- Ollama plugin declares `providerAuthChoices` with choiceId "ollama" (kind: custom)
- Discovery: ambient Ollama model detection on startup (can be disabled)

## Config Location
- `/root/.openclaw/openclaw.json`
- Backup: `/root/.openclaw/openclaw.json.bak`

## See Also
- [provider-registration](../concepts/provider-registration.md)
- [persistence-architecture](../concepts/persistence-architecture.md)