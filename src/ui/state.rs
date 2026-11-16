//! Application state, catalogs, and streaming turn targets for the UI.

use super::widgets::{Disclosure, MarkdownStream};
use crate::backend;
use crate::format;
use crate::mentions;
use crate::settings::AppSettings;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

pub const THINKING_LEVELS: &[(&str, &str)] = &[
    ("off", "Off"),
    ("minimal", "Minimal"),
    ("low", "Low"),
    ("medium", "Balance"),
    ("high", "Deep"),
];

/// Agent modes, matching the ACP mode ids so both harnesses agree.
///
/// `default` is the ACP name; the Pi harness maps it onto its own "agent" mode.
pub const AGENT_MODES: &[(&str, &str, &str)] = &[
    ("default", "Default", "Ask before changing anything"),
    ("accept-edits", "Accept Edits", "Apply file edits without asking"),
    ("auto", "Auto", "Run tools and edits autonomously"),
    ("plan", "Plan", "Research and plan, no changes"),
];

#[cfg(test)]
pub fn model_display_name(model_id: &str) -> String {
    let models = backend::query_pi_models_sync();
    model_display_name_with_list(model_id, &models)
}

pub fn model_display_name_with_list(model_id: &str, available_models: &[(String, String)]) -> String {
    if model_id.is_empty() {
        return "Model".to_string();
    }
    if model_id == "default" {
        return "Auto".to_string();
    }
    for (id, name) in available_models {
        if id == model_id {
            if let Some((_prov, rest)) = name.split_once('/') {
                return rest.to_string();
            }
            return name.clone();
        }
    }
    if let Some((_prov, rest)) = model_id.split_once('/') {
        return rest.to_string();
    }
    model_id.to_string()
}

pub fn thinking_display_name(th_id: &str) -> String {
    for (id, name) in THINKING_LEVELS {
        if *id == th_id {
            return (*name).to_string();
        }
    }
    "Balance".to_string()
}

pub fn mode_display_name(mode_id: &str) -> String {
    for (id, name, _) in AGENT_MODES {
        if *id == mode_id {
            return (*name).to_string();
        }
    }
    if mode_id == "build" {
        return "Build".to_string();
    }
    "Agent".to_string()
}

/// Short label for a workspace path: its final component, or "Workspace".
pub fn workspace_label(dir: &str) -> String {
    let trimmed = dir.trim_end_matches('/');
    match trimmed.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => "Workspace".to_string(),
    }
}

pub struct TrackedPrompt {
    pub index: usize,
    #[allow(dead_code)]
    pub user_card: gtk4::Box,
}

pub struct StepItemEntry {
    pub card: gtk4::Box,
    pub icon: gtk4::Image,
    pub badge: gtk4::Label,
    pub start_time: std::time::Instant,
}

pub struct ProgressDisclosureState {
    pub disclosure: Disclosure,
    pub steps_box: gtk4::Box,
    pub steps_count: usize,
    pub has_error: bool,
    pub start_time: std::time::Instant,
    pub items: HashMap<String, StepItemEntry>,
}

pub struct ThinkingDisclosureState {
    pub disclosure: Disclosure,
    pub text_view: gtk4::TextView,
    pub buffer: gtk4::TextBuffer,
    pub content: String,
    pub start_time: std::time::Instant,
    pub last_elapsed_sec: u64,
}

pub struct ActiveStreamTarget {
    pub turn_body: gtk4::Box,
    pub full_text: Rc<RefCell<String>>,
    pub thinking: Rc<RefCell<Option<ThinkingDisclosureState>>>,
    pub progress: Rc<RefCell<Option<ProgressDisclosureState>>>,
    pub active_markdown: Rc<RefCell<Option<MarkdownStream>>>,
    pub metrics_badge: gtk4::Label,
}

pub fn collapse_and_finalize_thinking(target: &ActiveStreamTarget) {
    if let Some(th) = target.thinking.borrow_mut().take() {
        let elapsed = th.start_time.elapsed().as_secs();
        th.disclosure
            .set_title(&format!("Thought for {}s", elapsed.max(1)));
        th.disclosure.set_expanded(false);
    }
}

