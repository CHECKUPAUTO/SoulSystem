#!/usr/bin/env bash
# Installation SYNERGIE v3 sur Debian 13.
# Usage: sudo bash scripts/install.sh
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SYS_USER="synergie"
INSTALL_DIR="/var/lib/synergie"
ETC_DIR="/etc/synergie"
BIN_PATH="/usr/local/bin/synergie"
SERVICE_PATH="/etc/systemd/system/synergie.service"

echo "→ vérification prérequis"
if [ "$(id -u)" -ne 0 ]; then
    echo "Ce script doit être lancé en root (sudo)." >&2
    exit 1
fi
command -v cargo >/dev/null || { echo "cargo manquant — installer rustup d'abord" >&2; exit 1; }
command -v systemctl >/dev/null || { echo "systemctl manquant" >&2; exit 1; }

echo "→ utilisateur système ${SYS_USER}"
if ! id "${SYS_USER}" >/dev/null 2>&1; then
    useradd --system --create-home --home-dir "${INSTALL_DIR}" --shell /usr/sbin/nologin "${SYS_USER}"
fi

echo "→ répertoires"
install -d -m 0750 -o "${SYS_USER}" -g "${SYS_USER}" "${INSTALL_DIR}"
install -d -m 0755 "${ETC_DIR}"
install -d -m 0755 -o "${SYS_USER}" -g "${SYS_USER}" /mnt/nvme/soullink_brain/synergies || true

echo "→ build release (cela peut prendre quelques minutes)"
cd "${REPO_ROOT}"
cargo build --release

echo "→ installation binaire"
install -m 0755 "${REPO_ROOT}/target/release/synergie" "${BIN_PATH}"

echo "→ installation config exemple"
if [ ! -f "${ETC_DIR}/synergie.toml" ]; then
    install -m 0644 "${REPO_ROOT}/examples/synergie.toml" "${ETC_DIR}/synergie.toml"
    echo "   config initiale : ${ETC_DIR}/synergie.toml — à éditer."
else
    echo "   config existante préservée : ${ETC_DIR}/synergie.toml"
fi

echo "→ installation service systemd"
install -m 0644 "${REPO_ROOT}/deploy/systemd/synergie.service" "${SERVICE_PATH}"
systemctl daemon-reload
systemctl enable synergie.service

echo
echo "✅ Installation terminée."
echo
echo "Étapes suivantes :"
echo "  1. Édite ${ETC_DIR}/synergie.toml (roots, telegram, github_proposals_repo)"
echo "  2. (Optionnel) Crée ${ETC_DIR}/synergie.env avec :"
echo "       TELEGRAM_BOT_TOKEN=..."
echo "       TELEGRAM_CHAT_ID=..."
echo "  3. Lance un premier scan :"
echo "       sudo -u ${SYS_USER} synergie --config ${ETC_DIR}/synergie.toml scan"
echo "  4. Démarre le daemon :"
echo "       systemctl start synergie"
echo "       journalctl -u synergie -f"
