# SoulSystem Operator Edition — Guide Operateur v0.5.0

## Architecture

SoulSystem is a modular autonomous agent ecosystem in Rust.

### Active Modules

| Module | Role |
|--------|------|
| `audit_log` | Signed audit journal with integrity verification |
| `bus` | Internal messaging system (broadcast channel) |
| `code_signing` | Code certification (SHA-256 HMAC) |
| `compute_backend` | CPU/GPU abstraction — CUDA/ROCm/Vulkan detection |
| `config` | Centralized configuration (TOML + env vars) |
| `discovery` | mDNS service discovery |
| `dev_dashboard` | SSE dashboard on :9090 (feature `dev`) |
| `anomaly` | HNN tick drop detector (feature `dev`) |
| `soul_memory` | Local vector memory (sled + SciRustEmbedder 64-dim) |
| `telemetry` | Distributed metrics (OTLP) |
| `clawd` | Telegram bot with commands and LLM routing |
| `model_router` | Dynamic Ollama model selection |
| `bound_system` | Secure shell command execution with signal support |
| `local_skills` | Plugin system with signature verification |
| `backup` | Signed backup/restore |
| `ansi_converter` | ANSI escape sequence → Telegram emoji/HTML conversion |
| `spinner` | Animated braille spinner for streaming progress |
| `terminal_stream` | Streaming command output to Telegram with stop button |
| `pty_terminal` | Persistent PTY terminal with tmux support |

## Installation

```bash
cargo build --release
sudo cp target/release/soulsystem /usr/local/bin/
```

For dev mode (dashboard + anomaly):

```bash
cargo build --release --features dev
```

## Configuration

File `soulsystem.toml`:

```toml
[paths]
config_dir = "/opt/soulsystem/config"
data_dir   = "/var/lib/soulsystem/data"
log_dir    = "/var/log/soulsystem"
```

Environment overrides:
- `SOULSYSTEM_CONFIG_FILE` — alternative config path
- `QDRANT_URL` — Qdrant endpoint (optional, sled fallback)
- `OTEL_EXPORTER_OTLP_ENDPOINT` — telemetry endpoint (default :4317)
- `SOULSYSTEM_BOT_TOKEN` — Telegram bot token for Clawd

## Usage

```bash
# Normal start
soulsystem

# Dev mode (dashboard :9090 + anomaly detection)
soulsystem --dev

# Mock mode (simulation)
soulsystem --mock
```

## Clawd — Telegram Bot

Commands available:
- `/veille <topics>` — Subscribe to AVID research topics
- `/skill <name> <args>` — Execute a local skill
- `/run <command>` — Execute an authorized system command (with spinner + stop button)
- `/terminal` — Open a persistent PTY terminal
- `/exit` — Close the PTY terminal
- `/help` — Show help

### Feedback utilisateur

Each Clawd response includes 👍/👎 inline buttons. Feedback is stored in
`FeedbackStore` (sled) with timestamp, query, response, and score (+1/-1).

```rust
// Retrieve recent feedback
let entries = clawd.feedback.get_recent(100)?;
```

### Model Router

Clawd uses `ModelRouter` to select the best Ollama model based on query complexity:

| Complexity | Model | Capacity | Cost |
|-----------|-------|----------|------|
| < 0.2 | tinyllama | 0.2 | 0.1 |
| < 0.35 | llama3.2:1b | 0.35 | 0.25 |
| < 0.6 | mistral | 0.6 | 0.5 |
| < 0.75 | codellama | 0.75 | 0.7 |
| >= 0.75 | deepseek-coder-v2:16b | 0.95 | 1.0 |

Complexity is evaluated by: length, keywords (explain, code, implement, debug), code indicators.

### Veille personnalisee

`/veille sujet1, sujet2` registers topics for daily AVID research.
Results are stored in SoulMemory with tag "veille".

### Taches systeme (Bound System)

Authorized commands: `date`, `df -h`, `uptime`, `free -h`, `whoami`, `hostname`, `ps aux`.

Execution is sandboxed via bubblewrap (`bwrap`):
- Network disabled (`--unshare-net`)
- Timeout: 10 seconds
- All executions audited
- **Signal support**: `kill_process(pid, signal)` via `libc::kill` for SIGTERM/SIGKILL

### Streaming automatique des commandes

Toute commande shell detectee dans les reponses du LLM est automatiquement
executee et sa sortie est diffusee en direct dans Telegram:

