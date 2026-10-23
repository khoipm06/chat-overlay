# Pi Ephemeral Overlay

A sleek, universal desktop overlay for the [Pi coding agent](https://pi.dev). Designed to the standards of Raycast, Linear, and Open-WebUI: zero screen footprint when idle, instantaneous single-keystroke toggle, real-time token streaming, rich Markdown rendering with syntax highlighting, and universal Wayland & X11 desktop support.

![Wayland & X11](https://img.shields.io/badge/Platform-Wayland%20%2F%20X11-blue)
![Rust GTK4](https://img.shields.io/badge/GTK4-Rust-orange)
![License](https://img.shields.io/badge/license-MIT-green)

---

## ⚡ Quick Start (TL;DR)

```bash
# 1. Clone repository
git clone https://github.com/khoipm06/pi-overlay.git
cd pi-overlay

# 2. Build release binary
cargo build --release

# 3. Launch background daemon
./target/release/pi-overlay &
```

> **Instant Toggle**: Press `Ctrl + Alt + L` anywhere on your system to summon or dismiss the overlay instantly (< 2ms).

---

## 🌟 Key Features

- **Universal Linux Compatibility**:
  - **Wayland Layer Shell**: Native integration with `zwlr_layer_surface_v1` on KDE Plasma 6, Hyprland, Sway, River, Wayfire.
  - **X11 & Non-Layer-Shell Fallback**: Automatic runtime detection via `gtk4_layer_shell::is_supported()`. Gracefully falls back to top-level borderless window on X11, GNOME Mutter, or macOS.
- **Open-WebUI Style UX**:
  - **Collapsible Thought Process**: Auto-expands while reasoning (`thinking_delta`), and neatly collapses on completion so output stays readable.
  - **Live Tool Execution Feed**: Displays active shell/file tools in real time (e.g. `[tool: bash] {"command": "cargo check"}`).
  - **Token & Latency Metrics**: Real-time stats badge reporting total tokens and duration (e.g. `21,499 tok • 3.1s`).
- **Rich Markdown & Syntax Highlighting Engine**:
  - Full support for the Rendering Capability Test Suite:
    - **Tables**: Auto-aligned Unicode Box-Drawing (`┌─┬─┐`, `├─┼─┤`, `└─┴─┘`).
    - **Syntax Highlighting**: Rust, Python, TypeScript, Shell, JSON, SQL with Raycast/Linear color tokens.
    - **Typography & Math**: `**bold**`, `*italic*`, `~~del~~`, Subscript (`H~2~O`), Superscript (`E = mc^2^`), Inline Math (`$e^{i\pi} + 1 = 0$`), and `<kbd>` shortcuts.
    - **Callouts & Alerts**: GitHub alerts (`[!NOTE]`, `[!WARNING]`, `[!TIP]`) and nested quotes (`│ │`).
    - **Interactive Tasklists**: Checked `☑` and unchecked `☐` task states.
- **Multi-Directional Resizing**:
  - Drag the left edge handle (`↔`) to adjust width (`320px` to `1200px`).
  - Drag the top or bottom handles (`↕`) to adjust vertical margins and panel height dynamically.
- **Slash Commands & Prompt Templates**:
  - Type `/explain`, `/refactor`, `/test`, `/fix`, `/review`, or `/doc` followed by your prompt to immediately expand standard prompts.
- **Model Selection & Persistent Settings**:
  - Switch models on the fly (`Claude 3.7 Sonnet`, `GPT-4o`, `DeepSeek R1/V3`, `Gemini 2.5 Pro`).
  - Settings automatically persist to `$XDG_CONFIG_HOME/pi-overlay/config.json`.
- **Audio Completion Bell**:
  - Optional auditory chime via system sound bell upon response completion.

---

## 🛠️ Configuration & Hidden Features

Configuration is stored in `$XDG_CONFIG_HOME/pi-overlay/config.json` (defaults to `~/.config/pi-overlay/config.json`):

```json
{
  "model": "claude-3-7-sonnet",
  "theme": "theme-dark-glass",
  "width": 520,
  "height": 720,
  "auto_hide_on_blur": true,
  "thinking_level": "medium",
  "sound_bell_on_done": true
}
```

### Available Themes
- `theme-dark-glass`: Translucent dark glass based on Radix Mauve.
- `theme-oled`: Deep OLED black (`#000000`).
- `theme-slate`: Muted slate graphite based on Radix Slate.
- `theme-forest`: Obsidian and subtle emerald green.
- `theme-zinc-light`: High-contrast minimal bright zinc.

### Slash Commands Reference
- `/explain <code/concept>`: Request clear, step-by-step breakdown.
- `/refactor <code>`: Optimize for performance, readability, and clean architecture.
- `/test <code>`: Generate comprehensive unit tests with edge cases.
- `/fix <code>`: Analyze and produce minimal diff fixes.
- `/review <code>`: Perform senior security and architectural review.
- `/doc <code>`: Generate clean documentation and specifications.

---

## ⌨️ Desktop Integration & Keybindings

### KDE Plasma 6
The shortcut `Ctrl+Alt+L` is automatically bound to `kglobalacceld` on launch.

### Hyprland (`~/.config/hypr/hyprland.conf`)
```ini
bind = CTRL ALT, L, exec, /path/to/pi-overlay
```

### Sway / i3 (`~/.config/sway/config`)
```ini
bindsym Ctrl+Mod1+l exec /path/to/pi-overlay
```

---

## 🏗️ Architecture

```text
┌─────────────────────────────────────────────────────────────┐
│                    Display Compositor                       │
│       (wlr-layer-shell on Wayland, X11 Window fallback)     │
└──────────────────────────────┬──────────────────────────────┘
                               │
                ┌──────────────▼─────────────┐
                │        pi-overlay GUI      │
                │        (GTK4 / GSK)        │
                ├──────────────┬─────────────┤
                │  settings.rs │  render/    │
                │  commands.rs │  overlay.rs │
                └──────┬───────┴──────┬──────┘
                       │              │
        ┌──────────────▼───┐   ┌──────▼─────────────┐
        │    IPC Socket    │   │     Pi Subprocess   │
        │ (~/.config/...)  │   │  (pi --mode json)  │
        └──────────────────┘   └────────────────────┘
```

---

## 📄 License

MIT License. See [LICENSE](LICENSE) for details.
