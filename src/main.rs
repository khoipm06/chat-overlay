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
    ("default", "Auto"),
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
    ("off", "Off"),
    ("low", "Low"),
    ("medium", "Balance"),
    ("high", "Deep"),
];

pub const AGENT_MODES: &[(&str, &str, &str)] = &[
    ("agent", "Agent", "Autonomous workspace execution"),
    ("ask", "Ask", "Answers questions without file edits"),
    ("edit", "Edit", "Direct focused code modifications"),
    ("plan", "Plan", "Architectural plans and breakdowns"),
];

pub fn model_display_name(model_id: &str) -> String {
    for (id, name) in POPULAR_MODELS {
        if *id == model_id {
            if *id == "default" {
                return "Auto".to_string();
            }
            if let Some(short) = name.strip_suffix(" Sonnet") {
                return short.to_string();
            }
            return (*name).to_string();
        }
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
    "Agent".to_string()
}

struct TrackedPrompt {
    index: usize,
    user_card: gtk4::Box,
}

struct StepItemEntry {
    badge: gtk4::Label,
    start_time: std::time::Instant,
}

struct ProgressDisclosureState {
    expander: gtk4::Expander,
    steps_box: gtk4::Box,
    steps_count: usize,
    start_time: std::time::Instant,
    items: HashMap<String, StepItemEntry>,
}

struct ActiveStreamTarget {
    turn_body: gtk4::Box,
    full_text: Rc<RefCell<String>>,
    progress: Rc<RefCell<Option<ProgressDisclosureState>>>,
    active_markdown: Rc<RefCell<Option<(gtk4::Box, String)>>>,
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
    current_mode: String,
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

fn get_text_view_content(tv: &gtk4::TextView) -> String {
    let buffer = tv.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, false).trim().to_string()
}

fn set_text_view_content(tv: &gtk4::TextView, text: &str) {
    tv.buffer().set_text(text);
}

fn add_context_chip(chips_box: &gtk4::Box, name: &str) {
    let chip = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    chip.add_css_class("composer-chip");

    let chip_icon = gtk4::Label::builder()
        .label("+")
        .css_classes(["composer-chip-icon"])
        .build();
    let chip_text = gtk4::Label::builder()
        .label(name)
        .css_classes(["composer-chip-text"])
        .build();
    let chip_del = gtk4::Button::builder()
        .label("✕")
        .css_classes(["composer-chip-del"])
        .tooltip_text("Remove context chip")
        .build();

    chip.append(&chip_icon);
    chip.append(&chip_text);
    chip.append(&chip_del);

    let chips_box_clone = chips_box.clone();
    let chip_clone = chip.clone();
    chip_del.connect_clicked(move |_| {
        chips_box_clone.remove(&chip_clone);
    });

    chips_box.append(&chip);
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
    let hold_guard = app.hold();
    let initial_settings = AppSettings::load();

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("Chat")
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
        .tooltip_text("Toggle Chat Sidebar (Ctrl+Alt+L)")
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

    // Main Drawer Card Container (Sidebar)
    let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    drawer.set_widget_name("drawer-container");
    drawer.add_css_class("drawer-container");
    drawer.add_css_class(&initial_settings.theme);
    drawer.set_width_request(initial_settings.width);
    drawer.set_vexpand(true);
    drawer.set_hexpand(false);

    // ── 1. Panel Header (Thanh Panel) ───────────────────────────────────────
    let panel_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    panel_header.add_css_class("panel-header");
    panel_header.set_hexpand(false);

    let panel_title = gtk4::Label::builder()
        .label("Chat")
        .css_classes(["panel-title"])
        .halign(gtk4::Align::Start)
        .build();
    panel_header.append(&panel_title);

    let header_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    header_spacer.set_hexpand(true);
    panel_header.append(&header_spacer);

    // Panel Actions (Right side)
    let btn_new_chat = gtk4::Button::builder()
        .label("+")
        .css_classes(["header-icon-btn"])
        .tooltip_text("New Chat (Ctrl+N)")
        .build();
    panel_header.append(&btn_new_chat);

    let btn_session_list = gtk4::Button::builder()
        .label("⌄")
        .css_classes(["header-icon-btn"])
        .tooltip_text("Session History")
        .build();
    panel_header.append(&btn_session_list);

    let btn_more = gtk4::Button::builder()
        .label("···")
        .css_classes(["header-icon-btn"])
        .tooltip_text("More Actions")
        .build();
    panel_header.append(&btn_more);

    let btn_layout = gtk4::Button::builder()
        .label("◫")
        .css_classes(["header-icon-btn"])
        .tooltip_text("Toggle Layout / Window Mode")
        .build();
    panel_header.append(&btn_layout);

    let btn_close = gtk4::Button::builder()
        .label("✕")
        .css_classes(["header-icon-btn", "close"])
        .tooltip_text("Close Overlay (Esc)")
        .build();
    panel_header.append(&btn_close);

    drawer.append(&panel_header);

    // ── 2. Session Sub-Header (Thanh Phiên) ──────────────────────────────────
    let session_subheader = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    session_subheader.add_css_class("session-subheader");

    let btn_back = gtk4::Button::builder()
        .label("←")
        .css_classes(["session-back-btn"])
        .tooltip_text("Previous Turn / Session")
        .build();
    session_subheader.append(&btn_back);

    let session_title_label = gtk4::Label::builder()
        .label("Local  •  Tạo giao diện chat giống copilot")
        .css_classes(["session-title-label"])
        .halign(gtk4::Align::Start)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(36)
        .build();
    session_subheader.append(&session_title_label);

    let subheader_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    subheader_spacer.set_hexpand(true);
    session_subheader.append(&subheader_spacer);

    let btn_side = gtk4::Button::builder()
        .label("◫")
        .css_classes(["session-side-btn"])
        .tooltip_text("Open to the Side")
        .build();
    session_subheader.append(&btn_side);

    drawer.append(&session_subheader);

    // ── 3. Chat Messages Scrollable Area (Vùng Hội Thoại) ────────────────────
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
        .label("Copilot Chat")
        .css_classes(["welcome-title"])
        .build();
    welcome_box.append(&w_title);

    let w_sub = gtk4::Label::builder()
        .label("Describe what to build, ask questions, or execute workflows")
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

    // Scrolled window overlay with Floating Scroll-to-Bottom button
    let scrolled_overlay = gtk4::Overlay::new();
    scrolled_overlay.set_child(Some(&scrolled));
    scrolled_overlay.set_vexpand(true);

    let btn_scroll_bottom = gtk4::Button::builder()
        .label("↓")
        .css_classes(["btn-scroll-bottom"])
        .tooltip_text("Scroll to bottom")
        .halign(gtk4::Align::End)
        .valign(gtk4::Align::End)
        .margin_end(16)
        .margin_bottom(12)
        .visible(false)
        .build();
    scrolled_overlay.add_overlay(&btn_scroll_bottom);
    drawer.append(&scrolled_overlay);

    // ── 5. Pinned Composer (pinned at bottom) ────────────────────────────────
    let composer_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    composer_box.add_css_class("composer-container");

    // Context Chips Row
    let chips_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    chips_row.add_css_class("composer-chips-row");
    // Default context chip matching user reference screenshot
    add_context_chip(&chips_row, "spec.md");
    composer_box.append(&chips_row);

    // Multi-line Text View with Placeholder in Overlay
    let input_overlay = gtk4::Overlay::new();
    let text_view = gtk4::TextView::builder()
        .wrap_mode(gtk4::WrapMode::WordChar)
        .css_classes(["composer-text-view"])
        .vexpand(false)
        .hexpand(true)
        .accepts_tab(false)
        .build();

    let input_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .min_content_height(38)
        .max_content_height(120)
        .propagate_natural_height(true)
        .child(&text_view)
        .build();
    input_overlay.set_child(Some(&input_scroll));

    let placeholder_label = gtk4::Label::builder()
        .label("Describe what to build")
        .css_classes(["composer-placeholder"])
        .halign(gtk4::Align::Start)
        .valign(gtk4::Align::Start)
        .margin_start(4)
        .margin_top(2)
        .can_target(false)
        .build();
    input_overlay.add_overlay(&placeholder_label);

    {
        let pl_ref = placeholder_label.clone();
        text_view.buffer().connect_changed(move |buf| {
            let empty = buf.char_count() == 0;
            pl_ref.set_visible(empty);
        });
    }

    composer_box.append(&input_overlay);

    // Composer Bottom Toolbar (integrated compact controls)
    let composer_toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    composer_toolbar.add_css_class("composer-toolbar");

    let btn_attach = gtk4::Button::builder()
        .label("+")
        .css_classes(["composer-pill-btn"])
        .tooltip_text("Attach context chip")
        .build();
    composer_toolbar.append(&btn_attach);

    let btn_mode_pill = gtk4::Button::builder()
        .label("@ Agent")
        .css_classes(["composer-pill-btn"])
        .tooltip_text("Agent Mode")
        .build();
    composer_toolbar.append(&btn_mode_pill);

    let initial_model_name = model_display_name(&initial_settings.model);
    let btn_model_pill = gtk4::Button::builder()
        .label(&initial_model_name)
        .css_classes(["composer-pill-btn"])
        .tooltip_text("Active Model")
        .build();
    composer_toolbar.append(&btn_model_pill);

    let initial_think_name = thinking_display_name(&initial_settings.thinking_level);
    let btn_think_pill = gtk4::Button::builder()
        .label(&initial_think_name)
        .css_classes(["composer-pill-btn"])
        .tooltip_text("Reasoning Depth")
        .build();
    composer_toolbar.append(&btn_think_pill);

    let btn_swap = gtk4::Button::builder()
        .label("⇄")
        .css_classes(["composer-icon-btn"])
        .tooltip_text("Toggle Agent / Ask Mode")
        .build();
    composer_toolbar.append(&btn_swap);

    let tb_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    tb_spacer.set_hexpand(true);
    composer_toolbar.append(&tb_spacer);

    let btn_voice = gtk4::Button::builder()
        .label("🎙")
        .css_classes(["composer-icon-btn"])
        .tooltip_text("Voice Input")
        .build();
    composer_toolbar.append(&btn_voice);

    let btn_send_stop = gtk4::Button::builder()
        .label("↑")
        .css_classes(["composer-send-btn"])
        .tooltip_text("Send (Enter)")
        .build();
    composer_toolbar.append(&btn_send_stop);

    composer_box.append(&composer_toolbar);
    drawer.append(&composer_box);

    // ── 6. Status Footer (Chân Trạng Thái) ───────────────────────────────────
    let status_footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    status_footer.add_css_class("status-footer");

    let status_server_icon = gtk4::Label::builder()
        .label("🖧")
        .css_classes(["status-footer-icon"])
        .build();
    status_footer.append(&status_server_icon);

    let status_env_label = gtk4::Label::builder()
        .label("Local  •  Autopilot (Preview)")
        .css_classes(["status-footer-label"])
        .build();
    status_footer.append(&status_env_label);

    let sf_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    sf_spacer.set_hexpand(true);
    status_footer.append(&sf_spacer);

    let status_dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    status_dot.add_css_class("status-footer-dot");
    status_dot.set_valign(gtk4::Align::Center);
    status_footer.append(&status_dot);

    drawer.append(&status_footer);

    root_box.append(&drawer);
    window.set_child(Some(&root_box));

    let state = Rc::new(RefCell::new(AppState {
        mode: OverlayMode::Hidden,
        runner: PiRunner::new(),
        prompts: Vec::new(),
        settings: initial_settings.clone(),
        current_mode: "agent".to_string(),
        top_margin: initial_settings.margin_top,
        bottom_margin: initial_settings.margin_top,
        is_window_mode,
    }));

    let is_streaming = Rc::new(Cell::new(false));

    // ── Attach Context Popover ──────────────────────────────────────────────
    let popover_attach = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .css_classes(["clean-popover"])
        .build();
    popover_attach.set_parent(&btn_attach);

    let attach_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    attach_box.set_width_request(190);

    let attach_title = gtk4::Label::builder()
        .label("ATTACH CONTEXT")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    attach_box.append(&attach_title);

    let sample_files = ["spec.md", "chatViewPane.ts", "chatInputPart.ts", "style.css", "Cargo.toml"];
    for file in sample_files {
        let item_btn = gtk4::Button::builder()
            .label(file)
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();
        let pop_close = popover_attach.clone();
        let chips_ref = chips_row.clone();
        let fname = file.to_string();
        item_btn.connect_clicked(move |_| {
            pop_close.popdown();
            add_context_chip(&chips_ref, &fname);
        });
        attach_box.append(&item_btn);
    }
    popover_attach.set_child(Some(&attach_box));

    {
        let pop_ref = popover_attach.clone();
        btn_attach.connect_clicked(move |_| {
            pop_ref.popup();
        });
    }

    // ── Mode Popover ────────────────────────────────────────────────────────
    let popover_mode = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .css_classes(["clean-popover"])
        .build();
    popover_mode.set_parent(&btn_mode_pill);

    let mode_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    mode_box.set_width_request(220);

    let mode_title = gtk4::Label::builder()
        .label("SELECT AGENT MODE")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    mode_box.append(&mode_title);

    for (m_id, m_name, m_desc) in AGENT_MODES {
        let m_btn = gtk4::Button::builder()
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();

        let inner_m = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        let lbl_name = gtk4::Label::builder()
            .label(format!("@ {}", m_name))
            .xalign(0.0)
            .build();
        let lbl_desc = gtk4::Label::builder()
            .label(*m_desc)
            .css_classes(["clean-popover-item-desc"])
            .xalign(0.0)
            .build();
        inner_m.append(&lbl_name);
        inner_m.append(&lbl_desc);
        m_btn.set_child(Some(&inner_m));

        let pop_m_close = popover_mode.clone();
        let btn_mode_lbl = btn_mode_pill.clone();
        let state_m = state.clone();
        let mode_key = m_id.to_string();
        let mode_label_str = format!("@ {}", m_name);

        m_btn.connect_clicked(move |_| {
            pop_m_close.popdown();
            btn_mode_lbl.set_label(&mode_label_str);
            state_m.borrow_mut().current_mode = mode_key.clone();
        });
        mode_box.append(&m_btn);
    }
    popover_mode.set_child(Some(&mode_box));

    {
        let pop_ref = popover_mode.clone();
        btn_mode_pill.connect_clicked(move |_| {
            pop_ref.popup();
        });
    }

    // Toggle swap button between Agent and Ask
    {
        let btn_mode_lbl = btn_mode_pill.clone();
        let state_m = state.clone();
        btn_swap.connect_clicked(move |_| {
            let mut st = state_m.borrow_mut();
            if st.current_mode == "agent" {
                st.current_mode = "ask".to_string();
                btn_mode_lbl.set_label("@ Ask");
            } else {
                st.current_mode = "agent".to_string();
                btn_mode_lbl.set_label("@ Agent");
            }
        });
    }

    // ── Model Popover ───────────────────────────────────────────────────────
    let popover_model = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .css_classes(["clean-popover"])
        .build();
    popover_model.set_parent(&btn_model_pill);

    let model_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    model_box.set_width_request(240);

    let model_title = gtk4::Label::builder()
        .label("SELECT MODEL")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    model_box.append(&model_title);

    for (m_id, m_name) in POPULAR_MODELS {
        let m_btn = gtk4::Button::builder()
            .label(*m_name)
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();

        let pop_mdl_close = popover_model.clone();
        let btn_mdl_lbl = btn_model_pill.clone();
        let state_mdl = state.clone();
        let model_key = m_id.to_string();
        let short_name = model_display_name(m_id);

        m_btn.connect_clicked(move |_| {
            pop_mdl_close.popdown();
            btn_mdl_lbl.set_label(&short_name);
            let mut st = state_mdl.borrow_mut();
            st.settings.model = model_key.clone();
            let _ = st.settings.save();
        });
        model_box.append(&m_btn);
    }
    popover_model.set_child(Some(&model_box));

    {
        let pop_ref = popover_model.clone();
        btn_model_pill.connect_clicked(move |_| {
            pop_ref.popup();
        });
    }

    // ── Thinking Depth Popover ──────────────────────────────────────────────
    let popover_think = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .css_classes(["clean-popover"])
        .build();
    popover_think.set_parent(&btn_think_pill);

    let think_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    think_box.set_width_request(180);

    let think_title = gtk4::Label::builder()
        .label("REASONING DEPTH")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    think_box.append(&think_title);

    for (th_id, th_name) in THINKING_LEVELS {
        let th_btn = gtk4::Button::builder()
            .label(*th_name)
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();

        let pop_th_close = popover_think.clone();
        let btn_th_lbl = btn_think_pill.clone();
        let state_th = state.clone();
        let think_key = th_id.to_string();
        let label_name = th_name.to_string();

        th_btn.connect_clicked(move |_| {
            pop_th_close.popdown();
            btn_th_lbl.set_label(&label_name);
            let mut st = state_th.borrow_mut();
            st.settings.thinking_level = think_key.clone();
            let _ = st.settings.save();
        });
        think_box.append(&th_btn);
    }
    popover_think.set_child(Some(&think_box));

    {
        let pop_ref = popover_think.clone();
        btn_think_pill.connect_clicked(move |_| {
            pop_ref.popup();
        });
    }

    // ── Session History Popover (⌄) ─────────────────────────────────────────
    let popover_session = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .css_classes(["clean-popover"])
        .build();
    popover_session.set_parent(&btn_session_list);

    let session_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    session_box.set_width_request(280);

    let session_pop_title = gtk4::Label::builder()
        .label("SESSION HISTORY")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    session_box.append(&session_pop_title);

    let prompts_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .max_content_height(260)
        .propagate_natural_height(true)
        .build();

    let prompts_list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let prompt_empty_label = gtk4::Label::builder()
        .label("No prompts in this session yet.")
        .css_classes(["prompt-snippet-text"])
        .margin_top(12)
        .margin_bottom(12)
        .xalign(0.5)
        .build();
    prompts_list_box.append(&prompt_empty_label);
    prompts_scroll.set_child(Some(&prompts_list_box));
    session_box.append(&prompts_scroll);
    popover_session.set_child(Some(&session_box));

    {
        let pop_ref = popover_session.clone();
        btn_session_list.connect_clicked(move |_| {
            pop_ref.popup();
        });
    }

    // ── More Actions Menu Popover (···) ──────────────────────────────────────
    let popover_more = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .css_classes(["clean-popover"])
        .build();
    popover_more.set_parent(&btn_more);

    let more_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    more_box.set_width_request(220);

    let more_title = gtk4::Label::builder()
        .label("MORE ACTIONS")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    more_box.append(&more_title);

    // Appearance Theme Section
    let theme_section_title = gtk4::Label::builder()
        .label("THEME")
        .css_classes(["clean-popover-item-desc"])
        .margin_start(8)
        .margin_top(4)
        .xalign(0.0)
        .build();
    more_box.append(&theme_section_title);

    for (t_id, t_name, _) in THEMES {
        let t_btn = gtk4::Button::builder()
            .label(*t_name)
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();

        let drawer_theme = drawer.clone();
        let state_theme = state.clone();
        let pop_more_close = popover_more.clone();
        let tid = *t_id;

        t_btn.connect_clicked(move |_| {
            pop_more_close.popdown();
            for (other, _, _) in THEMES {
                drawer_theme.remove_css_class(other);
            }
            drawer_theme.add_css_class(tid);
            let mut st = state_theme.borrow_mut();
            st.settings.theme = tid.to_string();
            let _ = st.settings.save();
        });
        more_box.append(&t_btn);
    }

    // Clear Chat Option
    let btn_clear_chat = gtk4::Button::builder()
        .label("🗑 Clear Chat History")
        .css_classes(["clean-popover-item"])
        .margin_top(6)
        .halign(gtk4::Align::Fill)
        .build();
    more_box.append(&btn_clear_chat);

    // Window Toggles
    let pin_check = gtk4::CheckButton::builder()
        .label("Pin sidebar on blur")
        .active(!initial_settings.auto_hide_on_blur)
        .css_classes(["clean-popover-check"])
        .build();
    more_box.append(&pin_check);

    let chathead_check = gtk4::CheckButton::builder()
        .label("Floating chat head bubble")
        .active(initial_settings.show_chat_head)
        .css_classes(["clean-popover-check"])
        .build();
    more_box.append(&chathead_check);

    let ephemeral_check = gtk4::CheckButton::builder()
        .label("Ephemeral (Clear on hide)")
        .active(initial_settings.ephemeral)
        .css_classes(["clean-popover-check"])
        .build();
    more_box.append(&ephemeral_check);

    popover_more.set_child(Some(&more_box));

    {
        let pop_ref = popover_more.clone();
        btn_more.connect_clicked(move |_| {
            pop_ref.popup();
        });
    }

    // Connect Settings Checkbox Callbacks
    {
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

    // Scroll button visibility based on vertical adjustment
    {
        let btn_scroll_ref = btn_scroll_bottom.clone();
        scrolled.vadjustment().connect_value_changed(move |adj| {
            let at_bottom = adj.value() >= adj.upper() - adj.page_size() - 40.0;
            btn_scroll_ref.set_visible(!at_bottom);
        });

        let scroll_fn = scroll_to_bottom.clone();
        btn_scroll_bottom.connect_clicked(move |_| {
            scroll_fn();
        });
    }

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
        let tv = text_view.clone();
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
            tv.grab_focus();
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
        let state_clear = state.clone();
        let active_stream_clear = active_stream.clone();
        let status_dot_clear = status_dot.clone();
        let head_dot_clear = head_dot.clone();
        let is_streaming_clear = is_streaming.clone();
        let btn_send_clear = btn_send_stop.clone();
        let session_title_clear = session_title_label.clone();

        Rc::new(move || {
            state_clear.borrow_mut().runner.abort();
            *active_stream_clear.borrow_mut() = None;
            is_streaming_clear.set(false);
            btn_send_clear.set_label("↑");
            btn_send_clear.remove_css_class("stop");
            btn_send_clear.set_tooltip_text(Some("Send (Enter)"));
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
            session_title_clear.set_label("Local  •  New Session");
            state_clear.borrow_mut().prompts.clear();
        })
    };

    // Destructive Confirmation Popover for Clear action
    let confirm_popover = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .css_classes(["confirm-popover"])
        .build();
    confirm_popover.set_parent(&btn_clear_chat);

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
        let pop_more_ref = popover_more.clone();
        btn_confirm.connect_clicked(move |_| {
            pop_ref.popdown();
            pop_more_ref.popdown();
            do_clear();
        });
    }

    // Clear Button in more menu shows confirmation popover
    {
        let pop_ref = confirm_popover.clone();
        let state_check = state.clone();
        btn_clear_chat.connect_clicked(move |_| {
            if !state_check.borrow().prompts.is_empty() {
                pop_ref.popup();
            }
        });
    }

    // New Chat Button (+) in panel header
    {
        let do_clear = execute_clear.clone();
        let tv_ref = text_view.clone();
        btn_new_chat.connect_clicked(move |_| {
            do_clear();
            tv_ref.grab_focus();
        });
    }

    // Layout Button (◫) in panel header and subheader
    {
        let win = window.clone();
        let state_layout = state.clone();
        let on_toggle_layout = move || {
            let is_win = {
                let mut st = state_layout.borrow_mut();
                st.is_window_mode = !st.is_window_mode;
                st.is_window_mode
            };
            if is_win {
                win.set_decorated(true);
            } else {
                win.set_decorated(false);
            }
        };

        let fn_clone1 = on_toggle_layout.clone();
        btn_layout.connect_clicked(move |_| {
            fn_clone1();
        });

        let fn_clone2 = on_toggle_layout;
        btn_side.connect_clicked(move |_| {
            fn_clone2();
        });
    }

    // Connect Welcome Pill Buttons to populate prompt
    {
        let tv = text_view.clone();
        pill1.connect_clicked(move |_| {
            set_text_view_content(&tv, "Please explain the architecture of this workspace.");
            tv.grab_focus();
        });
        let tv2 = text_view.clone();
        pill2.connect_clicked(move |_| {
            set_text_view_content(&tv2, "Run the test suite and report any errors.");
            tv2.grab_focus();
        });
        let tv3 = text_view.clone();
        pill3.connect_clicked(move |_| {
            set_text_view_content(&tv3, "Review recent git commits and pending changes.");
            tv3.grab_focus();
        });
    }

    // Stream consumer loop
    {
        let active_stream_clone = active_stream.clone();
        let status_dot_clone = status_dot.clone();
        let head_dot_clone = head_dot.clone();
        let state_clone = state.clone();
        let scroll_fn = scroll_to_bottom.clone();
        let btn_send_ref = btn_send_stop.clone();
        let is_streaming_ref = is_streaming.clone();

        glib::spawn_future_local(async move {
            while let Some((gen, event)) = stream_rx.recv().await {
                let st = state_clone.borrow();
                if gen != st.runner.generation() {
                    continue;
                }

                match event {
                    StreamEvent::ThinkingDelta(delta) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let mut p_opt = target.progress.borrow_mut();
                            if p_opt.is_none() {
                                let expander = gtk4::Expander::builder()
                                    .label("Thinking... ▾")
                                    .css_classes(["progress-steps-disclosure"])
                                    .expanded(true)
                                    .build();
                                let steps_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                                steps_box.add_css_class("steps-list-box");
                                expander.set_child(Some(&steps_box));
                                target.turn_body.append(&expander);

                                *p_opt = Some(ProgressDisclosureState {
                                    expander,
                                    steps_box,
                                    steps_count: 0,
                                    start_time: std::time::Instant::now(),
                                    items: HashMap::new(),
                                });
                            }
                            if let Some(ref mut p) = *p_opt {
                                let elapsed = p.start_time.elapsed().as_secs();
                                p.expander.set_label(Some(&format!("Thinking... ({}s) ▾", elapsed.max(1))));
                            }
                            target.full_text.borrow_mut().push_str(&delta);
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolStart { id, name, args } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let mut p_opt = target.progress.borrow_mut();
                            if p_opt.is_none() {
                                let expander = gtk4::Expander::builder()
                                    .label("Running steps... ▾")
                                    .css_classes(["progress-steps-disclosure"])
                                    .expanded(true)
                                    .build();
                                let steps_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                                steps_box.add_css_class("steps-list-box");
                                expander.set_child(Some(&steps_box));
                                target.turn_body.append(&expander);

                                *p_opt = Some(ProgressDisclosureState {
                                    expander,
                                    steps_box,
                                    steps_count: 0,
                                    start_time: std::time::Instant::now(),
                                    items: HashMap::new(),
                                });
                            }

                            if let Some(ref mut p) = *p_opt {
                                p.steps_count += 1;
                                let elapsed = p.start_time.elapsed().as_secs();
                                p.expander.set_label(Some(&format!(
                                    "Running step {} ({}s) ▾",
                                    p.steps_count,
                                    format::format_duration_sec(elapsed)
                                )));

                                let item_card = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                                item_card.add_css_class("step-item-card");

                                let icon_str = match name.as_str() {
                                    "bash" | "powershell" => "⚙",
                                    "read" => "📄",
                                    "edit" | "write" => "✏",
                                    _ => "⚡",
                                };
                                let icon_lbl = gtk4::Label::builder()
                                    .label(icon_str)
                                    .css_classes(["step-item-icon"])
                                    .build();
                                item_card.append(&icon_lbl);

                                let action_lbl = format::format_tool_call_label(&name, &args);
                                let name_lbl = gtk4::Label::builder()
                                    .label(&action_lbl)
                                    .css_classes(["step-item-label"])
                                    .ellipsize(gtk4::pango::EllipsizeMode::End)
                                    .max_width_chars(36)
                                    .hexpand(true)
                                    .halign(gtk4::Align::Start)
                                    .build();
                                item_card.append(&name_lbl);

                                let badge = gtk4::Label::builder()
                                    .label("RUNNING")
                                    .css_classes(["step-item-status", "running"])
                                    .build();
                                item_card.append(&badge);

                                p.steps_box.append(&item_card);
                                p.items.insert(
                                    id,
                                    StepItemEntry {
                                        badge,
                                        start_time: std::time::Instant::now(),
                                    },
                                );
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolEnd {
                        id,
                        name: _,
                        result: _,
                        is_error,
                        duration_ms,
                    } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let mut p_opt = target.progress.borrow_mut();
                            if let Some(ref mut p) = *p_opt {
                                if let Some(item) = p.items.get_mut(&id) {
                                    let dur_s = if duration_ms > 0 {
                                        duration_ms / 1000
                                    } else {
                                        item.start_time.elapsed().as_secs()
                                    };
                                    item.badge.remove_css_class("running");
                                    if is_error {
                                        item.badge.add_css_class("error");
                                        item.badge.set_text(&format!(
                                            "ERROR ({})",
                                            format::format_duration_sec(dur_s)
                                        ));
                                    } else {
                                        item.badge.set_text(&format!(
                                            "DONE ({})",
                                            format::format_duration_sec(dur_s)
                                        ));
                                    }
                                }
                                let total_elapsed = p.start_time.elapsed().as_secs();
                                p.expander.set_label(Some(&format!(
                                    "Completed {} steps in {} ▾",
                                    p.steps_count,
                                    format::format_duration_sec(total_elapsed)
                                )));
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolCall { name, args } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let mut p_opt = target.progress.borrow_mut();
                            if p_opt.is_none() {
                                let expander = gtk4::Expander::builder()
                                    .label("Steps ▾")
                                    .css_classes(["progress-steps-disclosure"])
                                    .expanded(true)
                                    .build();
                                let steps_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                                steps_box.add_css_class("steps-list-box");
                                expander.set_child(Some(&steps_box));
                                target.turn_body.append(&expander);

                                *p_opt = Some(ProgressDisclosureState {
                                    expander,
                                    steps_box,
                                    steps_count: 0,
                                    start_time: std::time::Instant::now(),
                                    items: HashMap::new(),
                                });
                            }
                            if let Some(ref mut p) = *p_opt {
                                p.steps_count += 1;
                                let item_card = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                                item_card.add_css_class("step-item-card");

                                let action_lbl = format::format_tool_call_label(&name, &args);
                                let name_lbl = gtk4::Label::builder()
                                    .label(&action_lbl)
                                    .css_classes(["step-item-label"])
                                    .ellipsize(gtk4::pango::EllipsizeMode::End)
                                    .max_width_chars(36)
                                    .hexpand(true)
                                    .halign(gtk4::Align::Start)
                                    .build();
                                item_card.append(&name_lbl);

                                let badge = gtk4::Label::builder()
                                    .label("DONE")
                                    .css_classes(["step-item-status"])
                                    .build();
                                item_card.append(&badge);
                                p.steps_box.append(&item_card);
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::TextDelta(delta) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
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
                        btn_send_ref.set_label("↑");
                        btn_send_ref.remove_css_class("stop");
                        btn_send_ref.set_tooltip_text(Some("Send prompt (Enter)"));
                        is_streaming_ref.set(false);
                        scroll_fn();
                    }
                    StreamEvent::Done => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            if let Some(ref p) = *target.progress.borrow() {
                                let total_elapsed = p.start_time.elapsed().as_secs();
                                p.expander.set_label(Some(&format!(
                                    "Completed {} steps in {} ▾",
                                    p.steps_count,
                                    format::format_duration_sec(total_elapsed)
                                )));
                            }
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
                        btn_send_ref.set_label("↑");
                        btn_send_ref.remove_css_class("stop");
                        btn_send_ref.set_tooltip_text(Some("Send prompt (Enter)"));
                        is_streaming_ref.set(false);
                        scroll_fn();
                    }
                }
            }
        });
    }

    // Submit Prompt handler
    let submit_prompt = {
        let tv = text_view.clone();
        let messages_box = messages_box.clone();
        let welcome_box = welcome_box.clone();
        let prompts_list_box = prompts_list_box.clone();
        let prompt_empty_label = prompt_empty_label.clone();
        let popover_session = popover_session.clone();
        let state = state.clone();
        let active_stream = active_stream.clone();
        let status_dot = status_dot.clone();
        let head_dot = head_dot.clone();
        let stream_tx = stream_tx.clone();
        let scroll_fn = scroll_to_bottom.clone();
        let scroll_to_widget = scroll_to_widget.clone();
        let is_streaming_submit = is_streaming.clone();
        let btn_send_submit = btn_send_stop.clone();
        let session_title_submit = session_title_label.clone();

        Rc::new(move || {
            let raw_prompt = get_text_view_content(&tv);
            if raw_prompt.is_empty() {
                return;
            }
            let prompt = commands::expand_slash_command(&raw_prompt).unwrap_or(raw_prompt.clone());

            // Clear draft from buffer on send
            set_text_view_content(&tv, "");
            welcome_box.set_visible(false);

            let prompt_idx = state.borrow().prompts.len() + 1;

            // Update session title in subheader
            let snippet = if prompt.chars().count() > 32 {
                format!("{}…", prompt.chars().take(32).collect::<String>())
            } else {
                prompt.clone()
            };
            session_title_submit.set_label(&format!("Local  •  {}", snippet));

            // 1. User Message Card (Right-aligned bubble)
            let user_card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            user_card.add_css_class("card-user");
            user_card.set_halign(gtk4::Align::End);

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
            asst_card.set_halign(gtk4::Align::Fill);

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

            asst_card.append(&asst_header);

            let turn_body = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
            asst_card.append(&turn_body);

            // Assistant Feedback & Action Bar
            let feedback_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            feedback_bar.add_css_class("card-feedback-bar");

            let btn_retry = gtk4::Button::builder()
                .label("↻")
                .css_classes(["feedback-btn"])
                .tooltip_text("Retry prompt")
                .build();

            let btn_copy = gtk4::Button::builder()
                .label("📋")
                .css_classes(["feedback-btn"])
                .tooltip_text("Copy response")
                .build();

            let btn_thumb_up = gtk4::Button::builder()
                .label("👍")
                .css_classes(["feedback-btn"])
                .tooltip_text("Helpful")
                .build();

            let btn_thumb_down = gtk4::Button::builder()
                .label("👎")
                .css_classes(["feedback-btn"])
                .tooltip_text("Not helpful")
                .build();

            feedback_bar.append(&btn_retry);
            feedback_bar.append(&btn_copy);
            feedback_bar.append(&btn_thumb_up);
            feedback_bar.append(&btn_thumb_down);
            asst_card.append(&feedback_bar);

            messages_box.append(&asst_card);

            let full_text = Rc::new(RefCell::new(String::new()));

            // Copy button feedback
            {
                let copy_text_ref = full_text.clone();
                let copy_btn_clone = btn_copy.clone();
                btn_copy.connect_clicked(move |_| {
                    let content = copy_text_ref.borrow().clone();
                    if let Some(display) = gdk::Display::default() {
                        display.clipboard().set_text(&content);
                    }
                    copy_btn_clone.set_label("✓");
                    copy_btn_clone.add_css_class("copied");
                    let reset_btn = copy_btn_clone.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                        reset_btn.set_label("📋");
                        reset_btn.remove_css_class("copied");
                    });
                });
            }

            // Retry button
            {
                let retry_prompt = prompt.clone();
                let tv_retry = tv.clone();
                btn_retry.connect_clicked(move |_| {
                    set_text_view_content(&tv_retry, &retry_prompt);
                    tv_retry.grab_focus();
                });
            }

            // Thumbs up / down interaction
            {
                let down_clone = btn_thumb_down.clone();
                btn_thumb_up.connect_clicked(move |btn| {
                    if btn.has_css_class("active") {
                        btn.remove_css_class("active");
                    } else {
                        btn.add_css_class("active");
                        down_clone.remove_css_class("active");
                    }
                });

                let up_clone = btn_thumb_up.clone();
                btn_thumb_down.connect_clicked(move |btn| {
                    if btn.has_css_class("active") {
                        btn.remove_css_class("active");
                    } else {
                        btn.add_css_class("active");
                        up_clone.remove_css_class("active");
                    }
                });
            }

            // 3. Track in Session History Popover
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
            let pop_ref = popover_session.clone();
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

            // Per-item prompt deletion with safety confirmation
            {
                let row_ref = row_box.clone();
                let u_card = user_card.clone();
                let a_card = asst_card.clone();
                let msg_box_ref = messages_box.clone();
                let p_list_box_ref = prompts_list_box.clone();
                let p_empty_lbl = prompt_empty_label.clone();
                let wel_box = welcome_box.clone();
                let st_ref = state.clone();
                let active_stream_ref = active_stream.clone();
                let status_dot_ref = status_dot.clone();
                let head_dot_ref = head_dot.clone();
                let is_str_ref = is_streaming_submit.clone();
                let btn_s_ref = btn_send_submit.clone();
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
                        is_str_ref.set(false);
                        btn_s_ref.set_label("↑");
                        btn_s_ref.remove_css_class("stop");
                        btn_s_ref.set_tooltip_text(Some("Send (Enter)"));
                        status_dot_ref.remove_css_class("working");
                        head_dot_ref.remove_css_class("working");
                    }
                    st.prompts.retain(|p| p.index != this_idx);
                    let remaining = st.prompts.len();

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
                user_card: user_card.clone(),
            });

            *active_stream.borrow_mut() = Some(ActiveStreamTarget {
                turn_body: turn_body.clone(),
                full_text: full_text.clone(),
                progress: Rc::new(RefCell::new(None)),
                active_markdown: Rc::new(RefCell::new(None)),
                metrics_badge: metrics_badge.clone(),
            });

            scroll_fn();
            status_dot.remove_css_class("error");
            status_dot.add_css_class("working");
            head_dot.add_css_class("working");

            // Switch Send button to Stop button during active stream
            is_streaming_submit.set(true);
            btn_send_submit.set_label("⏹");
            btn_send_submit.add_css_class("stop");
            btn_send_submit.set_tooltip_text(Some("Stop Generation"));

            let mut st = state.borrow_mut();
            let model_opt = Some(st.settings.model.clone());
            let thinking_opt = Some(st.settings.thinking_level.clone());
            if let Err(e) =
                st.runner
                    .spawn_prompt(prompt.clone(), model_opt, thinking_opt, stream_tx.clone())
            {
                // On failure, restore prompt to buffer so user doesn't lose draft
                set_text_view_content(&tv, &prompt);
                let err_str = format!("[Error] Failed to spawn agent: {}", e);
                full_text.borrow_mut().push_str(&err_str);
                let err_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                turn_body.append(&err_box);
                crate::render::render_markdown_blocks_into(&err_box, &err_str);
                status_dot.remove_css_class("working");
                status_dot.add_css_class("error");
                head_dot.remove_css_class("working");
                is_streaming_submit.set(false);
                btn_send_submit.set_label("↑");
                btn_send_submit.remove_css_class("stop");
                btn_send_submit.set_tooltip_text(Some("Send (Enter)"));
                scroll_fn();
            }
        })
    };

    // Connect Subheader Back Button to scroll to previous turn
    {
        let state_back = state.clone();
        let scroll_widget_back = scroll_to_widget.clone();
        btn_back.connect_clicked(move |_| {
            let st = state_back.borrow();
            if let Some(prev) = st.prompts.iter().rev().nth(1) {
                scroll_widget_back(&prev.user_card);
            } else if let Some(first) = st.prompts.first() {
                scroll_widget_back(&first.user_card);
            }
        });
    }

    // Keyboard Controller on Multi-line TextView: Enter sends, Shift+Enter new line
    {
        let key_controller = gtk4::EventControllerKey::new();
        let submit_on_enter = submit_prompt.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
            if keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter {
                if modifier.contains(gdk::ModifierType::SHIFT_MASK) {
                    glib::Propagation::Proceed
                } else {
                    submit_on_enter();
                    glib::Propagation::Stop
                }
            } else {
                glib::Propagation::Proceed
            }
        });
        text_view.add_controller(key_controller);
    }

    // Connect Send / Stop Button
    {
        let is_str_btn = is_streaming.clone();
        let state_stop = state.clone();
        let submit_btn = submit_prompt.clone();
        let active_stream_stop = active_stream.clone();
        let status_dot_stop = status_dot.clone();
        let head_dot_stop = head_dot.clone();
        let btn_send_ref = btn_send_stop.clone();

        btn_send_stop.connect_clicked(move |_| {
            if is_str_btn.get() {
                // Active streaming: Stop request
                state_stop.borrow_mut().runner.abort();
                *active_stream_stop.borrow_mut() = None;
                is_str_btn.set(false);
                btn_send_ref.set_label("↑");
                btn_send_ref.remove_css_class("stop");
                btn_send_ref.set_tooltip_text(Some("Send (Enter)"));
                status_dot_stop.remove_css_class("working");
                head_dot_stop.remove_css_class("working");
            } else {
                submit_btn();
            }
        });
    }

    // Global Window Keyboard Shortcuts (Esc to close, Ctrl+N for new chat)
    {
        let key_controller = gtk4::EventControllerKey::new();
        let hide_fn = hide_overlay_fn.clone();
        let do_clear = execute_clear.clone();
        let tv_ref = text_view.clone();

        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            if key == gdk::Key::Escape {
                hide_fn();
                return glib::Propagation::Stop;
            }
            if modifier.contains(gdk::ModifierType::CONTROL_MASK) && key == gdk::Key::n {
                do_clear();
                tv_ref.grab_focus();
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

    // Start IPC socket listener
    let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel::<ipc::IpcCommand>();
    ipc::start_ipc_listener(ipc_tx);

    let expand_fn_ipc = expand_overlay_fn.clone();
    let hide_fn_ipc = hide_overlay_fn.clone();
    let state_ipc = state.clone();
    let tv_ipc = text_view.clone();
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
                    set_text_view_content(&tv_ipc, &text);
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
    }
}
