# Installation

## End-User Installers (Recommended)

Download the latest build from [GitHub Releases](https://github.com/khoipm06/chat-overlay/releases/latest). No developer runtime (`node`, `bun`, `ffmpeg`, `cmake`, or `brew`) is required.

### Linux
- **AppImage** (recommended across all distros, including immutable desktops):
  ```bash
  chmod +x Chat-Overlay-*.AppImage
  mkdir -p ~/Applications && mv Chat-Overlay-*.AppImage ~/Applications/Chat-Overlay.AppImage
  ~/Applications/Chat-Overlay.AppImage
  ```
  On startup, the AppImage automatically registers its `.desktop` launcher, icon, KDE global shortcuts, and KDE ScreenShot2 D-Bus permission. Launching it again toggles the overlay.
- **Debian / Ubuntu**: `sudo apt install ./chat-overlay_*.deb`
- **Fedora / RHEL**: `sudo dnf install ./chat-overlay-*.rpm`

### Windows
Run `Chat-Overlay-Setup-*.exe`. Both per-user/custom install paths and automatic updates are supported.

### macOS
Open `Chat-Overlay-*-arm64.dmg` (Apple Silicon) or `Chat-Overlay-*-x64.dmg` (Intel) and drag **Chat Overlay** to **Applications**. Grant Screen Recording and Accessibility permissions when prompted for screenshots and optional computer control.

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Alt+L` / `Meta+Shift+L` | Toggle overlay |
| `Ctrl+.` | Open model picker |
| `Ctrl+Alt+Q` | Clipboard companion |
| `Ctrl+Alt+G` | Ghost click-through mode |
| `Ctrl+Alt+X` | Desktop automation panic stop |

---

## Building from Source (Developers)

Requires Node.js 20+ (or Bun) and npm.

```bash
cd app
bun install   # or: npm install
npm run start
```

Linux source install script (adds a launcher at `~/.local/bin/chat-overlay`):

```bash
./install.sh
./install.sh --autostart
```

Uninstall source wrapper:

```bash
./uninstall.sh
```
