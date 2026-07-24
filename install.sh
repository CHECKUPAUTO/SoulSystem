#!/bin/sh
# SoulSystem installer.
#
#   curl -fsSL https://raw.githubusercontent.com/Memorithm/SoulSystem/main/install.sh | sh
#
# Installs the `soulsystem` binary. Prefers a prebuilt release binary for your
# platform; falls back to building from source with cargo (installing the Rust
# toolchain via rustup if it is missing). POSIX sh — no bashisms.

set -eu

REPO="Memorithm/SoulSystem"
BIN="soulsystem"
# Override with: SOULSYSTEM_INSTALL_DIR=/usr/local/bin sh install.sh
INSTALL_DIR="${SOULSYSTEM_INSTALL_DIR:-$HOME/.local/bin}"
# Override with: SOULSYSTEM_VERSION=v0.6.0 sh install.sh   (default: latest)
VERSION="${SOULSYSTEM_VERSION:-latest}"

info() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33mwarning:\033[0m %s\n' "$1" >&2; }
err()  { printf '\033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

# ── Detect platform ────────────────────────────────────────────────────────
detect_target() {
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux)  os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *) err "unsupported OS: $os (build from source instead)" ;;
  esac
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) err "unsupported architecture: $arch" ;;
  esac
  echo "${arch}-${os}"
}

# ── Try a prebuilt release binary ──────────────────────────────────────────
try_prebuilt() {
  target="$1"
  if [ "$VERSION" = "latest" ]; then
    url="https://github.com/${REPO}/releases/latest/download/${BIN}-${target}.tar.gz"
  else
    url="https://github.com/${REPO}/releases/download/${VERSION}/${BIN}-${target}.tar.gz"
  fi
  info "Looking for a prebuilt binary: $url"
  tmp="$(mktemp -d)"
  archive="$tmp/${BIN}-${target}.tar.gz"
  checksum="${archive}.sha256"
  if curl -fsSL "$url" -o "$archive" 2>/dev/null &&
     curl -fsSL "${url}.sha256" -o "$checksum" 2>/dev/null; then
    if command -v sha256sum >/dev/null 2>&1; then
      (cd "$tmp" && sha256sum -c "$(basename "$checksum")") >/dev/null ||
        err "release checksum verification failed"
    elif command -v shasum >/dev/null 2>&1; then
      (cd "$tmp" && shasum -a 256 -c "$(basename "$checksum")") >/dev/null ||
        err "release checksum verification failed"
    else
      err "sha256sum or shasum is required to verify the release"
    fi
    tar -xzf "$archive" -C "$tmp"
    [ -f "$tmp/$BIN" ] || err "release archive does not contain $BIN"
    mkdir -p "$INSTALL_DIR"
    install -m 0755 "$tmp/$BIN" "$INSTALL_DIR/$BIN"
    rm -rf "$tmp"
    return 0
  fi
  rm -rf "$tmp"
  return 1
}

# ── Build from source ──────────────────────────────────────────────────────
ensure_cargo() {
  if command -v cargo >/dev/null 2>&1; then
    return 0
  fi
  warn "cargo not found; installing the Rust toolchain via rustup"
  curl -fsSL https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
}

build_from_source() {
  ensure_cargo
  info "Building $BIN from source (this can take a few minutes)…"
  if [ -f "Cargo.toml" ] && grep -q 'name = "soulsystem"' Cargo.toml 2>/dev/null; then
    src="."
  else
    src="$(mktemp -d)/SoulSystem"
    info "Cloning $REPO…"
    git clone --depth 1 "https://github.com/${REPO}.git" "$src"
  fi
  ( cd "$src" && cargo build --release --bin "$BIN" )
  mkdir -p "$INSTALL_DIR"
  install -m 0755 "$src/target/release/$BIN" "$INSTALL_DIR/$BIN"
}

# ── Main ───────────────────────────────────────────────────────────────────
main() {
  command -v curl >/dev/null 2>&1 || err "curl is required"
  target="$(detect_target)"

  if try_prebuilt "$target"; then
    info "Installed prebuilt binary."
  else
    warn "No prebuilt binary available; falling back to source build."
    build_from_source
  fi

  info "$BIN installed to $INSTALL_DIR/$BIN"
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *) warn "$INSTALL_DIR is not on your PATH. Add this to your shell profile:"
       printf '\n    export PATH="%s:$PATH"\n\n' "$INSTALL_DIR" ;;
  esac
  info "Run '$BIN --help' to get started."
}

main "$@"
