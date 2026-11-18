# chat-overlay

Cross-platform ACP coding and screen-aware desktop overlay (`Electron` + `TypeScript`), with a legacy `Rust/GTK4` fallback client.

## Supported ACP Harnesses
- **Antigravity (`agy acp`)**
- **OpenCode (`opencode acp`)**
- **Codex (`@agentclientprotocol/codex-acp`)**
- **Claude Code (`@agentclientprotocol/claude-agent-acp`)**
- **Pi (`pi acp`)**

## Quick Start (Universal Electron App)
```bash
cd app
npm install   # or bun install
npm run start
```

### Production Build & Daemon Autostart
```bash
cd app
npm run build:prod
```
- Toggle overlay: `Ctrl+Alt+L` (or `Meta+Shift+L`)
- Searchable Model Picker: `Ctrl+.`
- Clipboard & Selection Companion: `Ctrl+Alt+Q`
- Ghost Click-Through Mode: `Ctrl+Alt+G`
- Desktop Automation Panic Stop: `Ctrl+Alt+X`

## Legacy Rust/GTK4 Binary
For Linux Wayland layer-shell native builds:
```bash
cargo build --release
./target/release/pi-overlay
```

## License
MIT
