# chat-overlay

Cross-platform ACP coding and screen-aware desktop overlay built with Electron + TypeScript.

## Supported ACP Harnesses
- **Antigravity** (`agy acp`)
- **OpenCode** (`opencode acp`)
- **Codex** (`@agentclientprotocol/codex-acp`)
- **Claude Code** (`@agentclientprotocol/claude-agent-acp`)
- **Pi** (`pi-acp`)

## Install (Linux)

```bash
git clone https://github.com/khoipm06/chat-overlay.git
cd chat-overlay
./install.sh            # build + install launcher + desktop entry
./install.sh --autostart  # also enable autostart on login
```

See [docs/installation.md](docs/installation.md) for full details.

## Dev Mode

```bash
cd app
npm install   # or: bun install
npm run start
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
