#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="${SCRIPT_DIR}/app"
BIN_DIR="${HOME}/.local/bin"
ICON_DIR="${HOME}/.local/share/icons/hicolor/256x256/apps"
APP_ENTRY_DIR="${HOME}/.local/share/applications"
AUTOSTART_DIR="${HOME}/.config/autostart"
WRAPPER="${BIN_DIR}/chat-overlay"
SOCK="${XDG_RUNTIME_DIR:-/tmp}/chat-overlay.sock"

# ── Build ────────────────────────────────────────────────────────────────────
echo "Building chat-overlay..."
cd "${APP_DIR}"

if command -v bun > /dev/null 2>&1; then
  bun install --frozen-lockfile
else
  npm ci
fi

npm run build:prod

# ── Wrapper script ───────────────────────────────────────────────────────────
echo "Installing launcher to ${WRAPPER}..."
mkdir -p "${BIN_DIR}"
cat > "${WRAPPER}" << WRAPPER_EOF
#!/usr/bin/env bash
set -euo pipefail
export PATH="\${HOME}/.opencode/bin:\${HOME}/.bun/bin:\${HOME}/.local/bin:\${HOME}/.local/share/mise/shims:/home/linuxbrew/.linuxbrew/bin:/usr/local/bin:/usr/bin:/bin:\${PATH:-}"
SOCK="\${XDG_RUNTIME_DIR:-/tmp}/chat-overlay.sock"
APP_DIR="${APP_DIR}"
ELECTRON_BIN="\${APP_DIR}/node_modules/electron/dist/electron"
MAIN_JS="\${APP_DIR}/dist/main/main.js"

if [[ "\${1:-}" != "--daemon" && -S "\${SOCK}" ]] && command -v socat > /dev/null 2>&1; then
  if printf 'toggle\n' | socat - "UNIX-CONNECT:\${SOCK}" > /dev/null 2>&1; then
    exit 0
  fi
fi

exec "\${ELECTRON_BIN}" "\${MAIN_JS}" "\$@"
WRAPPER_EOF
chmod 755 "${WRAPPER}"

# ── Desktop icon ─────────────────────────────────────────────────────────────
if [[ -f "${APP_DIR}/resources/icons/icon.png" ]]; then
  echo "Installing icon..."
  mkdir -p "${ICON_DIR}"
  install -m 644 "${APP_DIR}/resources/icons/icon.png" "${ICON_DIR}/chat-overlay.png"
  gtk-update-icon-cache -f "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
fi

# ── Desktop entry ────────────────────────────────────────────────────────────
echo "Installing desktop entry to ${APP_ENTRY_DIR}/chat-overlay.desktop..."
mkdir -p "${APP_ENTRY_DIR}"
cat > "${APP_ENTRY_DIR}/chat-overlay.desktop" << DESKTOP_EOF
[Desktop Entry]
Name=Chat Overlay
Comment=Cross-platform ACP coding and screen assistant overlay
Exec=${WRAPPER}
Icon=chat-overlay
Terminal=false
Type=Application
Categories=Development;Utility;
StartupNotify=false
DBusActivatable=false
X-KDE-Shortcuts=Ctrl+Alt+L,Meta+Shift+L
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
DESKTOP_EOF
chmod 644 "${APP_ENTRY_DIR}/chat-overlay.desktop"

update-desktop-database "${APP_ENTRY_DIR}" 2>/dev/null || true

# ── KDE global shortcuts ─────────────────────────────────────────────────────
if command -v kwriteconfig6 > /dev/null 2>&1; then
  echo "Registering KDE global shortcuts (Ctrl+Alt+L / Meta+Shift+L)..."
  kwriteconfig6 --file kglobalshortcutsrc --group services \
    --group chat-overlay.desktop \
    --key _launch "Ctrl+Alt+L\tMeta+Shift+L,Ctrl+Alt+L\tMeta+Shift+L,Chat Overlay" 2>/dev/null || true
fi
if command -v kbuildsycoca6 > /dev/null 2>&1; then
  kbuildsycoca6 --noincremental > /dev/null 2>&1 || true
fi

# ── Autostart ────────────────────────────────────────────────────────────────
if [[ "${1:-}" == "--autostart" ]]; then
  echo "Installing autostart entry to ${AUTOSTART_DIR}/chat-overlay.desktop..."
  mkdir -p "${AUTOSTART_DIR}"
  cat > "${AUTOSTART_DIR}/chat-overlay.desktop" << AUTOSTART_EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=Chat Overlay
Comment=Cross-platform ACP coding and screen assistant overlay
Exec=${WRAPPER} --daemon
Terminal=false
X-GNOME-Autostart-enabled=true
StartupNotify=false
X-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2
AUTOSTART_EOF
  chmod 644 "${AUTOSTART_DIR}/chat-overlay.desktop"
fi

# ── Start daemon ─────────────────────────────────────────────────────────────
if [[ -n "${WAYLAND_DISPLAY:-}" || -n "${DISPLAY:-}" ]]; then
  echo "Restarting daemon..."
  pkill -f "dist/main/main.js" 2>/dev/null || true
  pkill -f "chat-overlay --daemon" 2>/dev/null || true
  sleep 0.8
  rm -f "${SOCK}" 2>/dev/null || true
  if command -v systemd-run > /dev/null 2>&1; then
    systemd-run --user --quiet --collect --description="Chat Overlay Daemon" \
      "${WRAPPER}" --daemon > /dev/null 2>&1 || true
  else
    nohup "${WRAPPER}" --daemon < /dev/null > /dev/null 2>&1 &
  fi
fi

echo ""
echo "✓ Installation complete."
echo "  Launcher: ${WRAPPER}"
echo "  Toggle:   Ctrl+Alt+L  (or Meta+Shift+L)"
if [[ "${1:-}" != "--autostart" ]]; then
  echo ""
  echo "  To enable autostart on login: ./install.sh --autostart"
fi
