# Installation Guide

## Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/yourusername/soullink-orchestrator.git
cd soullink-orchestrator

# 2. Build
./build.sh

# 3. Install
sudo cp target/release/soullink-orchestrator /usr/local/bin/

# 4. Run
soullink-orchestrator
```

## Systemd Service

```bash
# Create user
sudo useradd -r -s /bin/false soullink

# Install service
sudo cp systemd/soullink-orchestrator.service /etc/systemd/system/
sudo systemctl daemon-reload

# Enable and start
sudo systemctl enable --now soullink-orchestrator

# Check status
sudo systemctl status soullink-orchestrator
```

## Docker (optional)

```dockerfile
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/soullink-orchestrator /usr/local/bin/
EXPOSE 9020
CMD ["soullink-orchestrator"]
```

```bash
docker build -t soullink-orchestrator .
docker run -p 9020:9020 soullink-orchestrator
```
