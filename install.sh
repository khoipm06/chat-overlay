#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_DIR="${HOME}/.local/bin"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
APP_DIR="${HOME}/.local/share/applications"
AUTOSTART_DIR="${HOME}/.config/autostart"

echo "Building pi-overlay (release)..."
cd "${SCRIPT_DIR}"
LIBRARY_PATH=/var/home/khoipm/.local/lib64 cargo build --release

echo "Stopping any existing pi-overlay instance..."
pkill -x pi-overlay || true
rm -f "${XDG_RUNTIME_DIR:-/tmp}/pi-ephemeral-overlay.sock" || true

echo "Installing binary to ${BIN_DIR}/pi-overlay..."
mkdir -p "${BIN_DIR}"
install -m 755 "${SCRIPT_DIR}/target/release/pi-overlay" "${BIN_DIR}/pi-overlay"

echo "Installing icon to ${ICON_DIR}/pi-overlay.svg..."
mkdir -p "${ICON_DIR}"
install -m 644 "${SCRIPT_DIR}/resources/icons/pi-overlay.svg" "${ICON_DIR}/pi-overlay.svg"

echo "Installing desktop entry to ${APP_DIR}/pi-overlay.desktop..."
mkdir -p "${APP_DIR}"
install -m 644 "${SCRIPT_DIR}/resources/pi-overlay.desktop" "${APP_DIR}/pi-overlay.desktop"

echo "Updating desktop database..."
update-desktop-database "${APP_DIR}" || true

echo "Updating icon cache..."
gtk-update-icon-cache -f "${HOME}/.local/share/icons/hicolor" || true

if [[ "${1:-}" == "--autostart" ]]; then
    echo "Installing autostart entry to ${AUTOSTART_DIR}/pi-overlay.desktop..."
    mkdir -p "${AUTOSTART_DIR}"
    install -m 644 "${SCRIPT_DIR}/resources/pi-overlay.desktop" "${AUTOSTART_DIR}/pi-overlay.desktop"
fi

echo "Installation complete."
echo "Binary: ${BIN_DIR}/pi-overlay"
echo "Launch with: pi-overlay"
echo "Toggle shortcut: Ctrl+Alt+L"
if [[ "${1:-}" != "--autostart" ]]; then
    echo "To enable autostart on login: ./install.sh --autostart"
fi
