# =============================================================================
# SoulSystem — Multi-stage Docker build
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

# Stage 2: Runtime
FROM debian:12-slim

RUN apt update && apt install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/soulsystem /app/soulsystem
COPY --from=builder /app/docs /app/docs

EXPOSE 9090

CMD ["/app/soulsystem"]
