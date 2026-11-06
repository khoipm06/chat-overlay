use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_WIDTH: i32 = 480;
pub const MIN_WIDTH: i32 = 320;
pub const MAX_WIDTH: i32 = 1200;

pub const DEFAULT_HEIGHT: i32 = 680;
pub const MIN_HEIGHT: i32 = 360;
pub const MAX_HEIGHT: i32 = 1600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_width")]
    pub width: i32,
    #[serde(default = "default_height")]
    pub height: i32,
    #[serde(default = "default_auto_hide")]
    pub auto_hide_on_blur: bool,
    #[serde(default = "default_thinking_level")]
    pub thinking_level: String,
    #[serde(default = "default_sound_bell")]
    pub sound_bell_on_done: bool,
    #[serde(default = "default_ephemeral")]
    pub ephemeral: bool,
    #[serde(default = "default_margin_right")]
    pub margin_right: i32,
    #[serde(default = "default_margin_top")]
    pub margin_top: i32,
    #[serde(default = "default_show_chat_head")]
    pub show_chat_head: bool,
}

fn default_show_chat_head() -> bool {
    true
}

fn default_margin_right() -> i32 {
    12
}

fn default_margin_top() -> i32 {
    32
}

fn default_model() -> String {
    "default".to_string()
}

fn default_theme() -> String {
    "theme-dark-glass".to_string()
}

fn default_width() -> i32 {
    DEFAULT_WIDTH
}

fn default_height() -> i32 {
    DEFAULT_HEIGHT
}

fn default_auto_hide() -> bool {
    false
}

fn default_thinking_level() -> String {
    "medium".to_string()
}

fn default_sound_bell() -> bool {
    true
}

fn default_ephemeral() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            model: default_model(),
            theme: default_theme(),
            width: default_width(),
            height: default_height(),
            auto_hide_on_blur: default_auto_hide(),
            show_chat_head: default_show_chat_head(),
            thinking_level: default_thinking_level(),
            sound_bell_on_done: default_sound_bell(),
            ephemeral: default_ephemeral(),
            margin_right: default_margin_right(),
            margin_top: default_margin_top(),
        }
    }
}

impl AppSettings {
    pub fn config_path() -> PathBuf {
        let base = match std::env::var("XDG_CONFIG_HOME") {
            Ok(p) => PathBuf::from(p),
            Err(_) => match std::env::var("HOME") {
                Ok(h) => PathBuf::from(h).join(".config"),
                Err(_) => PathBuf::from("/tmp"),
            },
        };
        base.join("pi-overlay").join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                    return settings.clamped();
                }
            }
        }
        let default = Self::default();
        let _ = default.save();
        default
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn clamped(mut self) -> Self {
        self.width = self.width.clamp(MIN_WIDTH, MAX_WIDTH);
        self.height = self.height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        self.margin_right = self.margin_right.clamp(0, 3840);
        self.margin_top = self.margin_top.clamp(0, 2160);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_defaults() {
        let s = AppSettings::default();
        assert_eq!(s.model, "default");
        assert_eq!(s.width, DEFAULT_WIDTH);
        assert_eq!(s.height, DEFAULT_HEIGHT);
        assert!(!s.auto_hide_on_blur);
        assert!(s.sound_bell_on_done);
        assert!(s.ephemeral);
        assert!(s.show_chat_head);
    }

    #[test]
    fn test_settings_clamp() {
        let s = AppSettings {
            width: 100,
            height: 3000,
            ..Default::default()
        };
        let clamped = s.clamped();
        assert_eq!(clamped.width, MIN_WIDTH);
        assert_eq!(clamped.height, MAX_HEIGHT);
    }
}
