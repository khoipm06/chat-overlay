//! Desktop entry and icon installation helpers for `pi-overlay`.

/// Installs or upgrades `~/.local/share/applications/pi-overlay.desktop` and the SVG icon.
pub fn ensure_desktop_integration() {
    let home = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => return,
    };

    let desktop_dir = home.join(".local/share/applications");
    let icon_dir = home.join(".local/share/icons/hicolor/scalable/apps");
    let target_desktop = desktop_dir.join("pi-overlay.desktop");
    let target_icon = icon_dir.join("pi-overlay.svg");

    let installed_bin = home.join(".local/bin/pi-overlay");
    let exec_path = if installed_bin.exists() {
        installed_bin
    } else if let Ok(exe) = std::env::current_exe() {
        exe
    } else {
        std::path::PathBuf::from("pi-overlay")
    };
    let exec_str = exec_path.to_string_lossy();
    let exec_line = if exec_str.contains(' ') {
        format!("Exec=\"{}\"", exec_str)
    } else {
        format!("Exec={}", exec_str)
    };

    if !target_desktop.exists() {
        let _ = std::fs::create_dir_all(&desktop_dir);
        let content = include_str!("../resources/pi-overlay.desktop")
            .replace("Exec=pi-overlay", &exec_line);
        let _ = std::fs::write(&target_desktop, content);
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&desktop_dir)
            .status();
    } else if exec_path.is_absolute() {
        if let Ok(existing) = std::fs::read_to_string(&target_desktop) {
            let needs_exec = existing.lines().any(|l| l.trim() == "Exec=pi-overlay");
            let needs_shortcut = !existing.contains("X-KDE-Shortcuts=");
            if needs_exec || needs_shortcut {
                let updated = include_str!("../resources/pi-overlay.desktop")
                    .replace("Exec=pi-overlay", &exec_line);
                let _ = std::fs::write(&target_desktop, updated);
                let _ = std::process::Command::new("update-desktop-database")
                    .arg(&desktop_dir)
                    .status();
            }
        }
    }

    if !target_icon.exists() {
        let _ = std::fs::create_dir_all(&icon_dir);
        let svg = include_str!("../resources/icons/pi-overlay.svg");
        let _ = std::fs::write(&target_icon, svg);
        let _ = std::process::Command::new("gtk-update-icon-cache")
            .args([
                "-f",
                home.join(".local/share/icons/hicolor").to_str().unwrap_or(""),
            ])
            .status();
    }
}
