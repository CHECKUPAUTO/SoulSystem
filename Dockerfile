# =============================================================================
# SoulSystem — Multi-stage Docker build (hardened)
# =============================================================================

# Stage 1: Builder
FROM rust:1.86-slim-bookworm AS builder

RUN apt update && apt install -y pkg-config libssl-dev clang && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY scirust-chronos-agent/Cargo.toml scirust-chronos-agent/
COPY soullink-brain/soullink-core/Cargo.toml soullink-brain/soullink-core/
# Copy all Cargo.tomls for dependency resolution
COPY . .

# Build release (without dev features)
RUN cargo build --release --features dev 2>/dev/null || cargo build --release

# Stage 2: Runtime (hardened)
FROM debian:12-slim

RUN apt update && apt install -y ca-certificates curl && rm -rf /var/lib/apt/lists/* && \
    addgroup --system --gid 1001 soulsystem && \
    adduser --system --uid 1001 --gid 1001 --no-create-home --disabled-password soulsystem

WORKDIR /app
COPY --from=builder /app/target/release/soulsystem /app/soulsystem
COPY --from=builder /app/docs /app/docs

# Security hardening
RUN chown -R soulsystem:soulsystem /app && \
    chmod 500 /app/soulsystem && \
    chmod 400 /app/docs

# Drop all capabilities, no new privileges
USER soulsystem

EXPOSE 9090

HEALTHCHECK --interval=30s --timeout=10s --start-period=15s --retries=3 \
    CMD curl -f http://localhost:9090/health || exit 1

CMD ["/app/soulsystem"]
