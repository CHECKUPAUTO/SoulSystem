# ═══════════════════════════════════════════════
# SoulSystem Evolution v0.2 — Dockerfile Production
# Rust 1.86 (requis pour edition2024/hashbrown)
# ═══════════════════════════════════════════════

# ── Étape 1 : Build ──────────────────────────
FROM rust:1.86-bookworm AS builder

WORKDIR /build

# Cache des dépendances
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    cargo build --release 2>/dev/null || true && \
    rm -rf src

# Build réel — touch force la recompilation
COPY src/ ./src/
RUN touch src/main.rs && cargo build --release && strip target/release/soulsystem

# ── Étape 2 : Runtime ────────────────────────
# NOTE: On garde Rust dans le runtime car le sandbox compile du code agent
FROM rust:1.86-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 procps curl \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -r soulsystem && useradd -r -g soulsystem -m -s /bin/bash soulsystem

WORKDIR /opt/soulsystem
COPY --from=builder /build/target/release/soulsystem .
COPY entrypoint.sh .
RUN chmod +x entrypoint.sh

RUN mkdir -p data/snapshots data/agents data/sandbox logs && \
    chown -R soulsystem:soulsystem /opt/soulsystem

USER soulsystem

ENV RUST_LOG=info

ENTRYPOINT ["./entrypoint.sh"]
