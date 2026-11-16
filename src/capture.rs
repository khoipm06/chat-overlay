use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    FullScreen,
    ActiveWindow,
    Region,
}

impl CaptureMode {
    pub fn label(&self) -> &'static str {
        match self {
            CaptureMode::FullScreen => "Full Screen",
            CaptureMode::ActiveWindow => "Active Window",
            CaptureMode::Region => "Select Region",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            CaptureMode::FullScreen => "🖥️",
            CaptureMode::ActiveWindow => "🪟",
            CaptureMode::Region => "✂️",
        }
    }
}

/// Zero-dependency RFC 4648 standard base64 encoder for image payload streaming.
pub fn base64_encode(data: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        let triple = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        result.push(CHARSET[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARSET[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARSET[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARSET[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Checks whether a file path or name has an image extension.
pub fn is_image_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
        || lower.ends_with(".gif")
}

/// Returns the MIME type based on file extension.
pub fn image_mime_type(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/png"
    }
}

fn command_exists(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if dir.join(cmd).is_file() {
                return true;
            }
        }
    }
    false
}

/// Builds `spectacle` CLI flags including live cursor context (`-p`).
pub fn spectacle_args(mode: CaptureMode, out_path: &str) -> Vec<String> {
    let mode_flag = match mode {
        CaptureMode::FullScreen => "-f",
        CaptureMode::ActiveWindow => "-a",
        CaptureMode::Region => "-r",
    };
    vec![
        "-b".to_string(),
        "-n".to_string(),
        "-p".to_string(),
        mode_flag.to_string(),
        "-o".to_string(),
        out_path.to_string(),
    ]
}

/// Reads pixel `(width, height)` from a PNG file's IHDR header without loading the image.
pub fn png_dimensions(path: &std::path::Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header).ok()?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
    let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
    Some((width, height))
}

/// Captures a screenshot using the best available platform tool (KDE spectacle, grim/slurp, or import)
/// and writes it to an ephemeral capture directory.
pub fn capture_screen(mode: CaptureMode) -> Result<PathBuf, String> {
    let capture_dir = std::env::temp_dir().join("pi-overlay-captures");
    if let Err(e) = std::fs::create_dir_all(&capture_dir) {
        return Err(format!("Failed to create capture dir {:?}: {}", capture_dir, e));
    }

    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let out_file = capture_dir.join(format!("screenshot_{}.png", now_ts));
    let out_str = out_file.to_string_lossy().to_string();

    if command_exists("spectacle") {
        let mut cmd = Command::new("spectacle");
        for arg in spectacle_args(mode, &out_str) {
            cmd.arg(arg);
        }

        let status = cmd.status().map_err(|e| format!("Failed to run spectacle: {}", e))?;
        if !status.success() {
            return Err(format!("spectacle exited with status {}", status));
        }
    } else if command_exists("grim") {
        let mut cmd = Command::new("grim");
        if mode == CaptureMode::Region && command_exists("slurp") {
            let slurp_out = Command::new("slurp")
                .output()
                .map_err(|e| format!("Failed to run slurp: {}", e))?;
            let geom = String::from_utf8_lossy(&slurp_out.stdout).trim().to_string();
            if geom.is_empty() {
                return Err("Region selection cancelled".to_string());
            }
            cmd.arg("-g").arg(geom);
        }
        cmd.arg(&out_str);
        let status = cmd.status().map_err(|e| format!("Failed to run grim: {}", e))?;
        if !status.success() {
            return Err(format!("grim exited with status {}", status));
        }
    } else if command_exists("import") {
        // ImageMagick X11 fallback
        let mut cmd = Command::new("import");
        match mode {
            CaptureMode::FullScreen => {
                cmd.arg("-window").arg("root");
            }
            CaptureMode::ActiveWindow => {
                // If xdotool exists, get active window id, else capture root
                if command_exists("xdotool") {
                    if let Ok(out) = Command::new("xdotool").arg("getactivewindow").output() {
                        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if !id.is_empty() {
                            cmd.arg("-window").arg(id);
                        } else {
                            cmd.arg("-window").arg("root");
                        }
                    } else {
                        cmd.arg("-window").arg("root");
                    }
                } else {
                    cmd.arg("-window").arg("root");
                }
            }
            CaptureMode::Region => {
                // Interactive click/drag in ImageMagick
            }
        }
        cmd.arg(&out_str);
        let status = cmd.status().map_err(|e| format!("Failed to run import: {}", e))?;
        if !status.success() {
            return Err(format!("import exited with status {}", status));
        }
    } else {
        return Err("No supported screenshot tool found on system (need spectacle, grim, or import)".to_string());
    }

    if !out_file.exists() || std::fs::metadata(&out_file).map(|m| m.len() == 0).unwrap_or(true) {
        return Err("Screenshot file was not generated or is empty".to_string());
    }

    Ok(out_file)
}

/// Full-screen capture (alias kept for callers that only need the screen).
pub fn capture_full_screen() -> Result<PathBuf, String> {
    capture_screen(CaptureMode::FullScreen)
}

/// Directory screenshots are written to.
pub fn capture_dir() -> PathBuf {
    std::env::temp_dir().join("pi-overlay-captures")
}

/// Delay between hiding the overlay and taking the shot.
///
/// The compositor needs a few frames to actually remove our surface from the
/// scanout, otherwise the capture contains the chat panel itself.
pub const CAPTURE_HIDE_DELAY_MS: u64 = 64;

/// Captures `mode` off the GTK main thread with the overlay temporarily hidden.
pub fn capture_mode_hiding_window<F>(
    window: &gtk4::ApplicationWindow,
    mode: CaptureMode,
    on_done: F,
) where
    F: FnOnce(Result<PathBuf, String>) + 'static,
{
    use gtk4::glib;
    use gtk4::prelude::*;

    let was_visible = window.is_visible();
    window.set_visible(false);

    let win = window.clone();
    glib::spawn_future_local(async move {
        glib::timeout_future(std::time::Duration::from_millis(CAPTURE_HIDE_DELAY_MS)).await;
        let result = tokio::task::spawn_blocking(move || capture_screen(mode))
            .await
            .unwrap_or_else(|e| Err(format!("Capture task failed: {e}")));
        if was_visible {
            win.set_visible(true);
            win.present();
        }
        on_done(result);
    });
}

/// Captures the full screen with the overlay temporarily hidden.
///
/// `on_done` runs on the GTK main thread after the window has been restored, so
/// callers can attach the capture without worrying about ordering.
pub fn capture_screen_hiding_window<F>(window: &gtk4::ApplicationWindow, on_done: F)
where
    F: FnOnce(Result<PathBuf, String>) + 'static,
{
    capture_mode_hiding_window(window, CaptureMode::FullScreen, on_done);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_hide_delay_gives_compositor_a_frame() {
        // At 60 Hz one frame is ~16 ms; the delay must comfortably exceed it.
        let frames_at_60hz = CAPTURE_HIDE_DELAY_MS / 16;
        assert!(frames_at_60hz >= 3, "must allow several frames to repaint");
    }

    #[test]
    fn test_capture_dir_is_writable_location() {
        let dir = capture_dir();
        assert!(dir.to_string_lossy().contains("pi-overlay"));
        std::fs::create_dir_all(&dir).expect("capture dir must be creatable");
        assert!(dir.is_dir());
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_is_image_path() {
        assert!(is_image_path("test.png"));
        assert!(is_image_path("photo.JPG"));
        assert!(is_image_path("/tmp/screenshot_123.webp"));
        assert!(is_image_path("image.BMP"));
        assert!(!is_image_path("code.rs"));
        assert!(!is_image_path("document.txt"));
    }

    #[test]
    fn test_capture_mode_properties() {
        assert_eq!(CaptureMode::FullScreen.label(), "Full Screen");
        assert_eq!(CaptureMode::ActiveWindow.label(), "Active Window");
        assert_eq!(CaptureMode::Region.label(), "Select Region");

        assert_eq!(CaptureMode::FullScreen.icon(), "🖥️");
        assert_eq!(CaptureMode::ActiveWindow.icon(), "🪟");
        assert_eq!(CaptureMode::Region.icon(), "✂️");
    }

    #[test]
    fn test_image_mime_type() {
        assert_eq!(image_mime_type("img.png"), "image/png");
        assert_eq!(image_mime_type("photo.jpeg"), "image/jpeg");
        assert_eq!(image_mime_type("photo.jpg"), "image/jpeg");
        assert_eq!(image_mime_type("pic.webp"), "image/webp");
        assert_eq!(image_mime_type("anim.gif"), "image/gif");
        assert_eq!(image_mime_type("raw.bmp"), "image/bmp");
        assert_eq!(image_mime_type("unknown.bin"), "image/png");
    }

    #[test]
    fn test_base64_encode_binary_patterns() {
        assert_eq!(base64_encode(&[0x00, 0x00, 0x00]), "AAAA");
        assert_eq!(base64_encode(&[0xFF, 0xFF, 0xFF]), "////");
        assert_eq!(base64_encode(&[0x01, 0x02]), "AQI=");
        assert_eq!(base64_encode(&[0xDE, 0xAD, 0xBE, 0xEF]), "3q2+7w==");
    }

    #[test]
    fn test_spectacle_args_include_pointer_and_mode() {
        let full = spectacle_args(CaptureMode::FullScreen, "/tmp/out.png");
        assert!(full.contains(&"-p".to_string()));
        assert!(full.contains(&"-f".to_string()));
        assert_eq!(full.last().map(String::as_str), Some("/tmp/out.png"));

        let win = spectacle_args(CaptureMode::ActiveWindow, "/tmp/w.png");
        assert!(win.contains(&"-a".to_string()));

        let reg = spectacle_args(CaptureMode::Region, "/tmp/r.png");
        assert!(reg.contains(&"-r".to_string()));
    }

    #[test]
    fn test_png_dimensions_parses_ihdr() {
        let mut hdr = Vec::from(b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".as_slice());
        hdr.extend_from_slice(&1920u32.to_be_bytes());
        hdr.extend_from_slice(&1200u32.to_be_bytes());
        let p = capture_dir().join("test_ihdr_dim.png");
        let _ = std::fs::create_dir_all(capture_dir());
        std::fs::write(&p, &hdr).unwrap();
        assert_eq!(png_dimensions(&p), Some((1920, 1200)));
        let _ = std::fs::remove_file(&p);
    }
}
