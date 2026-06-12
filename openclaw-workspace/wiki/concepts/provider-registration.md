# Provider Registration

_Concept page — how OpenClaw discovers and registers model providers._

## Architecture
- **Plugin system**: Each provider is a plugin (e.g., ollama, google, anthropic)
- **Manifest**: `openclaw.plugin.json` declares providers, auth choices, contracts
- **Registry**: `manifest-registry` loads all plugin manifests at runtime
- **Auth choices**: Mapped from `providerAuthChoices` in manifest → shown in onboard wizard

## How It Works
1. Plugin declares `providerAuthChoices` in manifest (choiceId, groupId, label, hint)
2. `manifest-registry` collects all choices from enabled plugins
3. Onboard wizard shows choices grouped by `groupId`/`groupLabel`
4. User selects → custom auth `run()` function executes configuration

## Ollama Plugin
- **Manifest**: `dist/extensions/ollama/openclaw.plugin.json`
- **ChoiceId**: "ollama" (groupId: "ollama", kind: "custom")
- **Auth flow**: `promptAndConfigureOllama()` — detects local/remote, pulls models
- **Discovery**: Ambient model detection on startup (can be disabled)
- **Default API key**: "ollama-local"

## Current Issues
- 3 stale entries in `plugins.allow`: serve, onboard, doctor (not found)
- These cause warnings but don't block ollama loading
- `pi-tools.before-tool-call.runtime` module missing — blocks exec tool entirely
- This may prevent onboard/configure from running properly

## Fix Required
1. Remove stale plugin entries from `plugins.allow`
2. Reinstall/update OpenClaw to fix missing pi-tools module
3. Then ollama should appear in onboard/configure provider list

## See Also
- [openclaw](../entities/openclaw.md)