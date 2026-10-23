# pi-overlay

Universal Linux desktop overlay client for the Pi coding agent.
Runs on Wayland layer-shell compositors and X11 desktops.

## Quick Start

### Build and Run
```bash
cargo build --release
./target/release/pi-overlay
```

### Toggle Shortcut
Default shortcut is `Ctrl+Alt+L`.
Calling `pi-overlay` again while running toggles overlay visibility via local IPC.

## Documentation
- [Installation Guide](docs/installation.md): Binary setup, desktop entry, shell scripts, and system requirements.
- [Configuration](docs/configuration.md): JSON settings, themes, slash commands, and ephemeral mode.
- [Desktop Integration](docs/desktop-integration.md): Keybindings for KDE Plasma, Hyprland, Sway, and GNOME.
- [Architecture & Protocol](docs/architecture.md): Wayland layer-shell lifecycle, NDJSON streaming, and IPC socket.

## License
MIT
