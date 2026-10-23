# Pi Ephemeral Overlay

A sleek, minimalist Wayland desktop overlay for the [Pi coding agent](https://pi.dev). Designed to the standards of Raycast and Linear: zero screen footprint when idle, instantaneous single-keystroke toggle, real-time token streaming, and multi-directional edge resizing.

![Wayland Layer Shell](https://img.shields.io/badge/Wayland-Layer--Shell-blue)
![Rust GTK4](https://img.shields.io/badge/GTK4-Rust-orange)
![License](https://img.shields.io/badge/license-MIT-green)

---

## Key Features

- **True Ephemeral Lifecycle**:
  - Leverages Wayland `wlr-layer-shell` (`zwlr_layer_surface_v1`).
  - When hidden, the Wayland surface is fully unmapped (`set_visible(false)` and `KeyboardMode::None`). Zero resident screen footprint and 100% click-through to windows beneath.
- **Instant Global Toggle (< 2ms)**:
  - Global desktop shortcut `Ctrl+Alt+L` (auto-registered with KDE Plasma / KWin via D-Bus).
  - Fast-path UNIX domain socket IPC listener: running `pi-overlay` toggles the active instance immediately without spawning a secondary process.
- **Live Incremental Token Streaming**:
  - Direct integration with `pi --mode json`.
  - Parses real-time `text_delta` chunks and live `thinking_delta` updates directly into GTK4 buffers without line-buffering delay.
- **Multi-Directional Resizing**:
  - Horizontal drag handle on the left edge (`MIN_WIDTH: 320px`, `MAX_WIDTH: 1200px`).
  - Vertical margin drag handles on top and bottom edges for customizable panel height.
- **Model Selection & Persistent Settings**:
  - Switch models mid-session (`Claude 3.7 Sonnet`, `GPT-4o`, `DeepSeek R1/V3`, `Gemini 2.5 Pro`, local presets).
  - Configurations automatically persist to `$XDG_CONFIG_HOME/pi-overlay/config.json`.
- **Minimalist Dark Palettes (Radix & Tailwind Zinc)**:
  - Dark Glass, OLED Black, Slate Graphite, Forest Charcoal, and Zinc Light.
  - Linear-style flat command blocks and clean full-bleed assistant responses.
- **Engineered for Stability**:
  - Zero zombie process leak: active subprocesses are cleanly killed and reaped asynchronously.
  - GLib custom log handler suppresses upstream GTK4 slider dimension warnings.

---

## Requirements

- **Linux Wayland Compositor** with `wlr-layer-shell` support (KDE Plasma 6, Hyprland, Sway, River, Wayfire, etc.).
- **GTK 4.x** (`libgtk-4-dev`)
- **gtk4-layer-shell** (`libgtk4-layer-shell-dev` or built from source)
- **Rust Toolchain** (1.75+)
- **Pi CLI** (`pi`) installed and available on `$PATH`.

---

## Installation & Building

```bash
# Clone the repository
git clone https://github.com/khoipm06/pi-overlay.git
cd pi-overlay

# Build release binary
cargo build --release

# Run in background
./target/release/pi-overlay &
```

---

## Usage & Keybindings

| Action | Keybinding / Control |
|---|---|
| **Toggle Overlay** | `Ctrl + Alt + L` (System-wide) |
| **Close / Hide** | `Escape` or click outside |
| **Submit Prompt** | `Enter` (or click `Send`) |
| **Clear Conversation** | `Ctrl + L` (or click `Clear`) |
| **Resize Width** | Drag left edge handle (`↔`) |
| **Resize Height** | Drag top or bottom handle (`↕`) |
| **Select Model / Theme** | Click `Settings` on header bar |

---

## Architecture Overview

```text
┌─────────────────────────────────────────────────────────────┐
│                      Wayland Compositor                     │
│               (wlr-layer-shell overlay surface)             │
└──────────────────────────────┬──────────────────────────────┘
                               │
                ┌──────────────▼─────────────┐
                │        pi-overlay GUI      │
                │        (GTK4 / GSK)        │
                ├──────────────┬─────────────┤
                │  settings.rs │  format.rs  │
                └──────┬───────┴──────┬──────┘
                       │              │
        ┌──────────────▼───┐   ┌──────▼─────────────┐
        │    IPC Socket    │   │     Pi Subprocess   │
        │ (~/.config/...)  │   │  (pi --mode json)  │
        └──────────────────┘   └────────────────────┘
```

---

## License

MIT License. See [LICENSE](LICENSE) for details.
