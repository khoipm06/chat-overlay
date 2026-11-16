use gtk4::gdk;
use gtk4::prelude::*;
use std::path::PathBuf;
use std::sync::Once;

static ICONS: &[(&str, &[u8])] = &[
    (
        "codicon-add-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-add-symbolic.svg"),
    ),
    (
        "codicon-arrow-down-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-arrow-down-symbolic.svg"),
    ),
    (
        "codicon-arrow-up-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-arrow-up-symbolic.svg"),
    ),
    (
        "codicon-check-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-check-symbolic.svg"),
    ),
    (
        "codicon-chevron-down-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-chevron-down-symbolic.svg"),
    ),
    (
        "codicon-chevron-right-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-chevron-right-symbolic.svg"),
    ),
    (
        "codicon-close-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-close-symbolic.svg"),
    ),
    (
        "codicon-copy-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-copy-symbolic.svg"),
    ),
    (
        "codicon-debug-stop-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-debug-stop-symbolic.svg"),
    ),
    (
        "codicon-device-camera-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-device-camera-symbolic.svg"),
    ),
    (
        "codicon-edit-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-edit-symbolic.svg"),
    ),
    (
        "codicon-error-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-error-symbolic.svg"),
    ),
    (
        "codicon-eye-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-eye-symbolic.svg"),
    ),
    (
        "codicon-file-code-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-file-code-symbolic.svg"),
    ),
    (
        "codicon-file-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-file-symbolic.svg"),
    ),
    (
        "codicon-folder-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-folder-symbolic.svg"),
    ),
    (
        "codicon-history-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-history-symbolic.svg"),
    ),
    (
        "codicon-refresh-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-refresh-symbolic.svg"),
    ),
    (
        "codicon-screen-full-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-screen-full-symbolic.svg"),
    ),
    (
        "codicon-search-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-search-symbolic.svg"),
    ),
    (
        "codicon-settings-gear-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-settings-gear-symbolic.svg"),
    ),
    (
        "codicon-shield-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-shield-symbolic.svg"),
    ),
    (
        "codicon-sparkle-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-sparkle-symbolic.svg"),
    ),
    (
        "codicon-symbol-namespace-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-symbol-namespace-symbolic.svg"),
    ),
    (
        "codicon-terminal-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-terminal-symbolic.svg"),
    ),
    (
        "codicon-trash-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-trash-symbolic.svg"),
    ),
    (
        "codicon-warning-symbolic.svg",
        include_bytes!("../resources/icons/codicons/codicon-warning-symbolic.svg"),
    ),
];

const INDEX_THEME: &str = "\
[Icon Theme]
Name=Hicolor
Comment=pi-overlay bundled codicons
Directories=scalable/actions

[scalable/actions]
Size=16
MinSize=8
MaxSize=256
Type=Scalable
Context=Actions
";

static INIT: Once = Once::new();

/// Writes bundled Codicon symbolic SVGs to a local icon theme dir and registers
/// it on the active `gtk4::IconTheme` so CSS `color:` recolors them cleanly.
pub fn ensure_registered() {
    INIT.call_once(|| {
        let base = icon_root_dir();
        let hicolor_dir = base.join("hicolor");
        let actions_dir = hicolor_dir.join("scalable").join("actions");
        if std::fs::create_dir_all(&actions_dir).is_ok() {
            let _ = std::fs::write(hicolor_dir.join("index.theme"), INDEX_THEME);
            for (name, bytes) in ICONS {
                let path = actions_dir.join(name);
                if !path.exists() {
                    let _ = std::fs::write(path, bytes);
                }
            }
            if let Some(display) = gdk::Display::default() {
                let theme = gtk4::IconTheme::for_display(&display);
                theme.add_search_path(&base);
            }
        }
    });
}

fn icon_root_dir() -> PathBuf {
    let parent = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    parent.join("pi-overlay-icons")
}

/// Builds a crisp symbolic icon widget scaled to `px`.
pub fn image(icon_name: &str, px: i32) -> gtk4::Image {
    ensure_registered();
    gtk4::Image::builder()
        .icon_name(icon_name)
        .pixel_size(px)
        .build()
}

/// Builds a monochrome icon-only button using a bundled symbolic icon.
pub fn icon_button(
    icon_name: &str,
    px: i32,
    tooltip: &str,
    css_classes: &[&str],
) -> gtk4::Button {
    let img = image(icon_name, px);
    gtk4::Button::builder()
        .child(&img)
        .css_classes(css_classes)
        .tooltip_text(tooltip)
        .build()
}

/// Replaces the child icon inside an icon-only button cleanly.
pub fn set_button_icon(btn: &gtk4::Button, icon_name: &str, px: i32) {
    btn.set_child(Some(&image(icon_name, px)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_icons_embedded() {
        assert_eq!(ICONS.len(), 27);
        for (name, bytes) in ICONS {
            assert!(name.ends_with("-symbolic.svg"));
            let text = std::str::from_utf8(bytes).unwrap();
            assert!(text.contains("<svg") && text.contains("<path"));
        }
    }
}
