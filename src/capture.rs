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
        cmd.arg("-b").arg("-n");
        match mode {
            CaptureMode::FullScreen => {
                cmd.arg("-f");
            }
            CaptureMode::ActiveWindow => {
                cmd.arg("-a");
            }
            CaptureMode::Region => {
                cmd.arg("-r");
            }
        }
        cmd.arg("-o").arg(&out_str);

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
