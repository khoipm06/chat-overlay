pub mod acp;
mod backend;
mod commands;
mod format;
mod ipc;
mod overlay;
mod render;
mod settings;
mod tray;

use backend::{PiRunner, StreamEvent};
use gtk4::gdk;
use gtk4::glib::{self, LogWriterOutput};
use gtk4::prelude::*;
use settings::AppSettings;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::mpsc;

const APP_ID: &str = "com.github.khoipm.pi-overlay";

pub const AVAILABLE_THEMES: &[(&str, &str, &str)] = &[
    ("theme-dark-glass", "Dark Glass", "Default modern dark canvas"),
    ("theme-oled", "OLED Black", "Pure deep black"),
    ("theme-slate", "Slate Graphite", "Deep slate & graphite"),
    ("theme-tokyo-night", "Tokyo Night", "Deep blue-black night"),
    ("theme-nord", "Nord Arctic", "Arctic cool bluish palette"),
    ("theme-catppuccin-mocha", "Catppuccin Mocha", "Soothing dark pastel"),
];
const THEMES: &[(&str, &str, &str)] = AVAILABLE_THEMES;

pub const POPULAR_MODELS: &[(&str, &str)] = &[
    ("default", "Default (System Preset)"),
    ("claude-3-7-sonnet", "Claude 3.7 Sonnet"),
    ("claude-3-5-sonnet", "Claude 3.5 Sonnet"),
    ("claude-3-5-haiku", "Claude 3.5 Haiku"),
    ("gpt-4o", "GPT-4o"),
    ("gpt-4o-mini", "GPT-4o Mini"),
    ("gemini-2.5-flash", "Gemini 2.5 Flash"),
    ("gemini-2.5-pro", "Gemini 2.5 Pro"),
    ("deepseek/deepseek-chat", "DeepSeek Chat (V3)"),
    ("deepseek/deepseek-reasoner", "DeepSeek Reasoner (R1)"),
];

pub const THINKING_LEVELS: &[(&str, &str)] = &[
    ("off", "Off (No reasoning)"),
    ("low", "Low"),
    ("medium", "Medium (Recommended)"),
    ("high", "High (Deep reasoning)"),
];

struct TrackedPrompt {
    index: usize,
}

type ActiveThoughtState = Option<(
    gtk4::Expander,
    gtk4::Label,
    Rc<RefCell<String>>,
    std::time::Instant,
)>;

struct ToolCardEntry {
    card: gtk4::Box,
    badge: gtk4::Label,
    start_time: std::time::Instant,
}

struct ActiveStreamTarget {
    turn_body: gtk4::Box,
    full_text: Rc<RefCell<String>>,
    active_thought_expander: Rc<RefCell<ActiveThoughtState>>,
    active_markdown: Rc<RefCell<Option<(gtk4::Box, String)>>>,
    tools: Rc<RefCell<HashMap<String, ToolCardEntry>>>,
    metrics_badge: gtk4::Label,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    Visible,
    Hidden,
}

struct AppState {
    mode: OverlayMode,
    runner: PiRunner,
    prompts: Vec<TrackedPrompt>,
    settings: AppSettings,
    top_margin: i32,
    bottom_margin: i32,
    is_window_mode: bool,
}

struct AppContext {
    #[allow(dead_code)]
    window: gtk4::ApplicationWindow,
    #[allow(dead_code)]
    state: Rc<RefCell<AppState>>,
    #[allow(dead_code)]
    _hold_guard: gtk4::gio::ApplicationHoldGuard,
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // Augment PATH with standard user toolchain directories
    let current_path = std::env::var("PATH").unwrap_or_default();
    let augmented = if let Ok(home) = std::env::var("HOME") {
        format!(
            "{}/.local/share/mise/installs/node/lts/bin:{}/.local/share/mise/shims:{}/.bun/bin:{}/.local/bin:{}/.cargo/bin:/home/linuxbrew/.linuxbrew/bin:/usr/local/bin:{}",
            home, home, home, home, home, current_path
        )
    } else {
        format!("/home/linuxbrew/.linuxbrew/bin:/usr/local/bin:{}", current_path)
    };
    std::env::set_var("PATH", augmented);

    // Fast path: if CLI args contain a prompt, send via IPC to running instance
    if args.len() > 1 && !args[1].starts_with('-') {
        let prompt_text = args[1..].join(" ");
        if ipc::try_send_command(&format!("PROMPT:{}", prompt_text)) {
            return glib::ExitCode::SUCCESS;
        }
    } else if let Some(pos) = args.iter().position(|a| a == "--prompt" || a == "-p") {
        if pos + 1 < args.len() {
            let prompt_text = args[pos + 1..].join(" ");
            if ipc::try_send_command(&format!("PROMPT:{}", prompt_text)) {
                return glib::ExitCode::SUCCESS;
            }
        }
    }

    if args.contains(&"--toggle".to_string()) {
        if ipc::try_send_toggle() {
            return glib::ExitCode::SUCCESS;
        }
        eprintln!("[pi-overlay] Overlay is not running.");
        return glib::ExitCode::FAILURE;
    }

    // Fast path: if already running, toggle visibility and exit immediately
    if ipc::try_send_toggle() {
        return glib::ExitCode::SUCCESS;
    }

    // Suppress benign GtkGizmo slider sizing warnings
    glib::log_set_writer_func(|_level, fields| {
        for field in fields {
            if let Some(val) = field.value_str() {
                if val.contains("GtkGizmo")
                    || (val.contains("slider")
                        && (val.contains("min width") || val.contains("min height")))
                {
                    return LogWriterOutput::Handled;
                }
            }
        }
        LogWriterOutput::Unhandled
    });

    // Auto-register desktop file and icons if missing
    ensure_desktop_integration();

    let is_window_mode = args.contains(&"--window".to_string());
    let is_daemon = args.contains(&"--daemon".to_string()) || args.contains(&"--hidden".to_string());

    // Initialize Tokio runtime so background workers and IPC listener can call tokio::spawn
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    let _rt_guard = rt.enter();

    let app = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let app_ctx: Rc<RefCell<Option<AppContext>>> = Rc::new(RefCell::new(None));

    let app_ctx_clone = app_ctx.clone();
    app.connect_activate(move |application| {
        if app_ctx_clone.borrow().is_none() {
            load_css();
            let ctx = build_ui(application, is_window_mode, is_daemon);
            *app_ctx_clone.borrow_mut() = Some(ctx);
        }
    });

    app.run_with_args(&Vec::<String>::new())
}

