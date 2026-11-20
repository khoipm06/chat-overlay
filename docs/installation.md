# Installation

## Requirements

- **Node.js** 20+ (or [Bun](https://bun.sh))
- **npm** (bundled with Node.js)
- Linux with Wayland or X11 (macOS / Windows: dev mode only)

## One-step install (Linux)

```bash
git clone https://github.com/khoipm06/chat-overlay.git
cd chat-overlay
./install.sh
```

This will:
1. Build the app (`npm run build:prod`)
2. Install a launcher script to `~/.local/bin/chat-overlay`
3. Register a desktop entry with the KDE/GNOME app launcher
4. Start the background daemon

**To enable autostart on login:**
```bash
./install.sh --autostart
```

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Alt+L` / `Meta+Shift+L` | Toggle overlay |
| `Ctrl+.` | Open model picker |
| `Ctrl+Alt+Q` | Clipboard companion |
| `Ctrl+Alt+G` | Ghost click-through mode |
| `Ctrl+Alt+X` | Desktop automation panic stop |

## Dev mode (any platform)

```bash
cd app
npm install   # or: bun install
npm run start
```

## Uninstall

```bash
./uninstall.sh
```

Removes the launcher, desktop entry, autostart entry, and icon.
