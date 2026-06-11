# SoulSystem — Quick Installation Guide (Debian 12)

This guide lets you install and run SoulSystem on a Debian 12 (Bookworm) machine in minutes.

## Prerequisites

- Debian 12 (Bookworm) freshly installed
- At least 8 GB RAM, 20 GB disk space
- Internet connection

## 1. Install system dependencies

```bash
sudo apt update
sudo apt install -y curl build-essential pkg-config libssl-dev nftables
```

## 2. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

## 3. Install Ollama

```bash
curl -fsSL https://ollama.com/install.sh | sh
```

Start the Ollama server and pull a lightweight model:

```bash
ollama serve &
ollama pull tinyllama
```

## 4. Clone SoulSystem

```bash
git clone https://github.com/CHECKUPAUTO/SoulSystem.git
cd SoulSystem
```

## 5. Compile

```bash
cargo build --release
```

Compilation takes a few minutes. The main binary is at `target/release/soulsystem`.

## 6. Minimum configuration

Create the default config:

```bash
mkdir -p /opt/soulsystem/config /var/lib/soulsystem/data /var/log/soulsystem
cp soulsystem.toml /opt/soulsystem/config/
```

The `soulsystem.toml` file contains default paths:

```toml
[paths]
config_dir = "/opt/soulsystem/config"
data_dir = "/var/lib/soulsystem/data"
log_dir = "/var/log/soulsystem"
```

## 7. Configure Telegram bot (Clawd Assistant)

1. Open Telegram and contact [@BotFather](https://t.me/BotFather)
2. Send `/newbot` and follow instructions
3. Note the **token** provided (e.g. `123456:ABC-DEF1234ghikl...`)

Export the token:

```bash
export TELEGRAM_BOT_TOKEN="${TELEGRAM_BOT_TOKEN}"
```

## 8. Launch SoulSystem

```bash
./target/release/soulsystem
```

The system starts:
- The OpenClaw-U kernel
- The SoulLink HNN neural mesh
- The Clawd assistant connected to Telegram
- Optional modules (AVID, OpenEvolve, SciRust, SYNERGIE) are **not** started by default.

## 9. Interact with Clawd

Open Telegram, find your bot, and start chatting. Clawd responds via the local tinyllama model.

## Troubleshooting

- **Ollama won't start**: check service with `ollama ps`
- **Compilation error**: run `rustup update` then retry
- **Clawd not responding**: check `export | grep TELEGRAM_BOT_TOKEN`