fn ensure_desktop_integration() {
    let home = match std::env::var("HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => return,
    };

    let desktop_dir = home.join(".local/share/applications");
    let icon_dir = home.join(".local/share/icons/hicolor/scalable/apps");
    let target_desktop = desktop_dir.join("pi-overlay.desktop");
    let target_icon = icon_dir.join("pi-overlay.svg");

    if !target_desktop.exists() {
        let _ = std::fs::create_dir_all(&desktop_dir);
        let content = include_str!("../resources/pi-overlay.desktop");
        let _ = std::fs::write(&target_desktop, content);
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&desktop_dir)
            .status();
    }

    if !target_icon.exists() {
        let _ = std::fs::create_dir_all(&icon_dir);
        let svg = include_str!("../resources/icons/pi-overlay.svg");
        let _ = std::fs::write(&target_icon, svg);
        let _ = std::process::Command::new("gtk-update-icon-cache")
            .args(["-f", home.join(".local/share/icons/hicolor").to_str().unwrap_or("")])
            .status();
    }
}

fn load_css() {
    let provider = gtk4::CssProvider::new();
    let css_bytes = include_bytes!("../resources/style.css");
    provider.load_from_data(std::str::from_utf8(css_bytes).unwrap_or(""));

    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );
    }
}

fn build_ui(app: &gtk4::Application, is_window_mode: bool, is_daemon: bool) -> AppContext {
    // Hold reference so daemon does not terminate when overlay window is hidden
    let hold_guard = app.hold();

    let initial_settings = AppSettings::load();

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("Agent Canvas")
        .default_width(initial_settings.width)
        .build();

    overlay::init_layer_shell(&window, !is_window_mode);
    if is_window_mode {
        window.set_decorated(true);
    }

    // Floating Chat Head Window
    let chat_head_window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("Chat Head")
        .css_classes(["chat-head-window"])
        .build();

    overlay::init_chat_head_layer_shell(
        &chat_head_window,
        initial_settings.margin_top,
        initial_settings.margin_right,
    );
    chat_head_window.set_visible(!is_window_mode && initial_settings.show_chat_head);

    let head_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    head_box.add_css_class("chat-head-box");

    let head_btn = gtk4::Button::builder()
        .css_classes(["chat-head-btn"])
        .tooltip_text("Toggle Agent Canvas (Ctrl+Alt+L)")
        .build();

    let head_inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    head_inner.set_halign(gtk4::Align::Center);
    head_inner.set_valign(gtk4::Align::Center);

    let head_icon = gtk4::Label::builder()
        .label("✦")
        .css_classes(["chat-head-icon"])
        .build();
    head_inner.append(&head_icon);

    let head_dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    head_dot.add_css_class("chat-head-dot");
    head_inner.append(&head_dot);

    head_btn.set_child(Some(&head_inner));
    head_box.append(&head_btn);
    chat_head_window.set_child(Some(&head_box));

    // Root overlay container
    let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root_box.set_halign(gtk4::Align::End);
    root_box.set_vexpand(true);
    root_box.set_hexpand(false);

    // Left Resize Grip
    let left_col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    left_col.set_width_request(10);
    left_col.set_vexpand(true);

    let resize_handle_w = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    resize_handle_w.add_css_class("resize-handle");
    resize_handle_w.set_cursor_from_name(Some("ew-resize"));
    resize_handle_w.set_vexpand(true);

    let resize_grip_w = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    resize_grip_w.add_css_class("resize-grip");
    resize_grip_w.set_valign(gtk4::Align::Center);
    resize_grip_w.set_vexpand(true);
    resize_handle_w.append(&resize_grip_w);
    left_col.append(&resize_handle_w);
    root_box.append(&left_col);

    // Main Drawer Card Container
    let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    drawer.set_widget_name("drawer-container");
    drawer.add_css_class("drawer-container");
    drawer.add_css_class(&initial_settings.theme);
    drawer.set_width_request(initial_settings.width);
    drawer.set_vexpand(true);
    drawer.set_hexpand(false);

    // ── Header Bar ──────────────────────────────────────────────────────────
    let header_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header_bar.add_css_class("header-bar");
    header_bar.set_hexpand(false);

    // Status Dot
    let status_dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    status_dot.add_css_class("status-dot");
    status_dot.set_valign(gtk4::Align::Center);
    header_bar.append(&status_dot);

    // App Title
    let title_label = gtk4::Label::builder()
        .label("Agent Canvas")
        .css_classes(["app-title"])
        .halign(gtk4::Align::Start)
        .build();
    header_bar.append(&title_label);

    // Model Pill Button (opens settings)
    let model_pill = gtk4::Button::builder()
        .label(format!("{} ▾", initial_settings.model))
        .css_classes(["model-pill-btn"])
        .tooltip_text("Active Model (Click to change)")
        .build();
    header_bar.append(&model_pill);

    let header_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    header_spacer.set_hexpand(true);
    header_bar.append(&header_spacer);

    // ── Session Flyout Button & Popover ─────────────────────────────────────
    let btn_session = gtk4::Button::builder()
        .label("📋 Session (0)")
        .css_classes(["header-btn"])
        .tooltip_text("Session Prompts")
        .build();

    let prompts_popover = gtk4::Popover::new();
    prompts_popover.set_parent(&btn_session);
    prompts_popover.add_css_class("flyout-popover");

    let prompts_pop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    prompts_pop_box.set_width_request(280);

    let prompts_pop_title = gtk4::Label::builder()
        .label("Session History")
        .css_classes(["flyout-header-title"])
        .xalign(0.0)
        .build();
    prompts_pop_box.append(&prompts_pop_title);

    let prompts_pop_sub = gtk4::Label::builder()
        .label("Click any prompt to jump to it in chat")
        .css_classes(["flyout-header-sub"])
        .xalign(0.0)
        .build();
    prompts_pop_box.append(&prompts_pop_sub);

    let prompts_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .max_content_height(280)
        .propagate_natural_height(true)
        .build();

    let prompts_list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let prompt_empty_label = gtk4::Label::builder()
        .label("No prompts in this session yet.")
        .css_classes(["prompt-snippet-text"])
        .margin_top(16)
        .margin_bottom(16)
        .xalign(0.5)
        .build();
    prompts_list_box.append(&prompt_empty_label);
    prompts_scroll.set_child(Some(&prompts_list_box));
    prompts_pop_box.append(&prompts_scroll);
    prompts_popover.set_child(Some(&prompts_pop_box));

    let prompts_pop_toggle = prompts_popover.clone();
    btn_session.connect_clicked(move |_| {
        prompts_pop_toggle.popup();
    });
    header_bar.append(&btn_session);

    // ── Settings Flyout Button & Popover ────────────────────────────────────
    let btn_settings = gtk4::Button::builder()
        .label("⚙ Settings")
        .css_classes(["header-btn"])
        .tooltip_text("Preferences")
        .build();

    let settings_popover = gtk4::Popover::new();
    settings_popover.set_parent(&btn_settings);
    settings_popover.add_css_class("flyout-popover");

    let settings_pop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    settings_pop_box.set_width_request(260);

    let settings_pop_title = gtk4::Label::builder()
        .label("Preferences")
        .css_classes(["flyout-header-title"])
        .xalign(0.0)
        .build();
    settings_pop_box.append(&settings_pop_title);

    // Model Selector
    let model_lbl = gtk4::Label::builder()
        .label("MODEL")
        .css_classes(["flyout-section-title"])
        .xalign(0.0)
        .build();
    settings_pop_box.append(&model_lbl);

    let model_dropdown = gtk4::DropDown::from_strings(
        &POPULAR_MODELS.iter().map(|(_, n)| *n).collect::<Vec<&str>>(),
    );
    model_dropdown.add_css_class("settings-dropdown");
    let cur_model_idx = POPULAR_MODELS
        .iter()
        .position(|(id, _)| *id == initial_settings.model)
        .unwrap_or(0);
    model_dropdown.set_selected(cur_model_idx as u32);
    settings_pop_box.append(&model_dropdown);

    // Thinking / Reasoning Depth
    let think_lbl = gtk4::Label::builder()
        .label("REASONING DEPTH")
        .css_classes(["flyout-section-title"])
        .xalign(0.0)
        .build();
    settings_pop_box.append(&think_lbl);

    let think_dropdown = gtk4::DropDown::from_strings(
        &THINKING_LEVELS.iter().map(|(_, n)| *n).collect::<Vec<&str>>(),
    );
    think_dropdown.add_css_class("settings-dropdown");
    let cur_think_idx = THINKING_LEVELS
        .iter()
        .position(|(id, _)| *id == initial_settings.thinking_level)
        .unwrap_or(2);
    think_dropdown.set_selected(cur_think_idx as u32);
    settings_pop_box.append(&think_dropdown);

    // Theme Selector
    let theme_lbl = gtk4::Label::builder()
        .label("APPEARANCE THEME")
        .css_classes(["flyout-section-title"])
        .xalign(0.0)
        .build();
    settings_pop_box.append(&theme_lbl);

    let theme_dropdown = gtk4::DropDown::from_strings(
        &THEMES.iter().map(|(_, n, _)| *n).collect::<Vec<&str>>(),
    );
    theme_dropdown.add_css_class("settings-dropdown");
    let cur_theme_idx = THEMES
        .iter()
        .position(|(id, _, _)| *id == initial_settings.theme)
        .unwrap_or(0);
    theme_dropdown.set_selected(cur_theme_idx as u32);
    settings_pop_box.append(&theme_dropdown);

    // Window Behavior Toggles
    let behavior_lbl = gtk4::Label::builder()
        .label("BEHAVIOR")
        .css_classes(["flyout-section-title"])
        .xalign(0.0)
        .build();
    settings_pop_box.append(&behavior_lbl);

    let pin_check = gtk4::CheckButton::builder()
        .label("Pin overlay (Keep open on blur)")
        .active(!initial_settings.auto_hide_on_blur)
        .css_classes(["settings-check"])
        .build();
    settings_pop_box.append(&pin_check);

    let chathead_check = gtk4::CheckButton::builder()
        .label("Floating chat head bubble")
        .active(initial_settings.show_chat_head)
        .css_classes(["settings-check"])
        .build();
    settings_pop_box.append(&chathead_check);

    let ephemeral_check = gtk4::CheckButton::builder()
        .label("Clear session on hide (Ephemeral)")
        .active(initial_settings.ephemeral)
        .css_classes(["settings-check"])
        .build();
    settings_pop_box.append(&ephemeral_check);

    settings_popover.set_child(Some(&settings_pop_box));
    let settings_pop_toggle = settings_popover.clone();
    btn_settings.connect_clicked(move |_| {
        settings_pop_toggle.popup();
    });
    header_bar.append(&btn_settings);

    // Model pill button connects to settings popover
    let settings_pop_ref = settings_popover.clone();
    model_pill.connect_clicked(move |_| {
        settings_pop_ref.popup();
    });

    // Clear Button
    let btn_clear = gtk4::Button::builder()
        .label("🗑 Clear")
        .css_classes(["header-btn"])
        .tooltip_text("Clear conversation (Ctrl+L)")
        .build();
    header_bar.append(&btn_clear);

    // Close Button
    let btn_close = gtk4::Button::builder()
        .label("✕")
        .css_classes(["header-btn", "close"])
        .tooltip_text("Close overlay (Esc)")
        .build();
    header_bar.append(&btn_close);

    drawer.append(&header_bar);

    // ── Chat Messages Scrollable Area ───────────────────────────────────────
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .propagate_natural_width(false)
        .propagate_natural_height(false)
        .css_classes(["chat-scroll"])
        .build();

    let messages_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    messages_box.add_css_class("messages-container");
    messages_box.set_vexpand(true);
    messages_box.set_valign(gtk4::Align::Start);

    // Welcome Hero
    let welcome_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    welcome_box.add_css_class("welcome-hero");
    welcome_box.set_halign(gtk4::Align::Center);
    welcome_box.set_valign(gtk4::Align::Center);
    welcome_box.set_vexpand(true);

    let glyph = gtk4::Label::builder()
        .label("✦")
        .css_classes(["welcome-glyph"])
        .build();
    welcome_box.append(&glyph);

    let w_title = gtk4::Label::builder()
        .label("Agent Canvas")
        .css_classes(["welcome-title"])
        .build();
    welcome_box.append(&w_title);

    let w_sub = gtk4::Label::builder()
        .label("Ask questions, inspect files, or execute terminal workflows")
        .css_classes(["welcome-subtitle"])
        .build();
    welcome_box.append(&w_sub);

    let pill1 = gtk4::Button::builder()
        .label("Explain workspace architecture")
        .css_classes(["welcome-pill-btn"])
        .build();
    let pill2 = gtk4::Button::builder()
        .label("Run tests and inspect errors")
        .css_classes(["welcome-pill-btn"])
        .build();
    let pill3 = gtk4::Button::builder()
        .label("Review recent git changes")
        .css_classes(["welcome-pill-btn"])
        .build();

    welcome_box.append(&pill1);
    welcome_box.append(&pill2);
    welcome_box.append(&pill3);
    messages_box.append(&welcome_box);
    scrolled.set_child(Some(&messages_box));
    drawer.append(&scrolled);

    // ── Bottom Floating Input Dock ──────────────────────────────────────────
    let input_dock = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    input_dock.add_css_class("input-dock");
    input_dock.set_hexpand(false);

    let entry = gtk4::Entry::builder()
        .placeholder_text("Ask agent or type / for commands...")
        .hexpand(true)
        .width_chars(1)
        .css_classes(["prompt-entry"])
        .build();
    input_dock.append(&entry);

    let btn_send = gtk4::Button::builder()
        .label("↑ Send")
        .css_classes(["btn-send"])
        .tooltip_text("Send prompt (Enter)")
        .build();
    input_dock.append(&btn_send);
    drawer.append(&input_dock);

    // Footer shortcut hint
    let shortcut_hint = gtk4::Label::builder()
        .label("Ctrl+Alt+L: Toggle  •  Enter: Send  •  Esc: Close")
        .css_classes(["shortcut-footer"])
        .halign(gtk4::Align::Center)
        .build();
    drawer.append(&shortcut_hint);

    root_box.append(&drawer);
    window.set_child(Some(&root_box));

    let state = Rc::new(RefCell::new(AppState {
        mode: OverlayMode::Hidden,
        runner: PiRunner::new(),
        prompts: Vec::new(),
        settings: initial_settings.clone(),
        top_margin: initial_settings.margin_top,
        bottom_margin: initial_settings.margin_top,
        is_window_mode,
    }));

    // Connect Welcome Pill Buttons to populate prompt
    {
        let entry_ref = entry.clone();
        pill1.connect_clicked(move |_| {
            entry_ref.set_text("Please explain the architecture of this workspace.");
            entry_ref.grab_focus();
        });
        let entry_ref2 = entry.clone();
        pill2.connect_clicked(move |_| {
            entry_ref2.set_text("Run the test suite and report any errors.");
            entry_ref2.grab_focus();
        });
        let entry_ref3 = entry.clone();
        pill3.connect_clicked(move |_| {
            entry_ref3.set_text("Review recent git commits and pending changes.");
            entry_ref3.grab_focus();
        });
    }

    // Connect Settings Callbacks
    {
        let state_model = state.clone();
        let model_pill_clone = model_pill.clone();
        model_dropdown.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            if idx < POPULAR_MODELS.len() {
                let model_id = POPULAR_MODELS[idx].0.to_string();
                model_pill_clone.set_label(&format!("{} ▾", model_id));
                let mut st = state_model.borrow_mut();
                st.settings.model = model_id;
                let _ = st.settings.save();
            }
        });

        let state_think = state.clone();
        think_dropdown.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            if idx < THINKING_LEVELS.len() {
                let th_id = THINKING_LEVELS[idx].0.to_string();
                let mut st = state_think.borrow_mut();
                st.settings.thinking_level = th_id;
                let _ = st.settings.save();
            }
        });

        let state_theme = state.clone();
        let drawer_theme = drawer.clone();
        theme_dropdown.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            if idx < THEMES.len() {
                let theme_id = THEMES[idx].0;
                for (other, _, _) in THEMES {
                    drawer_theme.remove_css_class(other);
                }
                drawer_theme.add_css_class(theme_id);
                let mut st = state_theme.borrow_mut();
                st.settings.theme = theme_id.to_string();
                let _ = st.settings.save();
            }
        });

        let state_pin = state.clone();
        pin_check.connect_toggled(move |cb| {
            let mut st = state_pin.borrow_mut();
            st.settings.auto_hide_on_blur = !cb.is_active();
            let _ = st.settings.save();
        });

        let state_head = state.clone();
        let head_win = chat_head_window.clone();
        chathead_check.connect_toggled(move |cb| {
            let active = cb.is_active();
            head_win.set_visible(active);
            let mut st = state_head.borrow_mut();
            st.settings.show_chat_head = active;
            let _ = st.settings.save();
        });

        let state_eph = state.clone();
        ephemeral_check.connect_toggled(move |cb| {
            let mut st = state_eph.borrow_mut();
            st.settings.ephemeral = cb.is_active();
            let _ = st.settings.save();
        });
    }

    // Scroll to bottom helper
    let scroll_to_bottom = {
        let scrolled = scrolled.clone();
        move || {
            glib::idle_add_local_once({
                let scrolled = scrolled.clone();
                move || {
                    let adj = scrolled.vadjustment();
                    adj.set_value(adj.upper() - adj.page_size());
                }
            });
        }
    };

    // Scroll to specific widget helper
    let scroll_to_widget = {
        let scrolled = scrolled.clone();
        Rc::new(move |widget: &gtk4::Box| {
            let scrolled = scrolled.clone();
            let (_, y) = widget.translate_coordinates(&scrolled, 0.0, 0.0).unwrap_or((0.0, 0.0));
            let adj = scrolled.vadjustment();
            let target_val = (adj.value() + y - 40.0).clamp(adj.lower(), adj.upper() - adj.page_size());
            adj.set_value(target_val);
        })
    };

    // Overlay Show/Hide logic
    let hide_overlay_fn = {
        let win = window.clone();
        let state = state.clone();
        Rc::new(move || {
            let is_ephemeral = state.borrow().settings.ephemeral;
            let is_win = state.borrow().is_window_mode;
            if !is_win {
                overlay::hide_overlay(&win);
            } else {
                win.set_visible(false);
            }
            state.borrow_mut().mode = OverlayMode::Hidden;
            if is_ephemeral {
                state.borrow_mut().runner.abort();
            }
        })
    };

    let expand_overlay_fn = {
        let win = window.clone();
        let state = state.clone();
        let entry = entry.clone();
        Rc::new(move || {
            let (top, bot, right, is_win) = {
                let st = state.borrow();
                (st.top_margin, st.bottom_margin, st.settings.margin_right, st.is_window_mode)
            };
            if !is_win {
                overlay::expand_overlay(&win, top, bot, right);
            } else {
                win.set_visible(true);
                win.present();
            }
            state.borrow_mut().mode = OverlayMode::Visible;
            entry.grab_focus();
        })
    };

    // Toggle button on chat head
    {
        let expand_fn = expand_overlay_fn.clone();
        let hide_fn = hide_overlay_fn.clone();
        let state_toggle = state.clone();
        head_btn.connect_clicked(move |_| {
            let current_mode = state_toggle.borrow().mode;
            if current_mode == OverlayMode::Visible {
                hide_fn();
            } else {
                expand_fn();
            }
        });
    }

    // StatusNotifierItem System Tray Integration
    {
        let expand_fn = expand_overlay_fn.clone();
        let hide_fn = hide_overlay_fn.clone();
        let state_toggle = state.clone();
        tray::setup_tray(move || {
            let current_mode = state_toggle.borrow().mode;
            if current_mode == OverlayMode::Visible {
                hide_fn();
            } else {
                expand_fn();
            }
        });
    }

    // Close button hides overlay
    {
        let hide_fn = hide_overlay_fn.clone();
        btn_close.connect_clicked(move |_| {
            hide_fn();
        });
    }

    // Setup tokio stream channel
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<(u64, StreamEvent)>();
    let active_stream: Rc<RefCell<Option<ActiveStreamTarget>>> = Rc::new(RefCell::new(None));

    // Clear transcript helper
    let execute_clear = {
        let messages_box = messages_box.clone();
        let welcome_box = welcome_box.clone();
        let prompts_list_box = prompts_list_box.clone();
        let prompt_empty_label = prompt_empty_label.clone();
        let btn_session_clear = btn_session.clone();
        let state_clear = state.clone();
        let active_stream_clear = active_stream.clone();
        let status_dot_clear = status_dot.clone();
        let head_dot_clear = head_dot.clone();

        Rc::new(move || {
            state_clear.borrow_mut().runner.abort();
            *active_stream_clear.borrow_mut() = None;
            status_dot_clear.remove_css_class("working");
            status_dot_clear.remove_css_class("error");
            head_dot_clear.remove_css_class("working");

            while let Some(child) = messages_box.first_child() {
                messages_box.remove(&child);
            }
            welcome_box.set_visible(true);
            messages_box.append(&welcome_box);

            while let Some(child) = prompts_list_box.first_child() {
                prompts_list_box.remove(&child);
            }
            prompt_empty_label.set_visible(true);
            prompts_list_box.append(&prompt_empty_label);
            btn_session_clear.set_label("📋 Session (0)");
            state_clear.borrow_mut().prompts.clear();
        })
    };

    // Destructive Confirmation Popover for Clear action
    let confirm_popover = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .css_classes(["confirm-popover"])
        .build();
    confirm_popover.set_parent(&btn_clear);

    let confirm_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);

    let confirm_title = gtk4::Label::builder()
        .label("Clear Conversation?")
        .css_classes(["confirm-dialog-title"])
        .xalign(0.0)
        .build();
    confirm_box.append(&confirm_title);

    let confirm_desc = gtk4::Label::builder()
        .label("This will erase all active messages and session history.")
        .css_classes(["confirm-dialog-desc"])
        .wrap(true)
        .wrap_mode(gtk4::pango::WrapMode::WordChar)
        .width_chars(1)
        .max_width_chars(32)
        .xalign(0.0)
        .build();
    confirm_box.append(&confirm_desc);

    let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    btn_row.set_halign(gtk4::Align::End);

    let btn_cancel = gtk4::Button::builder()
        .label("Cancel")
        .css_classes(["btn-dialog-cancel"])
        .build();

    let btn_confirm = gtk4::Button::builder()
        .label("Clear All")
        .css_classes(["btn-dialog-destructive"])
        .build();

    btn_row.append(&btn_cancel);
    btn_row.append(&btn_confirm);
    confirm_box.append(&btn_row);
    confirm_popover.set_child(Some(&confirm_box));

    {
        let pop_ref = confirm_popover.clone();
        btn_cancel.connect_clicked(move |_| {
            pop_ref.popdown();
        });
    }

    {
        let pop_ref = confirm_popover.clone();
        let do_clear = execute_clear.clone();
        btn_confirm.connect_clicked(move |_| {
            pop_ref.popdown();
            do_clear();
        });
    }

    // Clear Button shows confirmation popover
    {
        let pop_ref = confirm_popover.clone();
        let state_check = state.clone();
        btn_clear.connect_clicked(move |_| {
            if !state_check.borrow().prompts.is_empty() {
                pop_ref.popup();
            }
        });
    }

    // Keyboard Shortcuts (Esc to close, Ctrl+L to confirm clear)
    {
        let key_controller = gtk4::EventControllerKey::new();
        let hide_fn = hide_overlay_fn.clone();
        let pop_ref = confirm_popover.clone();
        let state_check = state.clone();

        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            if key == gdk::Key::Escape {
                hide_fn();
                return glib::Propagation::Stop;
            }
            if modifier.contains(gdk::ModifierType::CONTROL_MASK) && key == gdk::Key::l {
                if !state_check.borrow().prompts.is_empty() {
                    pop_ref.popup();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(key_controller);
    }

    // Auto-hide on blur (focus loss) if enabled
    {
        let focus_controller = gtk4::EventControllerFocus::new();
        let hide_fn = hide_overlay_fn.clone();
        let state_focus = state.clone();
        let had_focus = Rc::new(Cell::new(false));

        focus_controller.connect_contains_focus_notify(move |controller| {
            let has_focus = controller.contains_focus();
            let prev = had_focus.get();
            had_focus.set(has_focus);

            if prev && !has_focus {
                let auto_hide = state_focus.borrow().settings.auto_hide_on_blur;
                if auto_hide {
                    hide_fn();
                }
            }
        });
        window.add_controller(focus_controller);
    }

    // Stream consumer loop
    {
        let active_stream_clone = active_stream.clone();
        let status_dot_clone = status_dot.clone();
        let head_dot_clone = head_dot.clone();
        let state_clone = state.clone();
        let scroll_fn = scroll_to_bottom.clone();

        glib::spawn_future_local(async move {
            while let Some((gen, event)) = stream_rx.recv().await {
                let st = state_clone.borrow();
                if gen != st.runner.generation() {
                    continue;
                }

                match event {
                    StreamEvent::ThinkingDelta(delta) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let mut cur_thought = target.active_thought_expander.borrow_mut();
                            if cur_thought.is_none() {
                                let item_expander = gtk4::Expander::builder()
                                    .label("Thinking... ▾")
                                    .css_classes(["thinking-disclosure"])
                                    .expanded(true)
                                    .build();
                                let inner_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                                inner_box.add_css_class("thinking-content-box");

                                let item_label = gtk4::Label::builder()
                                    .selectable(true)
                                    .wrap(true)
                                    .wrap_mode(gtk4::pango::WrapMode::WordChar)
                                    .width_chars(1)
                                    .max_width_chars(50)
                                    .xalign(0.0)
                                    .css_classes(["thinking-text"])
                                    .build();
                                inner_box.append(&item_label);
                                item_expander.set_child(Some(&inner_box));
                                target.turn_body.append(&item_expander);
                                *cur_thought = Some((
                                    item_expander,
                                    item_label,
                                    Rc::new(RefCell::new(String::new())),
                                    std::time::Instant::now(),
                                ));
                            }

                            if let Some((ref exp, ref lbl, ref txt, thought_start)) = *cur_thought {
                                let mut t = txt.borrow_mut();
                                t.push_str(&delta);
                                lbl.set_text(&t);
                                let elapsed = thought_start.elapsed().as_secs();
                                exp.set_label(Some(&format!("Thinking... ({}s) ▾", elapsed.max(1))));
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolStart { id, name, args } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            if let Some((ref exp, _, _, thought_start)) =
                                *target.active_thought_expander.borrow()
                            {
                                let elapsed = thought_start.elapsed().as_secs();
                                exp.set_label(Some(&format!("Thought for {}s ▾", elapsed.max(1))));
                                exp.set_expanded(false);
                            }
                            *target.active_thought_expander.borrow_mut() = None;
                            *target.active_markdown.borrow_mut() = None;

                            let tool_card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                            tool_card.add_css_class("tool-call-card");
                            tool_card.set_hexpand(false);
                            tool_card.set_halign(gtk4::Align::Fill);

                            let card_hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                            card_hdr.set_hexpand(false);
                            let action_label = format::format_tool_call_label(&name, &args);
                            let name_lbl = gtk4::Label::builder()
                                .label(format!("⚙ {}", action_label))
                                .css_classes(["tool-name-label"])
                                .halign(gtk4::Align::Start)
                                .ellipsize(gtk4::pango::EllipsizeMode::End)
                                .max_width_chars(32)
                                .hexpand(false)
                                .build();
                            card_hdr.append(&name_lbl);

                            let hdr_sp = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                            hdr_sp.set_hexpand(true);
                            card_hdr.append(&hdr_sp);

                            let badge = gtk4::Label::builder()
                                .label("RUNNING")
                                .css_classes(["tool-badge-running"])
                                .build();
                            card_hdr.append(&badge);
                            tool_card.append(&card_hdr);

                            let trimmed_args = args.trim();
                            if !trimmed_args.is_empty() && trimmed_args != "{}" {
                                let args_lbl = gtk4::Label::builder()
                                    .label(trimmed_args)
                                    .selectable(true)
                                    .wrap(true)
                                    .wrap_mode(gtk4::pango::WrapMode::WordChar)
                                    .width_chars(1)
                                    .max_width_chars(48)
                                    .xalign(0.0)
                                    .css_classes(["tool-args-box"])
                                    .build();
                                tool_card.append(&args_lbl);
                            }

                            target.turn_body.append(&tool_card);
                            target.tools.borrow_mut().insert(
                                id,
                                ToolCardEntry {
                                    card: tool_card,
                                    badge,
                                    start_time: std::time::Instant::now(),
                                },
                            );
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolEnd {
                        id,
                        name: _,
                        result,
                        is_error,
                        duration_ms,
                    } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let mut tools_map = target.tools.borrow_mut();
                            if let Some(entry) = tools_map.get_mut(&id) {
                                let dur_s = if duration_ms > 0 {
                                    duration_ms / 1000
                                } else {
                                    entry.start_time.elapsed().as_secs()
                                };
                                entry.badge.remove_css_class("tool-badge-running");
                                if is_error {
                                    entry.badge.set_text(&format!(
                                        "ERROR ({})",
                                        format::format_duration_sec(dur_s)
                                    ));
                                    entry.badge.add_css_class("tool-badge-error");
                                } else {
                                    entry.badge.set_text(&format!(
                                        "DONE ({})",
                                        format::format_duration_sec(dur_s)
                                    ));
                                    entry.badge.add_css_class("tool-badge-done");
                                }

                                let trimmed_res = result.trim();
                                if !trimmed_res.is_empty() {
                                    let output_expander = gtk4::Expander::builder()
                                        .label("Output ▾")
                                        .css_classes(["tool-output-expander"])
                                        .expanded(false)
                                        .build();

                                    let out_scroll = gtk4::ScrolledWindow::builder()
                                        .hscrollbar_policy(gtk4::PolicyType::Automatic)
                                        .vscrollbar_policy(gtk4::PolicyType::Automatic)
                                        .max_content_height(200)
                                        .propagate_natural_height(true)
                                        .build();

                                    let out_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                                    out_box.add_css_class("tool-output-box");
                                    let out_lbl = gtk4::Label::builder()
                                        .label(trimmed_res)
                                        .selectable(true)
                                        .wrap(true)
                                        .wrap_mode(gtk4::pango::WrapMode::WordChar)
                                        .width_chars(1)
                                        .max_width_chars(48)
                                        .xalign(0.0)
                                        .build();
                                    out_box.append(&out_lbl);
                                    out_scroll.set_child(Some(&out_box));
                                    output_expander.set_child(Some(&out_scroll));
                                    entry.card.append(&output_expander);
                                }
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolCall { name, args } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            if let Some((ref exp, _, _, thought_start)) =
                                *target.active_thought_expander.borrow()
                            {
                                let elapsed = thought_start.elapsed().as_secs();
                                exp.set_label(Some(&format!("Thought for {}s ▾", elapsed.max(1))));
                                exp.set_expanded(false);
                            }
                            *target.active_thought_expander.borrow_mut() = None;
                            *target.active_markdown.borrow_mut() = None;

                            let tool_card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                            tool_card.add_css_class("tool-call-card");
                            tool_card.set_hexpand(false);
                            tool_card.set_halign(gtk4::Align::Fill);

                            let card_hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                            card_hdr.set_hexpand(false);
                            let action_label = format::format_tool_call_label(&name, &args);
                            let name_lbl = gtk4::Label::builder()
                                .label(format!("⚙ {}", action_label))
                                .css_classes(["tool-name-label"])
                                .halign(gtk4::Align::Start)
                                .ellipsize(gtk4::pango::EllipsizeMode::End)
                                .max_width_chars(32)
                                .hexpand(false)
                                .build();
                            card_hdr.append(&name_lbl);

                            let hdr_sp = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                            hdr_sp.set_hexpand(true);
                            card_hdr.append(&hdr_sp);

                            let badge = gtk4::Label::builder()
                                .label("DONE")
                                .css_classes(["tool-badge-done"])
                                .build();
                            card_hdr.append(&badge);
                            tool_card.append(&card_hdr);

                            let trimmed_args = args.trim();
                            if !trimmed_args.is_empty() && trimmed_args != "{}" {
                                let args_lbl = gtk4::Label::builder()
                                    .label(trimmed_args)
                                    .selectable(true)
                                    .wrap(true)
                                    .wrap_mode(gtk4::pango::WrapMode::WordChar)
                                    .width_chars(1)
                                    .max_width_chars(48)
                                    .xalign(0.0)
                                    .css_classes(["tool-args-box"])
                                    .build();
                                tool_card.append(&args_lbl);
                            }

                            target.turn_body.append(&tool_card);
                        }
                        scroll_fn();
                    }
                    StreamEvent::TextDelta(delta) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            if let Some((ref exp, _, _, thought_start)) =
                                *target.active_thought_expander.borrow()
                            {
                                let elapsed = thought_start.elapsed().as_secs();
                                exp.set_label(Some(&format!("Thought for {}s ▾", elapsed.max(1))));
                                exp.set_expanded(false);
                            }
                            *target.active_thought_expander.borrow_mut() = None;

                            target.full_text.borrow_mut().push_str(&delta);

                            let mut md_opt = target.active_markdown.borrow_mut();
                            if md_opt.is_none() {
                                let md_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                                target.turn_body.append(&md_box);
                                *md_opt = Some((md_box, String::new()));
                            }

                            if let Some((ref md_box, ref mut md_text)) = *md_opt {
                                md_text.push_str(&delta);
                                if md_text.contains("```") || md_text.contains('|') {
                                    crate::render::render_markdown_blocks_into(md_box, md_text);
                                } else {
                                    if md_box.first_child().is_none() {
                                        let lbl = gtk4::Label::builder()
                                            .selectable(true)
                                            .wrap(true)
                                            .wrap_mode(gtk4::pango::WrapMode::WordChar)
                                            .width_chars(1)
                                            .max_width_chars(50)
                                            .xalign(0.0)
                                            .css_classes(["bubble-assistant-text"])
                                            .build();
                                        md_box.append(&lbl);
                                    }
                                    if let Some(lbl) = md_box
                                        .first_child()
                                        .and_then(|w| w.downcast::<gtk4::Label>().ok())
                                    {
                                        format::apply_assistant_markup(&lbl, md_text);
                                    }
                                }
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::Content(line) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let mut full = target.full_text.borrow_mut();
                            if !full.is_empty() {
                                full.push('\n');
                            }
                            full.push_str(&line);

                            let mut md_opt = target.active_markdown.borrow_mut();
                            if md_opt.is_none() {
                                let md_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                                target.turn_body.append(&md_box);
                                *md_opt = Some((md_box, String::new()));
                            }

                            if let Some((ref md_box, ref mut md_text)) = *md_opt {
                                if !md_text.is_empty() {
                                    md_text.push('\n');
                                }
                                md_text.push_str(&line);
                                crate::render::render_markdown_blocks_into(md_box, md_text);
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::Metrics(m) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            target.metrics_badge.set_visible(true);
                            let sec = (m.duration_ms as f64) / 1000.0;
                            target.metrics_badge.set_text(&format!(
                                "{} tok • {:.1}s",
                                m.total_tokens, sec
                            ));
                        }
                    }
                    StreamEvent::Error(err_line) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let err_msg = format!("[Error] {}", err_line);
                            target.full_text.borrow_mut().push_str(&err_msg);

                            let mut md_opt = target.active_markdown.borrow_mut();
                            if md_opt.is_none() {
                                let md_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                                target.turn_body.append(&md_box);
                                *md_opt = Some((md_box, String::new()));
                            }

                            if let Some((ref md_box, ref mut md_text)) = *md_opt {
                                md_text.push_str(&err_msg);
                                crate::render::render_markdown_blocks_into(md_box, md_text);
                            }
                        }
                        status_dot_clone.remove_css_class("working");
                        status_dot_clone.add_css_class("error");
                        scroll_fn();
                    }
                    StreamEvent::Done => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            if let Some((ref exp, _, _, thought_start)) =
                                *target.active_thought_expander.borrow()
                            {
                                let elapsed = thought_start.elapsed().as_secs();
                                exp.set_label(Some(&format!("Thought for {}s ▾", elapsed.max(1))));
                                exp.set_expanded(false);
                            }
                            *target.active_thought_expander.borrow_mut() = None;

                            if let Some((ref md_box, ref md_text)) = *target.active_markdown.borrow() {
                                crate::render::render_markdown_blocks_into(md_box, md_text);
                            }
                        }
                        if state_clone.borrow().settings.sound_bell_on_done {
                            overlay::ring_completion_bell();
                        }
                        status_dot_clone.remove_css_class("working");
                        status_dot_clone.remove_css_class("error");
                        head_dot_clone.remove_css_class("working");
                        scroll_fn();
                    }
                }
            }
        });
    }

    // Submit Prompt handler
    let submit_prompt = {
        let entry = entry.clone();
        let messages_box = messages_box.clone();
        let welcome_box = welcome_box.clone();
        let prompts_list_box = prompts_list_box.clone();
        let prompt_empty_label = prompt_empty_label.clone();
        let prompts_popover = prompts_popover.clone();
        let state = state.clone();
        let active_stream = active_stream.clone();
        let status_dot = status_dot.clone();
        let head_dot = head_dot.clone();
        let stream_tx = stream_tx.clone();
        let scroll_fn = scroll_to_bottom.clone();
        let scroll_to_widget = scroll_to_widget.clone();
        let btn_session_submit = btn_session.clone();

        Rc::new(move || {
            let raw_prompt = entry.text().trim().to_string();
            if raw_prompt.is_empty() {
                return;
            }
            let prompt = commands::expand_slash_command(&raw_prompt).unwrap_or(raw_prompt);

            entry.set_text("");
            welcome_box.set_visible(false);

            let prompt_idx = state.borrow().prompts.len() + 1;
            btn_session_submit.set_label(&format!("📋 Session ({})", prompt_idx));

            // 1. User Message Card
            let user_card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            user_card.add_css_class("card-user");

            let u_hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            u_hdr.add_css_class("card-user-header");

            let u_avatar = gtk4::Label::builder()
                .label("●")
                .css_classes(["card-user-avatar"])
                .build();
            let u_badge = gtk4::Label::builder()
                .label("You")
                .css_classes(["card-user-badge"])
                .build();
            u_hdr.append(&u_avatar);
            u_hdr.append(&u_badge);

            let u_sp = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            u_sp.set_hexpand(true);
            u_hdr.append(&u_sp);

            let u_idx = gtk4::Label::builder()
                .label(format!("#{}", prompt_idx))
                .css_classes(["card-user-idx"])
                .build();
            u_hdr.append(&u_idx);
            user_card.append(&u_hdr);

            let user_label = gtk4::Label::builder()
                .label(&prompt)
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .width_chars(1)
                .max_width_chars(50)
                .selectable(true)
                .xalign(0.0)
                .css_classes(["card-user-text"])
                .build();
            user_card.append(&user_label);
            messages_box.append(&user_card);

            // 2. Assistant Message Card
            let asst_card = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
            asst_card.add_css_class("card-assistant");

            let asst_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            asst_header.add_css_class("card-assistant-header");

            let asst_avatar = gtk4::Label::builder()
                .label("✦")
                .css_classes(["card-assistant-avatar"])
                .build();
            let asst_badge = gtk4::Label::builder()
                .label("Agent")
                .css_classes(["card-assistant-badge"])
                .build();
            asst_header.append(&asst_avatar);
            asst_header.append(&asst_badge);

            let metrics_badge = gtk4::Label::new(None);
            metrics_badge.add_css_class("card-metrics-pill");
            metrics_badge.set_visible(false);
            asst_header.append(&metrics_badge);

            let asst_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            asst_spacer.set_hexpand(true);
            asst_header.append(&asst_spacer);

            let btn_copy = gtk4::Button::builder()
                .label("Copy")
                .css_classes(["card-copy-btn"])
                .tooltip_text("Copy entire response")
                .build();
            asst_header.append(&btn_copy);
            asst_card.append(&asst_header);

            let turn_body = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
            asst_card.append(&turn_body);

            messages_box.append(&asst_card);

            let full_text = Rc::new(RefCell::new(String::new()));
            let copy_text_ref = full_text.clone();
            let copy_btn_clone = btn_copy.clone();
            btn_copy.connect_clicked(move |_| {
                let content = copy_text_ref.borrow().clone();
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&content);
                }
                copy_btn_clone.set_label("Copied ✓");
                let reset_btn = copy_btn_clone.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    reset_btn.set_label("Copy");
                });
            });

            // 3. Track in Prompts Flyout with per-item Delete button
            prompt_empty_label.set_visible(false);

            let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
            row_box.add_css_class("prompt-row-box");

            let nav_btn = gtk4::Button::builder()
                .css_classes(["prompt-item-btn"])
                .hexpand(true)
                .build();

            let nav_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            let nav_pill = gtk4::Label::builder()
                .label(format!("#{}", prompt_idx))
                .css_classes(["prompt-idx-pill"])
                .build();
            nav_row.append(&nav_pill);

            let snippet = if prompt.chars().count() > 24 {
                format!("{}…", prompt.chars().take(24).collect::<String>())
            } else {
                prompt.clone()
            };
            let nav_txt = gtk4::Label::builder()
                .label(&snippet)
                .css_classes(["prompt-snippet-text"])
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .xalign(0.0)
                .hexpand(true)
                .build();
            nav_row.append(&nav_txt);
            nav_btn.set_child(Some(&nav_row));

            let target_widget = user_card.clone();
            let pop_ref = prompts_popover.clone();
            let scroll_fn_widget = scroll_to_widget.clone();

            nav_btn.connect_clicked(move |_| {
                pop_ref.popdown();
                scroll_fn_widget(&target_widget);
            });
            row_box.append(&nav_btn);

            let del_btn = gtk4::Button::builder()
                .label("🗑")
                .css_classes(["prompt-delete-btn"])
                .tooltip_text("Delete prompt from session")
                .build();
            row_box.append(&del_btn);

            prompts_list_box.append(&row_box);

            // Per-item prompt deletion connection with safety confirmation
            {
                let row_ref = row_box.clone();
                let u_card = user_card.clone();
                let a_card = asst_card.clone();
                let msg_box_ref = messages_box.clone();
                let p_list_box_ref = prompts_list_box.clone();
                let p_empty_lbl = prompt_empty_label.clone();
                let wel_box = welcome_box.clone();
                let st_ref = state.clone();
                let btn_sess_ref = btn_session_submit.clone();
                let active_stream_ref = active_stream.clone();
                let status_dot_ref = status_dot.clone();
                let head_dot_ref = head_dot.clone();
                let this_idx = prompt_idx;

                let item_popover = gtk4::Popover::builder()
                    .autohide(true)
                    .has_arrow(true)
                    .css_classes(["confirm-popover"])
                    .build();
                item_popover.set_parent(&del_btn);

                let del_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
                let del_title = gtk4::Label::builder()
                    .label("Delete this message?")
                    .css_classes(["confirm-dialog-title"])
                    .xalign(0.0)
                    .build();
                del_box.append(&del_title);

                let del_desc = gtk4::Label::builder()
                    .label("Remove this turn and its response from history.")
                    .css_classes(["confirm-dialog-desc"])
                    .wrap(true)
                    .wrap_mode(gtk4::pango::WrapMode::WordChar)
                    .width_chars(1)
                    .max_width_chars(28)
                    .xalign(0.0)
                    .build();
                del_box.append(&del_desc);

                let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                btn_row.set_halign(gtk4::Align::End);

                let btn_cancel_item = gtk4::Button::builder()
                    .label("Cancel")
                    .css_classes(["btn-dialog-cancel"])
                    .build();
                btn_row.append(&btn_cancel_item);

                let btn_confirm_item = gtk4::Button::builder()
                    .label("Delete")
                    .css_classes(["btn-dialog-destructive"])
                    .build();
                btn_row.append(&btn_confirm_item);
                del_box.append(&btn_row);
                item_popover.set_child(Some(&del_box));

                let pop_close_cancel = item_popover.clone();
                btn_cancel_item.connect_clicked(move |_| {
                    pop_close_cancel.popdown();
                });

                let pop_close_confirm = item_popover.clone();
                btn_confirm_item.connect_clicked(move |_| {
                    pop_close_confirm.popdown();
                    msg_box_ref.remove(&u_card);
                    msg_box_ref.remove(&a_card);
                    p_list_box_ref.remove(&row_ref);

                    let mut st = st_ref.borrow_mut();
                    if st.prompts.last().map(|p| p.index == this_idx).unwrap_or(false) {
                        st.runner.abort();
                        *active_stream_ref.borrow_mut() = None;
                        status_dot_ref.remove_css_class("working");
                        head_dot_ref.remove_css_class("working");
                    }
                    st.prompts.retain(|p| p.index != this_idx);
                    let remaining = st.prompts.len();
                    btn_sess_ref.set_label(&format!("📋 Session ({})", remaining));

                    if remaining == 0 {
                        p_empty_lbl.set_visible(true);
                        wel_box.set_visible(true);
                        msg_box_ref.append(&wel_box);
                    }
                });

                let pop_show = item_popover.clone();
                del_btn.connect_clicked(move |_| {
                    pop_show.popup();
                });
            }

            state.borrow_mut().prompts.push(TrackedPrompt {
                index: prompt_idx,
            });

            *active_stream.borrow_mut() = Some(ActiveStreamTarget {
                turn_body: turn_body.clone(),
                full_text: full_text.clone(),
                active_thought_expander: Rc::new(RefCell::new(None)),
                active_markdown: Rc::new(RefCell::new(None)),
                tools: Rc::new(RefCell::new(HashMap::new())),
                metrics_badge: metrics_badge.clone(),
            });

            scroll_fn();
            status_dot.remove_css_class("error");
            status_dot.add_css_class("working");
            head_dot.add_css_class("working");

            let mut st = state.borrow_mut();
            let model_opt = Some(st.settings.model.clone());
            let thinking_opt = Some(st.settings.thinking_level.clone());
            if let Err(e) =
                st.runner
                    .spawn_prompt(prompt, model_opt, thinking_opt, stream_tx.clone())
            {
                let err_str = format!("[Error] Failed to spawn agent: {}", e);
                full_text.borrow_mut().push_str(&err_str);
                let err_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                turn_body.append(&err_box);
                crate::render::render_markdown_blocks_into(&err_box, &err_str);
                status_dot.remove_css_class("working");
                status_dot.add_css_class("error");
                head_dot.remove_css_class("working");
                scroll_fn();
            }
        })
    };

    let submit_on_enter = submit_prompt.clone();
    entry.connect_activate(move |_| {
        submit_on_enter();
    });

    let submit_on_click = submit_prompt.clone();
    btn_send.connect_clicked(move |_| {
        submit_on_click();
    });

    // Start IPC socket listener
    let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel::<ipc::IpcCommand>();
    ipc::start_ipc_listener(ipc_tx);

    let expand_fn_ipc = expand_overlay_fn.clone();
    let hide_fn_ipc = hide_overlay_fn.clone();
    let state_ipc = state.clone();
    let entry_ipc = entry.clone();
    let submit_ipc = submit_prompt.clone();

    glib::spawn_future_local(async move {
        while let Some(cmd) = ipc_rx.recv().await {
            match cmd {
                ipc::IpcCommand::Toggle => {
                    let current = state_ipc.borrow().mode;
                    if current == OverlayMode::Visible {
                        hide_fn_ipc();
                    } else {
                        expand_fn_ipc();
                    }
                }
                ipc::IpcCommand::Prompt(text) => {
                    expand_fn_ipc();
                    entry_ipc.set_text(&text);
                    submit_ipc();
                }
            }
        }
    });

    // Expand overlay on startup unless launched as hidden daemon
    if !is_daemon {
        expand_overlay_fn();
    }

    AppContext {
        window,
        state,
        _hold_guard: hold_guard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_popular_models_list() {
        assert!(!POPULAR_MODELS.is_empty());
        assert_eq!(POPULAR_MODELS[0].0, "default");
    }

    #[test]
    fn test_theme_list_integrity() {
        assert!(!THEMES.is_empty());
        for (id, label, desc) in THEMES {
            assert!(!id.is_empty());
            assert!(!label.is_empty());
            assert!(!desc.is_empty());
        }
    }
}
