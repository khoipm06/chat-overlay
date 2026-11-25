# chat-overlay

Universal cross-platform ACP coding and screen-aware desktop overlay built with Electron + TypeScript.

## Installation (End Users — Zero Prerequisites)

Download the installer for your OS from [GitHub Releases](https://github.com/khoipm06/chat-overlay/releases/latest):

- **Linux**: `Chat-Overlay-*.AppImage` (portable, works on any distro including immutable Fedora Kinoite/Aurora), `.deb`, or `.rpm`
- **Windows**: `Chat-Overlay-Setup-*.exe`
- **macOS**: `Chat-Overlay-*-arm64.dmg` (Apple Silicon) or `Chat-Overlay-*-x64.dmg` (Intel)

On first launch:
- If no ACP coding agent is installed yet, Chat Overlay offers one-click standalone installation of **OpenCode** (no Node.js, Bun, or build tools required).
- When using offline voice dictation for the first time, Chat Overlay bundles the `whisper-server` engine and downloads only your chosen Whisper model.
- Built-in auto-updates keep installed packages current via Settings -> Overlay -> Check for Updates.

See [docs/installation.md](docs/installation.md) for full details.

## Supported ACP Harnesses
- **OpenCode** (`opencode acp`) — standalone one-click installer built in
- **Antigravity** (`agy acp`)
- **Codex** (`@agentclientprotocol/codex-acp`)
- **Claude Code** (`@agentclientprotocol/claude-agent-acp`)
- **Pi** (`pi-acp`)

## Building from Source (Developers)

```bash
git clone https://github.com/khoipm06/chat-overlay.git
cd chat-overlay/app
bun install   # or: npm install
npm run start
```

Or install from source on Linux:

```bash
./install.sh            # build + install launcher + desktop entry
./install.sh --autostart  # also enable autostart on login
```

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Alt+L` / `Meta+Shift+L` | Toggle overlay |
| `Ctrl+.` | Model picker |
| `Ctrl+Alt+Q` | Clipboard companion |
| `Ctrl+Alt+G` | Ghost click-through mode |
| `Ctrl+Alt+X` | Panic stop desktop automation |

## License

MIT
