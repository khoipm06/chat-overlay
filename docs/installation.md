# Installation Guide

## Requirements
- Rust toolchain (stable)
- GTK4 development headers (`gtk4`)
- GTK4 Layer Shell library (`gtk4-layer-shell`)
- Pi coding agent CLI (`pi`) available in system PATH

### Fedora / RHEL
```bash
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

### Arch Linux
```bash
sudo pacman -S gtk4 gtk4-layer-shell
```

### Debian / Ubuntu
```bash
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev
```

## Quick Installation
Run the one-step install script:
```bash
./install.sh
```

To automatically start `pi-overlay` on desktop login:
```bash
./install.sh --autostart
```

## Uninstallation
To remove `pi-overlay`, its icon, desktop entry, and autostart config:
```bash
./uninstall.sh
```

## Manual Installation
```bash
cargo build --release
install -Dm755 target/release/pi-overlay ~/.local/bin/pi-overlay
```