pub fn collapse_and_finalize_progress(target: &ActiveStreamTarget) {
    if let Some(p) = target.progress.borrow_mut().take() {
        let total_elapsed = p.start_time.elapsed().as_secs();
        let dur_str = format::format_duration_sec(total_elapsed);
        let lbl = if p.has_error {
            format!("Completed {} steps with errors in {}", p.steps_count, dur_str)
        } else {
            format!("Completed {} steps in {}", p.steps_count, dur_str)
        };
        p.disclosure.set_title(&lbl);
        p.disclosure.set_expanded(false);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    Visible,
    Hidden,
}

pub struct AppState {
    pub mode: OverlayMode,
    pub harness: backend::Harness,
    pub prompts: Vec<TrackedPrompt>,
    pub settings: AppSettings,
    pub current_mode: String,
    pub top_margin: i32,
    pub bottom_margin: i32,
    pub is_window_mode: bool,
    pub attached_files: Vec<(String, String)>,
    pub available_models: Vec<(String, String)>,
    pub available_modes: Vec<(String, String)>,
    pub available_thinking_levels: Vec<String>,
    pub slash_commands: Rc<RefCell<Vec<mentions::SlashCommand>>>,
}

pub struct AppContext {
    #[allow(dead_code)]
    pub window: gtk4::ApplicationWindow,
    #[allow(dead_code)]
    pub state: Rc<RefCell<AppState>>,
    #[allow(dead_code)]
    pub hold_guard: gtk4::gio::ApplicationHoldGuard,
}

/// Merges harness-reported catalogs into app state, validating the selected model.
pub fn apply_session_config(state: &Rc<RefCell<AppState>>, cfg: &backend::SessionConfig) {
    let mut st = state.borrow_mut();

    if !cfg.models.is_empty() {
        st.available_models = cfg.models.clone();
        let current_known = st.available_models.iter().any(|(id, _)| id == &st.settings.model);
        if (st.settings.model.is_empty() || !current_known) && !cfg.current_model.is_empty() {
            let kind = st.settings.harness;
            st.settings.set_model_for_harness(kind, &cfg.current_model);
        } else if st.settings.model.is_empty() {
            if let Some((first_id, _)) = st.available_models.first().cloned() {
                let kind = st.settings.harness;
                st.settings.set_model_for_harness(kind, &first_id);
            }
        }
    } else if st.settings.model.is_empty() && !cfg.current_model.is_empty() {
        let kind = st.settings.harness;
        st.settings.set_model_for_harness(kind, &cfg.current_model);
    }

    if !cfg.modes.is_empty() {
        st.available_modes = cfg.modes.clone();
    }
    if !cfg.thinking_levels.is_empty() {
        st.available_thinking_levels = cfg.thinking_levels.clone();
    }
    if !cfg.commands.is_empty() {
        st.slash_commands
            .borrow_mut()
            .clone_from(&mentions::merge_commands(&cfg.commands));
    }

    if !cfg.current_mode.is_empty() {
        st.current_mode = cfg.current_mode.clone();
        st.settings.acp_mode = cfg.current_mode.clone();
        let auto = st.settings.auto_approve_permissions || cfg.current_mode == "auto";
        st.harness.set_auto_approve(auto);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings;

    #[test]
    fn test_popular_models_list() {
        let models = backend::query_pi_models_sync();
        assert!(!models.is_empty());
        assert_eq!(models[0].0, "default");
    }

    #[test]
    fn test_agent_modes_list() {
        assert!(!AGENT_MODES.is_empty());
        for (id, label, desc) in AGENT_MODES {
            assert!(!id.is_empty());
            assert!(!label.is_empty());
            assert!(!desc.is_empty());
        }
    }

    #[test]
    fn test_model_and_thinking_helpers() {
        assert_eq!(model_display_name("default"), "Auto");
        assert_eq!(thinking_display_name("medium"), "Balance");
        assert_eq!(mode_display_name("agent"), "Agent");
        assert_eq!(mode_display_name("build"), "Build");
    }

    #[test]
    fn test_ui_scale_presets_integration() {
        assert_eq!(settings::UI_SCALE_PRESETS.len(), 4);
        assert_eq!(settings::UI_SCALE_PRESETS[0].scale, 100);
        assert_eq!(settings::UI_SCALE_PRESETS[1].scale, 115);
        assert_eq!(settings::UI_SCALE_PRESETS[2].scale, 130);
        assert_eq!(settings::UI_SCALE_PRESETS[3].scale, 150);
        assert_eq!(settings::FONT_SIZE_PRESETS.len(), 5);
        assert_eq!(settings::FONT_SIZE_PRESETS[1].size, 13);
    }
}
