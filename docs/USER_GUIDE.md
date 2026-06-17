# SoulSystem - Comprehensive User Guide

Autonomous digital entity framework with multi-provider LLM support.

## Table of Contents

1. [About SoulSystem](#about-soulsystem)
2. [Installation](#installation)
3. [Quick Start](#quick-start)
4. [REPL Commands](#repl-commands)
5. [CLI Commands](#cli-commands)
6. [TUI Commands](#tui-commands)
7. [Configuration](#configuration)
8. [Integrations](#integrations)
9. [API Reference](#api-reference)
10. [Troubleshooting](#troubleshooting)
11. [Usage Examples](#usage-examples)

---

# About SoulSystem

SoulSystem is a unified Rust workspace that integrates the original SoulSystem monolith, the autonomous agent monolith (`soul_agent_core`, `soul_entity`, `souls`, ...), the SoulLink Neural Mesh, SciRust core, and CCOS (Causal Context Operating System).

## Key Features

- **Autonomous agents** with ReAct loop (observe→plan→act→evaluate)
- **Multi-LLM support** (Ollama, OpenAI, Anthropic, etc.)
- **40+ system tools** automatically discovered
- **Complete Docker and system integration**
- **Built-in self-healing and metacognition**
- **Multilingual documentation** (French and English)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    AutonomousEntity                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   soul_llm  │  │soul_planner │  │ soul_tools  │        │
│  │  (Ollama)   │  │  (Cognitive)│  │  (40+ tools) │        │
│  └─────────────┘  └─────────────┘  └─────────────┘        │
│                          │                                   │
│  ┌───────────────────────┴───────────────────────┐         │
│  │              soul_bridges                      │         │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐     │         │
│  │  │OpenEvolve│ │  Docker  │ │ Monitor  │     │         │
│  │  │(Evolution)│ │(Containers)│ │(CPU/RAM) │     │         │
│  │  └──────────┘ └──────────┘ └──────────┘     │         │
│  └───────────────────────────────────────────────┘         │
└─────────────────────────────────────────────────────────────┘
```

---

# Installation

## Prerequisites

1. **Rust** 1.75+ installed
   ```bash
   rustup --version
   ```

2. **Ollama** with a model installed
   ```bash
   ollama serve &
   ollama pull qwen3:4b
   ```

3. **Docker** (optional, for container management)
   ```bash
   docker --version
   ```

4. **Git** 2.0+ for cloning

## Step-by-Step Installation

### Option 1: Install via Cargo (Recommended)

```bash
# Clone the repository
git clone https://github.com/copilotacker/SoulSystem
cd SoulSystem

# Build and install in release mode
cargo build --release

# Install main binaries
sudo cp target/release/soulsystem /usr/local/bin/
```

### Option 2: Using the Autonomous Launch Script

```bash
# Clone the repository if not already done
git clone https://github.com/copilotacker/SoulSystem
cd SoulSystem

# Make the script executable
chmod +x launch-autonomous.sh

# Install services
sudo cp soulsystem-autonomous.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable soulsystem-autonomous
sudo systemctl start soulsystem-autonomous
```

### Option 3: Manual Installation with All Components

```bash
# Clone all submodules (requires more disk space)
git clone --recursive https://github.com/copilotacker/SoulSystem

# Full workspace build
cd SoulSystem
cargo build --release

# Install selected components
sudo cp target/release/soulsystem /usr/local/bin/
```

## Verifying Installation

```bash
# Check service status
systemctl status soulsystem-autonomous

# Test the main binary
soulsystem --help

# Launch the interactive REPL
soulsystem --repl
```

---

# Quick Start

## 1. Check Full Status

```bash
./launch-autonomous.sh status
```

## 2. Launch Interactive REPL

```bash
./launch-autonomous.sh repl
```

## 3. Ask a Question

```bash
./launch-autonomous.sh ask "Analyze the system state"
```

## 4. Create a Plan

```bash
./launch-autonomous.sh plan "Optimize performance"
```

---

# REPL Commands

The REPL (Read-Eval-Print Loop) is the main interface for interacting with autonomous agents.

| Command | Description | Example |
|----------|-------------|---------|
| `/ask <msg>` | Ask the LLM a question | `/ask What are the current system metrics?` |
| `/help <topic>` | Show help for a topic | `/help tools` |
| `/models` | List all available Ollama models | `/models` |
| `/status` | Show full system status (LLM, CPU, RAM, Docker) | `/status` |
| `/plan <goal>` | Create a plan to achieve a goal | `/plan Update documentation` |
| `/run <cmd>` | Execute a shell command | `/run df -h` |
| `/tools` | List all available tools | `/tools` |
| `/memory` | View current working memory | `/memory` |
| `/observe <msg>` | Record an observation in memory | `/observe Visited /etc/passwd` |
| `/decide <ctx>` | Make a decision based on context | `/decide Analyze system logs` |
| `/history` | View action history | `/history` |
| `/clear` | Clear current conversation | `/clear` |
| `/save <name>` | Save current session | `/save session-2026-06-17` |
| `/export <format>` | Export data (json, yaml, txt) | `/export json` |
| `/files <pattern>` | List files matching pattern | `/files *.rs` |
| `/search <query>` | Search in memory and files | `/search performance` |

### REPL Usage Examples

```bash
# Start a conversation
soulsystem --repl

# Ask about system metrics
/ask What are the current CPU metrics?

# List available tools
/tools

# View current working memory
/memory

# Create a plan for a task
/plan "Update user documentation"

# Execute a system command
/run ps aux | grep ollama

# Record an observation
/observe Checked system performance metrics

# Make a decision
/decide Recommend upgrading to qwen3:8b model

# View history
/history
```

---

# CLI Commands

CLI commands provide programmatic access to SoulSystem features.

```bash
/ask, /help, /models, /status, /plan, /run,
/observe, /decide, /clear, /save, /export,
/files, /search
```

### CLI Command Usage

```bash
# Ask a question via CLI
soulsystem /ask "Analyze the system"

# Create a plan via CLI
soulsystem /plan "Optimize performance"

# Execute a command via CLI
soulsystem /run uptime

# View status via CLI
soulsystem /status
```

---

# TUI Commands

The TUI (Terminal User Interface) provides a graphical text-mode interface with keyboard support.

| Shortcut | Action | Description |
|-----------|--------|-------------|
| `Ctrl+Shift+P` | Command Palette | Show command palette (like VS Code) |
| `Ctrl+F` | File Browser | Open file browser |
| `Ctrl+R` | Search History | Search conversation history |
| `Ctrl+O` | Session Manager | Open session manager |
| `Ctrl+Y` | Copy to Clipboard | Copy selection to clipboard |
| `Ctrl+E` | Export Chat | Export current conversation |
| `Shift+Enter` | Multi-line Input | Insert a newline in the editor |

### TUI Navigation

```bash
# Launch the TUI interface
soulsystem --dev

# Use keyboard shortcuts
Ctrl+Shift+P    → Command Palette
Ctrl+F        → File Browser
Ctrl+R        → Search History
Ctrl+O        → Session Manager
Ctrl+Y        → Copy
Ctrl+E        → Export
Shift+Enter   → New Line
```

---

# Configuration

## Configuration Files

### `~/.config/soulsystem/`

Create this directory and add the following configuration files:

#### `config.toml` (Main Configuration)

```toml
title = "SoulSystem"

[llm]
provider = "ollama"
model = "qwen3:4b"
host = "http://localhost:11434"
timeout = 30

[system]
monitor_interval = 5
cpu_threshold = 80
memory_threshold = 85

[docker]
enabled = true
endpoint = "unix:///var/run/docker.sock"

[soul_bridge]
rest_port = 9030
enable_telegram = false
telegram_token = ""
```

#### `.env` (Environment Variables)

```bash
# LLM Configuration
OLLAMA_HOST=http://localhost:11434
OLLAMA_MODEL=qwen3:4b

# Telegram Configuration
TELEGRAM_BOT_TOKEN=your_bot_token_here
TELEGRAM_CHAT_ID=your_chat_id

# SoulSystem Configuration
SOULSYSTEM_LOG_LEVEL=info
SOULSYSTEM_MAX_CONVERSATIONS=100

# Bridge Configuration
BRIDGE_REST_PORT=9030
BRIDGE_TELEGRAM_ENABLED=true
```

#### `extensions.json` (Available Extensions)

```json
[
  {
    "name": "telegram",
    "enabled": true,
    "config": {
      "bot_token": "${TELEGRAM_BOT_TOKEN}",
      "chat_id": "${TELEGRAM_CHAT_ID}"
    }
  },
  {
    "name": "docker",
    "enabled": true,
    "config": {
      "endpoint": "unix:///var/run/docker.sock"
    }
  },
  {
    "name": "system_monitor",
    "enabled": true,
    "config": {
      "interval": 5
    }
  }
]
```

---

# Integrations

## Ollama (Primary LLM)

- **Status**: ✅ Active
- **Model**: qwen3:4b (default)
- **Streaming**: NDJSON
- **Integration**: Local, keyless, fast

### Command to Change Model

```bash
/soulsystem ask "Change to llama2:7b model"
```

## Docker (Containers)

- **Status**: ✅ Active
- **Features**:
  - `list_containers()` - List all containers
  - `start_container(name)` - Start a container
  - `stop_container(name)` - Stop a container
  - `is_docker_running()` - Check if Docker is running

### Using Docker Tools

```bash
/run docker ps -a
/run docker start my-app
/run docker stop my-app
```

## OpenEvolve (Self-Evolution)

- **Status**: ⏳ Available (if service is running on configured port)
- **Feature**: Code self-evolution via LLM

## System Monitor (System Monitoring)

- **Status**: ✅ Active
- **Metrics**: CPU, Memory, Processes, Load
- **Source**: Real-time metrics from `/proc`

### Monitoring Commands

```bash
/run free -h
/run mpstat 1 3
/run iostat -x 1
```

---

# API Reference

## REST Endpoints

### `/api/v1/`

| Method | Endpoint | Description |
|---------|----------|-------------|
| `GET` | `/status` | Returns full system status |
| `POST` | `/ask` | Ask the LLM a question |
| `POST` | `/plan` | Create a plan |
| `POST` | `/run` | Execute a shell command |
| `GET` | `/tools` | List available tools |
| `GET` | `/memory` | View working memory |
| `POST` | `/observe` | Record an observation |
| `POST` | `/decide` | Make a decision |

### `/api/v1/health`

Returns system health status.

```json
{
  "status": "healthy",
  "timestamp": "2026-06-17T10:30:00Z",
  "services": {
    "llm": "connected",
    "docker": "connected",
    "system_monitor": "active"
  },
  "uptime": "2h 15m 30s"
}
```

### `/api/v1/telegram`

Telegram webhook management.

```json
{
  "webhook": {
    "url": "https://your-domain.com/api/v1/telegram",
    "allowed_updates": ["message"]
  },
  "bot_info": {
    "username": "soul_system_bot",
    "first_name": "Soul",
    "description": "Autonomous agent system"
  }
}
```

---

# Troubleshooting

## Common Errors and Solutions

### 1. Ollama Not Installed or Not Running

**Problem**: Ollama is not installed or not running.

**Solution**:
```bash
# Install Ollama (Debian/Ubuntu)
curl -fsSL https://ollama.com/install.sh | sh

# Start Ollama
sudo systemctl start ollama
sudo systemctl enable ollama

# Download a model
sudo -u ollama ollama pull qwen3:4b
```

### 2. Telegram Token Not Configured

**Problem**: The Telegram bot is not configured.

**Solution**:
```bash
# Get a Telegram token
https://t.me/BotFather
/call /start
/call /newbot

# Set environment variables
echo "TELEGRAM_BOT_TOKEN=your_token_here" >> .env
```

### 3. Ports Blocked

**Problem**: Required ports are already in use.

**Solution**:
```bash
# Check which processes are using the ports
netstat -tulpn | grep :9030
netstat -tulpn | grep :11434

# Kill conflicting processes
sudo kill <pid>

# Or change port configuration in config.toml
```

### 4. Insufficient Permissions

**Problem**: SoulSystem lacks necessary permissions.

**Solution**:
```bash
# Ollama permissions
sudo usermod -aG ollama $USER

# Docker permissions
sudo usermod -aG docker $USER

# Config file permissions
chmod 700 ~/.config/soulsystem/
chmod 600 ~/.config/soulsystem/*.toml
```

### 5. Incorrect Configuration

**Problem**: Configuration is incorrect.

**Solution**:
```bash
# Validate TOML configuration
cargo run --bin config-validator

# Display current configuration
soulsystem /status

# Show configuration errors
soulsystem --debug
```

## Diagnostic Commands

```bash
# Check system status
./launch-autonomous.sh status

# View logs
journalctl -u soulsystem-autonomous -f

# Test connectivity
./launch-autonomous.sh test

# Reload configuration
sudo systemctl reload soulsystem-autonomous

# Restart service
sudo systemctl restart soulsystem-autonomous
```

---

# Usage Examples

## Example 1: Complete System Monitoring

```bash
# 1. Launch the system
./launch-autonomous.sh repl

# 2. Request system analysis
/ask "Give me a complete analysis of current system metrics"

# 3. List available tools
/tools

# 4. View current working memory
/memory

# 5. Create an optimization plan
/plan "Optimize performance and reduce latency"

# 6. Run diagnostic commands
/run uptime
/run free -h
/run mpstat 1 5

# 7. Record observations
/observe System running smoothly with 45% CPU usage
/observe Memory usage at 67% of total
/observe Ollama responding normally

# 8. Make a decision
/decide Recommend monitoring CPU usage during peak hours
```

## Example 2: Docker Management via Telegram

```bash
# 1. Configure the Telegram bot
# Get token from @BotFather
# Add to .env
TELEGRAM_BOT_TOKEN=your_token
TELEGRAM_CHAT_ID=your_chat_id

# 2. Launch the system
./launch-autonomous.sh repl

# 3. Check Docker integration
/ask "List all running Docker containers"

# 4. Take action via Docker tool
/run docker ps -a
/run docker stats --no-stream

# 5. Send notification via Telegram
/telegram "Container usage: high"
```

## Example 3: Self-Healing and Metacognition

```bash
# 1. Launch the system
./launch-autonomous.sh repl

# 2. Request system metacognition
/ask "Analyze system health and suggest improvements"

# 3. View self-healing opportunities
/memory

# 4. Create a self-healing plan
/plan "Implement auto-healing mechanisms and improve error recovery"

# 5. Run maintenance tasks
/run journalctl -u soulsystem --since "1 hour ago" | grep ERROR
/run sudo systemctl status ollama

# 6. Record results
/observe Auto-healing completed successfully
/observe System health improved

# 7. Make final decision
/decide System is stable and ready for production
```

## Quick Reference

```bash
# Primary Commands (REPL)
/ask <msg>        - Ask the LLM a question
/plan <goal>      - Create a plan
/run <cmd>        - Execute a shell command
/tools            - List available tools
/memory          - View working memory
/observe <msg>    - Record an observation
/decide <ctx>     - Make a decision
/history          - View action history
/status           - View system status
/models          - List available models
/clear            - Clear conversation
/save <name>      - Save session
/export <format>  - Export data
/files <pattern>  - List files
/search <query>   - Search

# TUI Commands
Ctrl+Shift+P      - Command Palette
Ctrl+F           - File Browser
Ctrl+R           - Search History
Ctrl+O           - Session Manager
Ctrl+Y           - Copy
Ctrl+E           - Export
Shift+Enter      - New Line
```

---

# License

MIT

---

# Support

For any questions, issues, or suggestions, please contact the SoulSystem team at support@soulsystem.ai.

You can also open an issue on GitHub: https://github.com/copilotacker/SoulSystem/issues

Feedback is always welcome!
