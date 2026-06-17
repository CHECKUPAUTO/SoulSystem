# SoulSystem Quick Reference

## 🚀 Quick Launch

| Action | Command |
|--------|---------|
| Check system status | `./launch-autonomous.sh status` |
| Launch REPL | `./launch-autonomous.sh repl` |
| Ask question | `./launch-autonomous.sh ask "..."` |
| Create plan | `./launch-autonomous.sh plan "..."` |

## 💬 REPL Commands

| Command | What it does |
|----------|-------------|
| `/ask <text>` | Ask the LLM a question |
| `/help <topic>` | Show help for a topic |
| `/models` | List available Ollama models |
| `/status` | Show system status (LLM, CPU, RAM, Docker) |
| `/plan <goal>` | Create an action plan |
| `/run <cmd>` | Execute a shell command |
| `/tools` | List all available tools |
| `/memory` | View working memory |
| `/observe <text>` | Record an observation |
| `/decide <context>` | Make a decision |
| `/history` | View action history |
| `/clear` | Clear conversation |
| `/save <name>` | Save current session |
| `/export <format>` | Export data (json/yaml/txt) |
| `/files <pattern>` | List files matching pattern |
| `/search <query>` | Search in memory & files |

## ⌨️ TUI Shortcuts

| Shortcut | Action |
|-----------|--------|
| `Ctrl+Shift+P` | Command palette |
| `Ctrl+F` | File browser |
| `Ctrl+R` | Search history |
| `Ctrl+O` | Session manager |
| `Ctrl+Y` | Copy to clipboard |
| `Ctrl+E` | Export chat |
| `Shift+Enter` | Multi-line input |

## 🔧 Configuration

```bash
# Main files
~/.config/soulsystem/config.toml    # Main configuration
~/.config/soulsystem/.env           # Environment variables
```

```bash
# Key environment variables
export TELEGRAM_BOT_TOKEN="..."
export OLLAMA_HOST="http://localhost:11434"
export OLLAMA_MODEL="qwen3:4b"
```

## 🩺 Troubleshooting

| Problem | Fix |
|---------|-----|
| Ollama not running | `sudo systemctl start ollama` |
| Port in use | `sudo kill $(lsof -t :9030)` |
| Config errors | `soulsystem --debug` |
| Permission denied | `chmod 700 ~/.config/soulsystem/` |

## 📚 Full Documentation

- **User Guide**: `docs/USER_GUIDE.md`
- **Guide Utilisateur**: `docs/USER_GUIDE.fr.md`
- **Quick Reference**: `docs/QUICK_REFERENCE.md` (this file)
- **Fiche Référence**: `docs/QUICK_REFERENCE.fr.md`
