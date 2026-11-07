use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_WIDTH: i32 = 480;
pub const MIN_WIDTH: i32 = 320;
pub const MAX_WIDTH: i32 = 1200;

pub const DEFAULT_HEIGHT: i32 = 680;
pub const MIN_HEIGHT: i32 = 360;
pub const MAX_HEIGHT: i32 = 1600;

pub const DEFAULT_UI_SCALE: u32 = 100;
pub const MIN_UI_SCALE: u32 = 80;
pub const MAX_UI_SCALE: u32 = 250;

pub const DEFAULT_FONT_SIZE: u32 = 13;
pub const MIN_FONT_SIZE: u32 = 9;
pub const MAX_FONT_SIZE: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiScalePreset {
    pub scale: u32,
    pub font_size: u32,
    pub label: &'static str,
    pub desc: &'static str,
}

pub const UI_SCALE_PRESETS: &[UiScalePreset] = &[
    UiScalePreset {
        scale: 100,
        font_size: DEFAULT_FONT_SIZE,
        label: "100%",
        desc: "Default (13px)",
    },
    UiScalePreset {
        scale: 115,
        font_size: DEFAULT_FONT_SIZE,
        label: "115%",
        desc: "Medium (~15px)",
    },
    UiScalePreset {
        scale: 130,
        font_size: DEFAULT_FONT_SIZE,
        label: "130%",
        desc: "Large (~17px)",
    },
    UiScalePreset {
        scale: 150,
        font_size: DEFAULT_FONT_SIZE,
        label: "150%",
        desc: "Extra Large (~20px)",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontSizePreset {
    pub size: u32,
    pub label: &'static str,
    pub desc: &'static str,
}

pub const FONT_SIZE_PRESETS: &[FontSizePreset] = &[
    FontSizePreset {
        size: 12,
        label: "12px",
        desc: "Small",
    },
    FontSizePreset {
        size: 13,
        label: "13px",
        desc: "Default",
    },
    FontSizePreset {
        size: 14,
        label: "14px",
        desc: "Medium",
    },
    FontSizePreset {
        size: 15,
        label: "15px",
        desc: "Large",
    },
    FontSizePreset {
        size: 16,
        label: "16px",
        desc: "X-Large",
    },
];

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
    #[serde(default = "default_margin_bottom")]
    pub margin_bottom: i32,
    #[serde(default = "default_show_chat_head")]
    pub show_chat_head: bool,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: u32,
    #[serde(default = "default_font_size")]
    pub font_size: u32,
    #[serde(default)]
    pub dictation_api_url: Option<String>,
    #[serde(default)]
    pub dictation_api_key: Option<String>,
    #[serde(default)]
    pub dictation_model: Option<String>,
    #[serde(default)]
    pub dictation_language: Option<String>,
    #[serde(default)]
    pub dictation_command: Option<String>,
}

fn default_show_chat_head() -> bool {
    true
}

fn default_margin_right() -> i32 {
    24
}

fn default_margin_top() -> i32 {
    24
}

fn default_margin_bottom() -> i32 {
    24
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

fn default_ui_scale() -> u32 {
    DEFAULT_UI_SCALE
}

fn default_font_size() -> u32 {
    DEFAULT_FONT_SIZE
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
            margin_bottom: default_margin_bottom(),
            ui_scale: default_ui_scale(),
            font_size: default_font_size(),
            dictation_api_url: None,
            dictation_api_key: None,
            dictation_model: None,
            dictation_language: None,
            dictation_command: None,
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

    #[allow(dead_code)]
    pub fn scale_factor(&self) -> f64 {
        self.ui_scale as f64 / 100.0
    }

    #[allow(dead_code)]
    pub fn effective_font_size(&self) -> f64 {
        ((self.font_size as f64) * self.scale_factor() * 10.0).round() / 10.0
    }

    pub fn clamped(mut self) -> Self {
        self.width = self.width.clamp(MIN_WIDTH, MAX_WIDTH);
        self.height = self.height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        self.margin_right = self.margin_right.clamp(0, 3840);
        self.margin_top = self.margin_top.clamp(0, 2160);
        self.margin_bottom = self.margin_bottom.clamp(0, 2160);
        self.ui_scale = self.ui_scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE);
        self.font_size = self.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
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
        assert_eq!(s.ui_scale, DEFAULT_UI_SCALE);
        assert_eq!(s.font_size, DEFAULT_FONT_SIZE);
        assert!((s.scale_factor() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_settings_clamp() {
        let s = AppSettings {
            width: 100,
            height: 3000,
            ui_scale: 10,
            font_size: 100,
            ..Default::default()
        };
        let clamped = s.clamped();
        assert_eq!(clamped.width, MIN_WIDTH);
        assert_eq!(clamped.height, MAX_HEIGHT);
        assert_eq!(clamped.ui_scale, MIN_UI_SCALE);
        assert_eq!(clamped.font_size, MAX_FONT_SIZE);
    }

    #[test]
    fn test_scale_presets_consistency() {
        assert!(!UI_SCALE_PRESETS.is_empty());
        for p in UI_SCALE_PRESETS {
            assert!(p.scale >= MIN_UI_SCALE && p.scale <= MAX_UI_SCALE);
            assert!(p.font_size >= MIN_FONT_SIZE && p.font_size <= MAX_FONT_SIZE);
            assert!(!p.label.is_empty());
            assert!(!p.desc.is_empty());
        }
    }

    #[test]
    fn test_font_size_presets_consistency() {
        assert!(!FONT_SIZE_PRESETS.is_empty());
        for p in FONT_SIZE_PRESETS {
            assert!(p.size >= MIN_FONT_SIZE && p.size <= MAX_FONT_SIZE);
            assert!(!p.label.is_empty());
            assert!(!p.desc.is_empty());
        }
    }

    #[test]
    fn test_effective_font_size() {
        let mut s = AppSettings::default();
        assert_eq!(s.effective_font_size(), 13.0);
        s.ui_scale = 150;
        assert_eq!(s.effective_font_size(), 19.5);
        s.font_size = 15;
        assert_eq!(s.effective_font_size(), 22.5);
    }

    #[test]
    fn test_backward_compatibility_deserialize() {
        let old_json = r#"{
            "model": "default",
            "theme": "theme-oled",
            "width": 500,
            "height": 700
        }"#;
        let s: AppSettings = serde_json::from_str(old_json).unwrap();
        assert_eq!(s.ui_scale, DEFAULT_UI_SCALE);
        assert_eq!(s.font_size, DEFAULT_FONT_SIZE);
        assert_eq!(s.width, 500);
        assert_eq!(s.theme, "theme-oled");
        assert!(s.dictation_api_url.is_none());
        assert!(s.dictation_command.is_none());
    }
}
