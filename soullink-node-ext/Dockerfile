# SoulLink Node — multi-stage Docker build
FROM rust:1.80-slim-bookworm AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy manifest files for dependency caching
COPY Cargo.toml Cargo.lock /build/
COPY soullink-node/Cargo.toml /build/soullink-node/
COPY soullink-ssm/Cargo.toml /build/soullink-ssm/
COPY scirust/scirust-core/Cargo.toml /build/scirust/scirust-core/

# Create dummy main.rs / lib.rs for dependency pre-build
RUN mkdir -p /build/soullink-node/src && \
    echo "fn main() {}" > /build/soullink-node/src/main.rs && \
    mkdir -p /build/soullink-ssm/src && \
    echo "pub fn dummy() {}" > /build/soullink-ssm/src/lib.rs && \
    mkdir -p /build/scirust/scirust-core/src && \
    echo "pub fn dummy() {}" > /build/scirust/scirust-core/src/lib.rs

# Build dependencies (cached layer)
RUN cargo build --release -p soullink-node 2>/dev/null || true

# Now copy real sources
COPY . /build/

# Build actual binary
RUN cargo build --release -p soullink-node

# ── Runtime stage ──
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libstd-rust-1.80 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/soullink-node /usr/local/bin/soullink-node

EXPOSE 8084

ENTRYPOINT ["soullink-node"]