- Blocs ```shell dans la reponse → execution streamée
- Lignes prefixees `$` ou `>` → execution streamée
- `/run <cmd>` → streaming manuel

**Nouveau (v0.5.0)** :
- **Spinner animé** : Un spinner braille (`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`) tourne dans le message
  pendant l'exécution, remplacé par ✅ ou ❌ à la fin.
- **Bouton Stop** : Un bouton inline `⏹️ Stop` permet d'arrêter l'exécution.
  Envoie SIGTERM puis SIGKILL après 3 secondes.
- **Couleurs ANSI** : Les séquences d'échappement ANSI sont converties :
  - `\x1b[31m` (rouge) → 🔴
  - `\x1b[32m` (vert) → 🟢
  - `\x1b[33m` (jaune) → 🟡
  - `\x1b[1m` (gras) → `<b>...</b>` (HTML Telegram)

La sortie est mise a jour en temps réel dans un message Telegram.
Limites: 30 dernieres lignes visibles, timeout 10s.

### Terminal integre Telegram

Un shell bash persistant accessible via la commande `/terminal`:

```
/terminal   # Ouvre un terminal bash (sandbox bwrap ou tmux)
/exit       # Ferme le terminal
```

Caracteristiques:
- PTY natif via `portable-pty` (entierement interactif)
- Isolation bwrap: `/usr`, `/lib`, `/bin`, `/etc` en read-only, reseau desactive
- **Persistance tmux** (nouveau v0.5.0) : Si `tmux` est installé, le terminal utilise
  une session tmux nommée `soulsystem_<chat_id>`. La session survit aux redémarrages
  de Clawd — il suffit de refaire `/terminal` pour se reconnecter.
- **Fallback automatique** : Si tmux n'est pas installé, le PTY standard est utilisé.
  Un avertissement informe l'utilisateur que le terminal ne survivra pas à un redémarrage.
- **Bouton Stop** : En mode terminal, un bouton `⏹️ Stop` envoie Ctrl+C (0x03) au PTY
  pour interrompre le processus en cours.
- **Couleurs ANSI** : La sortie du terminal est convertie automatiquement (émojis + HTML).
- Etat persistant: variables, historique, processus en cours conserves
- Timeout: 30 min d'inactivite → fermeture automatique
- Sortie diffusee en temps reel (rafraichissement 500ms)
- Audit: commandes enregistrees dans l'AuditLog
- **Sauvegarde environnement** : Les variables d'environnement importantes
  (`PATH`, `HOME`, `USER`, `SHELL`, `SOUL*`) sont sauvegardées dans
  `~/.soulsystem/pty_env_<chat_id>.json` et restaurées à la réouverture.

En mode terminal, tout message texte (hors commandes slash) est transmis au shell.
Les commandes sont executees dans un sandbox identique au Bound System.

### Securite du terminal

- Sandbox obligatoire si `bwrap` est disponible (fallback bash direct sinon, en test uniquement)
- Commandes auditees mais la sortie n'est pas stockee (volume trop important)
- PTY redimensionnable (defaut 24×80) — resize automatique ignore dans l'implementation actuelle

## Sauvegarde

```bash
# Create signed backup
soulsystem backup create --output /backup/soulsystem-$(date +%Y%m%d).tar.gz

# Verify backup
soulsystem backup verify /backup/soulsystem-20260517.tar.gz
```

Backups include `data_dir` and `config_dir`, compressed as tar.gz, SHA-256 hashed,
and HMAC-signed with the instance private key.

## Tests de charge

```bash
# Run load tests once
cargo test --test load_test

# Endurance test (5 minutes default)
./scripts/stress_test.sh

# Endurance test (30 minutes)
./scripts/stress_test.sh 30
```

## Supervision

- **Dashboard** : `http://localhost:9090` (requires `--dev`)
- **Bus** : modules subscribe to the bus for alerts
- **Anomaly** : detects >40% HNN tick rate drops, 60s cooldown

## Troubleshooting

| Problem | Likely Cause | Solution |
|----------|-------------|----------|
| `soulmemory: QDRANT_URL not set` | No Qdrant | Normal, uses local sled |
| `mDNS unavailable` | `mdns-sd` not installed | Local registry only |
| Dashboard unreachable | `--dev` not enabled | Rebuild with `--features dev` |
| Clawd not responding | No `SOULSYSTEM_BOT_TOKEN` | Set env var and restart |
| Ollama errors | Ollama service down | Check `systemctl status ollama` |
| Commands rejected | Not in whitelist | Use `/run` help for allowed commands |
| Backup verify fails | Wrong private key | Re-create backup with current key |
| Terminal non persistant | tmux non installé | `apt install tmux` |
| Bouton Stop sans effet | Processus déjà terminé | Normal si la commande est rapide |
