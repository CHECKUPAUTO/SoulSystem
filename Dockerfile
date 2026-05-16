FROM debian:12-slim

RUN apt update && apt install -y curl build-essential pkg-config libssl-dev

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN curl -fsSL https://ollama.com/install.sh | sh
RUN ollama pull tinyllama

WORKDIR /app
COPY . .

RUN cargo build --release --features dev

CMD ["./target/release/soulsystem", "--dev"]
