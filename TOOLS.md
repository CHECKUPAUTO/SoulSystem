# TOOLS.md - Local Notes

## Current Setup (2026-04-04)

### Servers

- **production-server** → `/mnt/nvme_secondary/` (nvme1n1p1, Debian, Ollama)
- **Backup path** → `/mnt/nvme_secondary/system_root/.openclaw/`

### Channels

- **Telegram** → Active, bot token configured, groups require mention
- **WhatsApp** → Self-chat mode, allowlist: +33XXXXXXXXX

### Models

- **Primary**: `ollama/qwen3-coder-next:cloud`
- **Ollama host**: `http://127.0.0.1:11434`
- **OpenClaw version**: 2026.4.2

## 🤖 Coding Agents (Available)

| Agent | Model/Runtime | Notes |
|-------|-------------|-------|
| **qwen3-coder-next:cloud** | Ollama | Puissant pour codage général |
| **Claude Code** | Anthropic | `~/.local/bin/claude`, analyse profonde |
| **Ruflo** | Ollama | Agent pour tâches diverses |

**Principe :** Ces outils sont **polyvalents**, pas des spécialisations rigides. Les solliciter selon le contexte — seul ou combinés — pour obtenir la meilleure qualité finale.

**Patterns d'usage :**
- **Génération simple** → qwen3-coder-next direct
- **Analyse complexe / multi-fichiers** → Claude Code
- **Doute sur l'approche** → Spawn 2 agents, comparer, fusionner le meilleur
- **Quick fix** → edit direct

**CodeWiki** : Analyse architecture, patterns codebase

### Storage

- `/mnt/nvme_secondary/openclaw/` → Main agent workspace
- `/mnt/nvme_secondary/ai_projects/openclaw/` → OpenClaw source (dev repo)
- `/mnt/nvme_secondary/projects/AionUi/` → AionUi coworking platform
- `/mnt/nvme_secondary/system_root/.openclaw/` → System backup workspace

### Fabric Patterns

Found at `/mnt/nvme_secondary/system_root/.config/fabric/patterns/`
- 253 patterns (agility, ai, analyze_*, etc.)
- Strategies: aot, cod, cot, ltm, reflexion, standard, etc.

## Evolution References

Located in `evolution/references/` - auto-generated docs from OpenEvolve Night Cycle:

### Architecture & Patterns
- `plugin_avoidance_pattern_2026-04-11.md` - Plugin import optimization pattern (direct registry vs barrel)
- `test_mock_consolidation_guide.md` - Centralized test fixture patterns
- `dreaming_ltm_architecture.md` - Long-Term Memory / Memory Palace cognitive model
- `ironreview_t430_integration.md` - T430 phase-shift evolutionary algorithm
- `session_state_management_patterns.md` - Runtime state extraction patterns
- `performance_optimization_patterns.md` - Static lookup optimization patterns
- `security_audit_patterns.md` - Security hardening patterns
- `codex_harness_integration_guide.md` - Codex app-server integration
- `error_handling_standardization_guide.md` - Unified error handling patterns
- `circuit_breaker_pattern.md` - Circuit breaker pattern for resilience (ported from VisionClaw)
- `startup_context_extraction_pattern.md` - Session state preloading patterns
- `barrel_bypassing_guide.md` - Eliminating circular dependencies via barrel removal
- `explicit_seams_pattern.md` - Explicit module boundary patterns
- `context_tree_pattern.md` - Immutable context tree architecture
- `narrow_surface_pattern.md` - Minimal API surface area principles

### Security & Reliability
- `visionclaw_security_remediation_guide.md` - P0 CRITICAL: VisionClaw security fixes (Keychain, TLS, circuit breaker)
- `security_fixes_20250411.md` - Recent security patches (TTL cleanup, SSRF guards)
- `cross_project_ecosystem_analysis_2026-04-11.md` - Cross-repository integration opportunities
- `active_memory_integration_testing_guide.md` - Active-memory context preservation testing
- `config_driven_fallback_pattern.md` - Removing built-in fallbacks for config-driven architecture
- `session_state_audit_trail.md` - Session state auditing for debugging and compliance
- `semantic_crossover_patterns.md` - T430 semantic crossover patterns for IronReview
- `startup_context_performance_monitoring.md` - Performance monitoring for startup context
- `startup_context_pattern.md` - Pattern for session state preloading across restarts
- `aion_mcp_bridge_guide.md` - AionUi MCP bridge integration patterns
- `vector_augmented_memory_vam.md` - Vector Augmented Memory (VAM) architecture
- `sqlite_fallback_strategy.md` - SQLite fallback for node:sqlite failures

### Active Development Areas (from Night Cycles)
- **Dreaming/LTM**: Memory-wiki extension with ChatGPT import, Memory Palace UI
- **IronReview v4**: Rust-based evolutionary code reviewer with T430 algorithm
- **Codex Harness**: Pluggable agent harness with strict-agentic execution contract
- **Performance**: Plugin barrel avoidance, static capability fast-paths
- **Active-Memory**: Session context preservation, fallback model cleanup
- **Circuit Breaker**: Resilience patterns for context engine latency cascades (from VisionClaw)
- **Barrel Elimination**: Systematic removal of barrel files to break circular dependencies

### Recent Analysis Reports (2026-04-12 Night Cycles)
- **03:45 Report**: Security hardening focus - authorization pattern consolidation across gateway endpoints
- **03:30 Report**: Circular dependency crisis analysis - 24 commits by Vincent Koc on barrel bypassing, type seam splitting
- **03:01 Report**: Active-memory stabilization and IronReview T430 integration opportunities

---

Add whatever helps you do your job. This is your cheat sheet.
