#!/usr/bin/env bash
set -euo pipefail

BIN_PATH="${HOME}/.local/bin/pi-overlay"
ICON_PATH="${HOME}/.local/share/icons/hicolor/scalable/apps/pi-overlay.svg"
APP_PATH="${HOME}/.local/share/applications/pi-overlay.desktop"
AUTOSTART_PATH="${HOME}/.config/autostart/pi-overlay.desktop"
APP_DIR="${HOME}/.local/share/applications"
ICON_BASE="${HOME}/.local/share/icons/hicolor"

echo "Uninstalling pi-overlay..."

if [[ -f "${BIN_PATH}" ]]; then
    rm -f "${BIN_PATH}"
    echo "Removed ${BIN_PATH}"
fi

if [[ -f "${ICON_PATH}" ]]; then
    rm -f "${ICON_PATH}"
    echo "Removed ${ICON_PATH}"
fi

if [[ -f "${APP_PATH}" ]]; then
    rm -f "${APP_PATH}"
    echo "Removed ${APP_PATH}"
fi

if [[ -f "${AUTOSTART_PATH}" ]]; then
    rm -f "${AUTOSTART_PATH}"
    echo "Removed ${AUTOSTART_PATH}"
fi

echo "Updating desktop database..."
update-desktop-database "${APP_DIR}" || true

echo "Updating icon cache..."
gtk-update-icon-cache -f "${ICON_BASE}" || true

echo "Uninstallation complete."
