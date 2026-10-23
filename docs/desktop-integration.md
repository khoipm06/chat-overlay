# Desktop Integration

## KDE Plasma
`pi-overlay` automatically registers `Ctrl+Alt+L` with `kglobalacceld` on startup.

## Hyprland
Add the following line to `~/.config/hypr/hyprland.conf`:
```ini
bind = CTRL ALT, L, exec, pi-overlay
```

## Sway
Add the following line to `~/.config/sway/config`:
```ini
bindsym Ctrl+Mod1+l exec pi-overlay
```

## GNOME
Configure via GNOME Settings > Keyboard > Custom Shortcuts:
- Name: Pi Overlay
- Command: `pi-overlay`
- Shortcut: `Ctrl+Alt+L`
