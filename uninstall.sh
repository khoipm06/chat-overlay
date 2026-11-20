#!/usr/bin/env bash
set -euo pipefail

BIN_PATH="${HOME}/.local/bin/chat-overlay"
ICON_PATH="${HOME}/.local/share/icons/hicolor/256x256/apps/chat-overlay.png"
APP_PATH="${HOME}/.local/share/applications/chat-overlay.desktop"
AUTOSTART_PATH="${HOME}/.config/autostart/chat-overlay.desktop"
APP_DIR="${HOME}/.local/share/applications"
ICON_BASE="${HOME}/.local/share/icons/hicolor"
SOCK="${XDG_RUNTIME_DIR:-/tmp}/chat-overlay.sock"

echo "Stopping chat-overlay daemon..."
pkill -f "electron.*chat-overlay" 2>/dev/null || true
pkill -f "electron.*dist/main/main.js" 2>/dev/null || true
rm -f "${SOCK}" 2>/dev/null || true

echo "Uninstalling chat-overlay..."

for f in "${BIN_PATH}" "${ICON_PATH}" "${APP_PATH}" "${AUTOSTART_PATH}"; do
  if [[ -f "${f}" ]]; then
    rm -f "${f}"
    echo "  Removed ${f}"
  fi
done

update-desktop-database "${APP_DIR}" 2>/dev/null || true
gtk-update-icon-cache -f "${ICON_BASE}" 2>/dev/null || true

echo ""
echo "✓ Uninstallation complete."
