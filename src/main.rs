pub mod acp;
#[cfg(feature = "audit")]
mod audit;
mod backend;
pub mod capture;
mod commands;
mod format;
mod icons;
mod ipc;
mod markup;
mod mentions;
mod menu;
mod overlay;
mod render;
mod settings;
mod tray;

use backend::StreamEvent;
use gtk4::gdk;
use gtk4::glib::{self, LogWriterOutput};
use gtk4::prelude::*;
use settings::AppSettings;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::mpsc;

const APP_ID: &str = "com.github.khoipm.pi-overlay";

pub fn model_display_name(model_id: &str) -> String {
    let models = backend::query_pi_models_sync();
    model_display_name_with_list(model_id, &models)
}

pub fn model_display_name_with_list(model_id: &str, available_models: &[(String, String)]) -> String {
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
    #[allow(dead_code)]
    index: usize,
    #[allow(dead_code)]
    user_card: gtk4::Box,
}

/// Lightweight VS Code-style collapsible disclosure with chevron icon + left vertical rail.
#[derive(Clone)]
struct Disclosure {
    root: gtk4::Box,
    chevron: gtk4::Image,
    label: gtk4::Label,
    revealer: gtk4::Revealer,
    rail: gtk4::Box,
    expanded: Rc<Cell<bool>>,
}

impl Disclosure {
    fn new(title: &str, expanded: bool) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        root.add_css_class("disclosure-box");

        let btn = gtk4::Button::builder()
            .css_classes(["disclosure-btn"])
            .halign(gtk4::Align::Start)
            .build();
        let hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let chevron = icons::image(
            if expanded {
                "codicon-chevron-down-symbolic"
            } else {
                "codicon-chevron-right-symbolic"
            },
            12,
        );
        let label = gtk4::Label::builder()
            .label(title)
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        hdr.append(&chevron);
        hdr.append(&label);
        btn.set_child(Some(&hdr));

        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(120)
            .reveal_child(expanded)
            .build();
        let rail = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        rail.add_css_class("disclosure-rail");
        revealer.set_child(Some(&rail));

        let expanded_cell = Rc::new(Cell::new(expanded));
        {
            let exp_c = expanded_cell.clone();
            let rev_c = revealer.clone();
            let chev_c = chevron.clone();
            btn.connect_clicked(move |_| {
                let next = !exp_c.get();
                exp_c.set(next);
                rev_c.set_reveal_child(next);
                chev_c.set_icon_name(Some(if next {
                    "codicon-chevron-down-symbolic"
                } else {
                    "codicon-chevron-right-symbolic"
                }));
            });
        }

        root.append(&btn);
        root.append(&revealer);

        Self {
            root,
            chevron,
            label,
            revealer,
            rail,
            expanded: expanded_cell,
        }
    }

    fn set_title(&self, title: &str) {
        self.label.set_text(title);
    }

    fn set_expanded(&self, expanded: bool) {
        self.expanded.set(expanded);
        self.revealer.set_reveal_child(expanded);
        self.chevron.set_icon_name(Some(if expanded {
            "codicon-chevron-down-symbolic"
        } else {
            "codicon-chevron-right-symbolic"
        }));
    }
}

struct StepItemEntry {
    card: gtk4::Box,
    icon: gtk4::Image,
    badge: gtk4::Label,
    start_time: std::time::Instant,
}

struct ProgressDisclosureState {
    disclosure: Disclosure,
    steps_box: gtk4::Box,
    steps_count: usize,
    has_error: bool,
    start_time: std::time::Instant,
    items: HashMap<String, StepItemEntry>,
}

struct ThinkingDisclosureState {
    disclosure: Disclosure,
    text_view: gtk4::TextView,
    buffer: gtk4::TextBuffer,
    content: String,
    start_time: std::time::Instant,
    last_elapsed_sec: u64,
}

struct ActiveStreamTarget {
    turn_body: gtk4::Box,
    full_text: Rc<RefCell<String>>,
    thinking: Rc<RefCell<Option<ThinkingDisclosureState>>>,
    progress: Rc<RefCell<Option<ProgressDisclosureState>>>,
    active_markdown: Rc<RefCell<Option<MarkdownStream>>>,
    metrics_badge: gtk4::Label,
}

/// Live markdown output for one assistant turn.
///
/// `blocks` caches the parse of the previous delta so the renderer can keep
/// finished widgets alive instead of tearing the whole box down per token.
struct MarkdownStream {
    container: gtk4::Box,
    text: String,
    blocks: Vec<render::MarkdownBlock>,
}

impl MarkdownStream {
    fn new(container: gtk4::Box) -> Self {
        Self {
            container,
            text: String::new(),
            blocks: Vec::new(),
        }
    }

    /// Appends a delta and re-renders only the blocks that changed.
    fn push(&mut self, delta: &str) {
        self.text.push_str(delta);
        // `[[markup: ...]]` tags drive the screen overlay, not the transcript,
        // so they are held back until the stream finishes.
        let visible = markup::strip_agent_markup_tags_preserving_open(&self.text);
        render::render_markdown_streaming_into(&self.container, &mut self.blocks, &visible);
    }
}

fn collapse_and_finalize_thinking(target: &ActiveStreamTarget) {
    if let Some(th) = target.thinking.borrow_mut().take() {
        let elapsed = th.start_time.elapsed().as_secs();
        th.disclosure
            .set_title(&format!("Thought for {}s", elapsed.max(1)));
        th.disclosure.set_expanded(false);
    }
}

fn collapse_and_finalize_progress(target: &ActiveStreamTarget) {
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

struct AppState {
    mode: OverlayMode,
    harness: backend::Harness,
    prompts: Vec<TrackedPrompt>,
    settings: AppSettings,
    current_mode: String,
    top_margin: i32,
    bottom_margin: i32,
    is_window_mode: bool,
    attached_files: Vec<(String, String)>,
    /// Model catalog for the active harness (Pi presets or ACP sessions).
    available_models: Vec<(String, String)>,
    /// ACP mode catalog; empty for the Pi harness.
    available_modes: Vec<(String, String)>,
    /// Thinking levels offered by the active harness.
    available_thinking_levels: Vec<String>,
    /// Slash commands offered by the `/` autocomplete.
    slash_commands: Rc<RefCell<Vec<mentions::SlashCommand>>>,
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

/// Short label for a workspace path: its final component, or "Workspace".
fn workspace_label(dir: &str) -> String {
    let trimmed = dir.trim_end_matches('/');
    match trimmed.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => "Workspace".to_string(),
    }
}

/// Builds a compact flat picker button displaying `label` + Codicon chevron-down.
fn make_picker_button(
    initial_label: &str,
    css_class: &str,
    tooltip: &str,
) -> (gtk4::Button, gtk4::Label) {
    let btn = gtk4::Button::builder()
        .css_classes([css_class])
        .tooltip_text(tooltip)
        .build();
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    let lbl = gtk4::Label::builder().label(initial_label).build();
    let chev = icons::image("codicon-chevron-down-symbolic", 11);
    chev.set_opacity(0.75);
    row.append(&lbl);
    row.append(&chev);
    btn.set_child(Some(&row));
    (btn, lbl)
}

fn set_text_view_content(tv: &gtk4::TextView, text: &str) {
    tv.buffer().set_text(text);
}

fn add_context_chip(
    chips_box: &gtk4::Box,
    display_name: &str,
    full_path: &str,
    state: Rc<RefCell<AppState>>,
    on_change: Option<Rc<dyn Fn()>>,
) {
    let chip = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    chip.add_css_class("composer-chip");
    let is_img = capture::is_image_path(full_path);
    if is_img {
        chip.add_css_class("image-chip");
    }

    let chip_icon = icons::image(
        if is_img {
            "codicon-device-camera-symbolic"
        } else {
            "codicon-file-code-symbolic"
        },
        11,
    );
    chip_icon.add_css_class("composer-chip-icon");

    let chip_text = gtk4::Label::builder()
        .label(display_name)
        .css_classes(["composer-chip-text"])
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(24)
        .tooltip_text(full_path)
        .build();
    let chip_del = icons::icon_button(
        "codicon-close-symbolic",
        10,
        "Remove context chip",
        &["composer-chip-del"],
    );

    chip.append(&chip_del);
    chip.append(&chip_icon);
    chip.append(&chip_text);

    let chips_box_clone = chips_box.clone();
    let chip_clone = chip.clone();
    let on_change_clone = on_change.clone();
    let path_clone = full_path.to_string();
    chip_del.connect_clicked(move |_| {
        chips_box_clone.remove(&chip_clone);
        state.borrow_mut().attached_files.retain(|(p, _)| p != &path_clone);
        if let Some(ref cb) = on_change_clone {
            cb();
        }
    });

    chips_box.append(&chip);
    if let Some(ref cb) = on_change {
        cb();
    }
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
    #[cfg(feature = "audit")]
    let audit_dir: Option<String> = args
        .iter()
        .position(|a| a == "--audit")
        .and_then(|i| args.get(i + 1))
        .cloned();
    #[cfg(not(feature = "audit"))]
    let audit_dir: Option<String> = None;

    if audit_dir.is_none() && ipc::try_send_toggle() {
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

    let is_window_mode = args.contains(&"--window".to_string()) || audit_dir.is_some();
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
            let scale_provider = load_css();
            let ctx = build_ui(
                application,
                is_window_mode,
                is_daemon,
                scale_provider,
                audit_dir.clone(),
            );
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
            .args(["-f", home.join(".local/share/icons/hicolor").to_str().unwrap_or("")])
            .status();
    }
}

fn load_css() -> gtk4::CssProvider {
    let provider = gtk4::CssProvider::new();
    let css_bytes = include_bytes!("../resources/style.css");
    provider.load_from_data(std::str::from_utf8(css_bytes).unwrap_or(""));

    let scale_provider = gtk4::CssProvider::new();

    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );
        gtk4::style_context_add_provider_for_display(
            &display,
            &scale_provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER + 10,
        );
    }

    scale_provider
}

fn build_ui(
    app: &gtk4::Application,
    is_window_mode: bool,
    is_daemon: bool,
    scale_css_provider: gtk4::CssProvider,
    #[allow(unused_variables)] audit_dir: Option<String>,
) -> AppContext {
    let hold_guard = app.hold();
    let initial_settings = AppSettings::load();
    let initial_models = backend::query_pi_models_sync();

    settings::apply_text_scale(
        &scale_css_provider,
        initial_settings.ui_scale,
        initial_settings.font_size,
    );

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("Chat")
        .default_width(initial_settings.width)
        .build();

    overlay::init_layer_shell(&window, !is_window_mode, &initial_settings.edge);
    if is_window_mode {
        window.set_decorated(true);
    }

    // Root overlay container
    let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root_box.set_halign(gtk4::Align::End);
    root_box.set_vexpand(true);
    root_box.set_hexpand(false);

    // Left Resize Grip (thin handle)
    let left_col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    left_col.set_width_request(3);
    left_col.set_vexpand(true);

    let resize_handle_w = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    resize_handle_w.add_css_class("resize-handle");
    resize_handle_w.set_cursor_from_name(Some("ew-resize"));
    resize_handle_w.set_vexpand(true);
    left_col.append(&resize_handle_w);
    root_box.append(&left_col);

    // Main Drawer Card Container (Sidebar)
    let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    drawer.set_widget_name("drawer-container");
    drawer.add_css_class("drawer-container");
    drawer.set_width_request(initial_settings.width);
    drawer.set_vexpand(true);
    drawer.set_hexpand(false);

    // ── 1. Panel Header (VS Code 35px View Header) ──────────────────────────
    let panel_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    panel_header.add_css_class("panel-header");
    panel_header.set_hexpand(false);

    let panel_title = gtk4::Label::builder()
        .label("CHAT")
        .css_classes(["panel-title"])
        .halign(gtk4::Align::Start)
        .build();
    panel_header.append(&panel_title);

    let header_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    header_spacer.set_hexpand(true);
    panel_header.append(&header_spacer);

    let btn_new_chat = icons::icon_button(
        "codicon-add-symbolic",
        15,
        "New Chat (Ctrl+N)",
        &["header-icon-btn"],
    );
    panel_header.append(&btn_new_chat);

    let btn_session_list = icons::icon_button(
        "codicon-history-symbolic",
        15,
        "Show Chats / History",
        &["header-icon-btn"],
    );
    panel_header.append(&btn_session_list);

    let btn_more = icons::icon_button(
        "codicon-settings-gear-symbolic",
        15,
        "Settings & Session Options",
        &["header-icon-btn"],
    );
    panel_header.append(&btn_more);

    let btn_close = icons::icon_button(
        "codicon-close-symbolic",
        15,
        "Close Overlay (Esc)",
        &["header-icon-btn", "close"],
    );
    panel_header.append(&btn_close);

    drawer.append(&panel_header);

    // ── 2. Session Sub-Header (workspace + harness) ─────────────────────────
    let session_subheader = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    session_subheader.add_css_class("session-subheader");

    let btn_workspace = gtk4::Button::builder()
        .css_classes(["session-chip-btn"])
        .tooltip_text(&initial_settings.workspace_dir)
        .build();
    let ws_inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    ws_inner.append(&icons::image("codicon-folder-symbolic", 12));
    let lbl_workspace = gtk4::Label::builder()
        .label(workspace_label(&initial_settings.workspace_dir))
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(20)
        .build();
    ws_inner.append(&lbl_workspace);
    btn_workspace.set_child(Some(&ws_inner));
    session_subheader.append(&btn_workspace);

    session_subheader.append(
        &gtk4::Label::builder()
            .label("/")
            .css_classes(["session-sep"])
            .build(),
    );

    let (btn_harness, lbl_harness) = make_picker_button(
        initial_settings.harness.label(),
        "session-chip-btn",
        "Switch agent harness",
    );
    session_subheader.append(&btn_harness);

    let subheader_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    subheader_spacer.set_hexpand(true);
    session_subheader.append(&subheader_spacer);

    let session_title_label = gtk4::Label::builder()
        .label("")
        .css_classes(["session-title-label"])
        .halign(gtk4::Align::End)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(22)
        .build();
    session_subheader.append(&session_title_label);

    drawer.append(&session_subheader);

    // ── 3. Chat Content Stack (Centered Welcome vs Scrollable Stream) ──────
    let chat_stack = gtk4::Stack::new();
    chat_stack.set_vexpand(true);
    chat_stack.set_hexpand(true);

    let welcome_page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    welcome_page.set_vexpand(true);
    welcome_page.set_hexpand(true);
    welcome_page.add_css_class("messages-container");

    let welcome_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    welcome_box.add_css_class("welcome-hero");
    welcome_box.set_halign(gtk4::Align::Center);
    welcome_box.set_valign(gtk4::Align::Center);
    welcome_box.set_vexpand(true);

    let glyph = icons::image("codicon-sparkle-symbolic", 42);
    glyph.add_css_class("welcome-glyph");
    glyph.set_halign(gtk4::Align::Center);
    welcome_box.append(&glyph);

    let w_title = gtk4::Label::builder()
        .label("Build with agent")
        .css_classes(["welcome-title"])
        .halign(gtk4::Align::Center)
        .build();
    welcome_box.append(&w_title);

    let w_sub = gtk4::Label::builder()
        .label("Ask about your workspace, run edits, or attach live screen context with @screen.")
        .css_classes(["welcome-subtitle"])
        .wrap(true)
        .wrap_mode(gtk4::pango::WrapMode::WordChar)
        .justify(gtk4::Justification::Center)
        .max_width_chars(38)
        .halign(gtk4::Align::Center)
        .build();
    welcome_box.append(&w_sub);

    let starters_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    starters_box.set_margin_top(6);
    starters_box.set_width_request(290);
    starters_box.set_halign(gtk4::Align::Center);

    let starter_items = [
        (
            "codicon-sparkle-symbolic",
            "Explain this workspace architecture",
            "Explain this workspace structure and main entrypoints",
        ),
        (
            "codicon-device-camera-symbolic",
            "Inspect what is visible on @screen",
            "@screen What am I looking at? Suggest next steps.",
        ),
        (
            "codicon-terminal-symbolic",
            "Check recent changes and test status",
            "Review recent changes in this repository and summarize test status",
        ),
    ];

    let mut starter_buttons: Vec<(gtk4::Button, &'static str)> = Vec::new();
    for (icon_name, title, prompt_template) in starter_items {
        let s_btn = gtk4::Button::builder()
            .css_classes(["welcome-starter-btn"])
            .halign(gtk4::Align::Fill)
            .build();
        let s_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let s_icon = icons::image(icon_name, 14);
        s_icon.add_css_class("welcome-starter-icon");
        let s_lbl = gtk4::Label::builder()
            .label(title)
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .hexpand(true)
            .build();
        s_row.append(&s_icon);
        s_row.append(&s_lbl);
        s_btn.set_child(Some(&s_row));
        starters_box.append(&s_btn);
        starter_buttons.push((s_btn, prompt_template));
    }
    welcome_box.append(&starters_box);
    welcome_page.append(&welcome_box);
    chat_stack.add_named(&welcome_page, Some("welcome"));

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

    scrolled.set_child(Some(&messages_box));

    let scrolled_overlay = gtk4::Overlay::new();
    scrolled_overlay.set_child(Some(&scrolled));
    scrolled_overlay.set_vexpand(true);

    let btn_scroll_bottom = icons::icon_button(
        "codicon-arrow-down-symbolic",
        13,
        "Scroll to bottom",
        &["btn-scroll-bottom"],
    );
    btn_scroll_bottom.set_halign(gtk4::Align::End);
    btn_scroll_bottom.set_valign(gtk4::Align::End);
    btn_scroll_bottom.set_margin_end(14);
    btn_scroll_bottom.set_margin_bottom(10);
    btn_scroll_bottom.set_visible(false);
    scrolled_overlay.add_overlay(&btn_scroll_bottom);

    chat_stack.add_named(&scrolled_overlay, Some("messages"));
    chat_stack.set_visible_child_name("welcome");
    drawer.append(&chat_stack);

    // ── 4. Unified VS Code Composer (.chat-input-container) ─────────────────
    let composer_outer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    composer_outer.add_css_class("composer-outer");

    let composer_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    composer_box.add_css_class("chat-input-container");

    // Top Context Row (Add Context button + attached context chips)
    let context_top_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    context_top_row.add_css_class("composer-context-row");

    let btn_attach = gtk4::Button::builder()
        .css_classes(["add-context-btn"])
        .tooltip_text("Add file or context")
        .build();
    let add_ctx_inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    add_ctx_inner.append(&icons::image("codicon-add-symbolic", 11));
    add_ctx_inner.append(&gtk4::Label::new(Some("Add Context…")));
    btn_attach.set_child(Some(&add_ctx_inner));
    context_top_row.append(&btn_attach);

    let chips_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    chips_row.add_css_class("composer-chips-row");
    let chips_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .propagate_natural_height(true)
        .hexpand(true)
        .child(&chips_row)
        .build();
    context_top_row.append(&chips_scroll);
    composer_box.append(&context_top_row);

    // Middle: Multi-line TextView with placeholder overlay
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
        .min_content_height(34)
        .max_content_height(130)
        .propagate_natural_height(true)
        .child(&text_view)
        .build();
    input_overlay.set_child(Some(&input_scroll));

    let placeholder_label = gtk4::Label::builder()
        .label("Ask anything, @ to mention, / for commands")
        .css_classes(["composer-placeholder"])
        .halign(gtk4::Align::Start)
        .valign(gtk4::Align::Start)
        .margin_start(4)
        .margin_top(4)
        .can_target(false)
        .build();
    input_overlay.add_overlay(&placeholder_label);
    composer_box.append(&input_overlay);

    // Bottom Toolbar inside the unified card
    let composer_toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    composer_toolbar.add_css_class("composer-toolbar");

    let initial_mode_name = mode_display_name(&initial_settings.acp_mode);
    let (btn_mode_pill, lbl_mode_pill) = make_picker_button(
        &initial_mode_name,
        "composer-picker-btn",
        "Switch Agent Mode",
    );
    composer_toolbar.append(&btn_mode_pill);

    let initial_model_name =
        model_display_name_with_list(&initial_settings.model, &initial_models);
    let (btn_model_pill, lbl_model_pill) = make_picker_button(
        &initial_model_name,
        "composer-picker-btn",
        "Select Language Model",
    );
    composer_toolbar.append(&btn_model_pill);

    let initial_think_name = thinking_display_name(&initial_settings.thinking_level);
    let (btn_think_pill, lbl_think_pill) = make_picker_button(
        &initial_think_name,
        "composer-picker-btn",
        "Reasoning Depth",
    );
    composer_toolbar.append(&btn_think_pill);

    let tb_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    tb_spacer.set_hexpand(true);
    composer_toolbar.append(&tb_spacer);

    let btn_capture = icons::icon_button(
        "codicon-device-camera-symbolic",
        14,
        "Capture screen context (Ctrl+Shift+S)",
        &["composer-tool-btn"],
    );
    composer_toolbar.append(&btn_capture);

    let btn_pin_screen = gtk4::ToggleButton::builder()
        .child(&icons::image("codicon-eye-symbolic", 14))
        .css_classes(["composer-tool-btn"])
        .tooltip_text("Pin live @screen screenshot to every prompt")
        .active(initial_settings.pin_live_screen)
        .build();
    if initial_settings.pin_live_screen {
        btn_pin_screen.add_css_class("active");
    }
    composer_toolbar.append(&btn_pin_screen);

    let btn_send_stop = icons::icon_button(
        "codicon-arrow-up-symbolic",
        13,
        "Send (Enter)",
        &["composer-send-btn"],
    );
    composer_toolbar.append(&btn_send_stop);

    composer_box.append(&composer_toolbar);
    composer_outer.append(&composer_box);
    drawer.append(&composer_outer);

    for (s_btn, prompt_template) in starter_buttons {
        let tv_starter = text_view.clone();
        s_btn.connect_clicked(move |_| {
            set_text_view_content(&tv_starter, prompt_template);
            let buf = tv_starter.buffer();
            buf.place_cursor(&buf.end_iter());
            tv_starter.grab_focus();
        });
    }

    let is_streaming = Rc::new(Cell::new(false));

    let update_send_btn_state = {
        let tv = text_view.clone();
        let btn = btn_send_stop.clone();
        let is_str = is_streaming.clone();
        let chips = chips_row.clone();
        Rc::new(move || {
            if is_str.get() {
                btn.set_sensitive(true);
                icons::set_button_icon(&btn, "codicon-debug-stop-symbolic", 12);
                btn.add_css_class("stop");
                btn.set_tooltip_text(Some("Stop Generation"));
            } else {
                let has_text = !get_text_view_content(&tv).is_empty();
                let has_chips = chips.first_child().is_some();
                let can_send = has_text || has_chips;
                btn.set_sensitive(can_send);
                icons::set_button_icon(&btn, "codicon-arrow-up-symbolic", 13);
                btn.remove_css_class("stop");
                btn.set_tooltip_text(Some(if can_send {
                    "Send (Enter)"
                } else {
                    "Type a prompt or attach context to send"
                }));
            }
        })
    };

    {
        let pl_ref = placeholder_label.clone();
        let update_btn = update_send_btn_state.clone();
        text_view.buffer().connect_changed(move |buf| {
            let empty = buf.char_count() == 0;
            pl_ref.set_visible(empty);
            update_btn();
        });
    }

    // ── @ / # / / autocomplete ──────────────────────────────────────────────
    // Built before the state so both share one index and one command list.
    let workspace_index: Rc<RefCell<mentions::WorkspaceIndex>> = Rc::new(RefCell::new(
        mentions::WorkspaceIndex::build(std::path::Path::new(&initial_settings.workspace_dir)),
    ));
    let slash_commands: Rc<RefCell<Vec<mentions::SlashCommand>>> =
        Rc::new(RefCell::new(mentions::builtin_commands()));

    let menu_host = menu::FloatingMenuHost::new(&drawer);

    let mention_popup = {
        let pop_outer = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        pop_outer.set_hexpand(true);

        let header_lbl = gtk4::Label::builder()
            .label("SUGGESTIONS")
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build();
        pop_outer.append(&header_lbl);

        let list = gtk4::ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::Single);
        list.add_css_class("mention-list");
        list.set_hexpand(true);

        let scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .max_content_height(240)
            .propagate_natural_height(true)
            .child(&list)
            .build();
        pop_outer.append(&scroll);

        let popup = Rc::new(RefCell::new(MentionPopup {
            menu_host: menu_host.clone(),
            composer_widget: composer_box.clone().upcast(),
            container: pop_outer,
            header_lbl,
            list: list.clone(),
            items: Vec::new(),
            selected: 0,
            replace_range: None,
            text_view: text_view.clone(),
            workspace_index: workspace_index.clone(),
            commands: slash_commands.clone(),
        }));

        // Clicking a row accepts it, matching keyboard selection.
        {
            let popup = popup.clone();
            list.connect_row_activated(move |_, row| {
                popup.borrow_mut().selected = row.index().max(0) as usize;
                popup.borrow_mut().accept();
            });
        }

        // Typing drives the suggestion list.
        {
            let popup = popup.clone();
            text_view.buffer().connect_changed(move |_| {
                popup.borrow_mut().refresh();
            });
        }

        popup
    };

    // Initial button state calculation
    update_send_btn_state();

    root_box.append(&menu_host.overlay);
    window.set_child(Some(&root_box));

    let state = Rc::new(RefCell::new(AppState {
        mode: OverlayMode::Hidden,
        harness: backend::Harness::from_settings(initial_settings.harness, &initial_settings),
        prompts: Vec::new(),
        settings: initial_settings.clone(),
        current_mode: initial_settings.acp_mode.clone(),
        top_margin: initial_settings.margin_top,
        bottom_margin: initial_settings.margin_top,
        is_window_mode,
        attached_files: Vec::new(),
        available_models: initial_models.clone(),
        available_modes: Vec::new(),
        available_thinking_levels: Vec::new(),
        slash_commands: slash_commands.clone(),
    }));

    // ── Workspace picker (the agent's cwd) ──────────────────────────────────
    {
        // Pin live screen capture: every prompt carries a fresh screenshot.
        let state_pin = state.clone();
        btn_pin_screen.connect_toggled(move |btn| {
            let active = btn.is_active();
            if active {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
            let mut st = state_pin.borrow_mut();
            st.settings.pin_live_screen = active;
            let _ = st.settings.save();
        });
    }

    {
        let win_ref = window.clone();
        let state_ws = state.clone();
        let btn_ws = btn_workspace.clone();
        let lbl_ws = lbl_workspace.clone();
        let index_ws = workspace_index.clone();
        btn_workspace.connect_clicked(move |_| {
            let chooser = gtk4::FileChooserNative::new(
                Some("Select Workspace Folder"),
                Some(&win_ref),
                gtk4::FileChooserAction::SelectFolder,
                Some("Use Folder"),
                Some("Cancel"),
            );
            chooser.set_modal(true);
            let state_cb = state_ws.clone();
            let btn_cb = btn_ws.clone();
            let lbl_cb = lbl_ws.clone();
            let index_cb = index_ws.clone();
            chooser.connect_response(move |dialog, resp| {
                if resp != gtk4::ResponseType::Accept {
                    return;
                }
                let Some(path) = dialog.file().and_then(|f| f.path()) else {
                    return;
                };
                let dir = path.to_string_lossy().to_string();

                // Rebind the harness and rebuild the file index for the new root.
                let mut st = state_cb.borrow_mut();
                st.settings.workspace_dir = dir.clone();
                st.harness.set_workspace_dir(path.clone());
                let _ = st.settings.save();
                drop(st);

                *index_cb.borrow_mut() = mentions::WorkspaceIndex::build(&path);
                lbl_cb.set_label(&workspace_label(&dir));
                btn_cb.set_tooltip_text(Some(&dir));
            });
            chooser.show();
        });
    }

    // ── Harness switcher ────────────────────────────────────────────────────
    let harness_box = {
        let harness_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        harness_box.set_hexpand(true);
        harness_box.append(
            &gtk4::Label::builder()
                .label("AGENT HARNESS")
                .css_classes(["clean-popover-title"])
                .xalign(0.0)
                .build(),
        );

        for kind in settings::HarnessKind::all() {
            let (label, desc) = match kind {
                settings::HarnessKind::PiJson => (
                    "Pi (JSON)",
                    "pi -p --mode json, one session per conversation",
                ),
                settings::HarnessKind::AgyAcp => (
                    "Antigravity (ACP)",
                    "agy acp over stdio, with tool permissions",
                ),
            };

            let item = gtk4::Button::builder()
                .css_classes(["clean-popover-item"])
                .halign(gtk4::Align::Fill)
                .build();
            let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
            inner.append(&gtk4::Label::builder().label(label).xalign(0.0).build());
            inner.append(
                &gtk4::Label::builder()
                    .label(desc)
                    .css_classes(["clean-popover-item-desc"])
                    .xalign(0.0)
                    .wrap(true)
                    .build(),
            );
            item.set_child(Some(&inner));

            let mh_close = menu_host.clone();
            let lbl_h = lbl_harness.clone();
            let state_h = state.clone();
            let index_h = workspace_index.clone();
            item.connect_clicked(move |_| {
                mh_close.close_menu();
                lbl_h.set_label(kind.label());

                let mut st = state_h.borrow_mut();
                if st.settings.harness == kind {
                    return;
                }
                st.settings.harness = kind;
                let _ = st.settings.save();
                let settings_snapshot = st.settings.clone();
                st.harness.set_kind(kind, &settings_snapshot);
                drop(st);

                // ACP may advertise a different catalog; the Pi list is local.
                if kind == settings::HarnessKind::PiJson {
                    let models = backend::query_pi_models_sync();
                    state_h.borrow_mut().available_models = models;
                    let _ = index_h;
                }
            });
            harness_box.append(&item);
        }

        let mh_ref = menu_host.clone();
        let hb_ref = harness_box.clone();
        btn_harness.connect_clicked(move |btn| {
            mh_ref.toggle_menu("harness", btn, &hb_ref, 240, menu::MenuPlacement::BelowLeft);
        });
        harness_box
    };
    let _ = &harness_box;

    // ── Attach File Context Button ───────────────────────────────────────────
    {
        let win_ref = window.clone();
        let state_attach = state.clone();
        let chips_ref = chips_row.clone();
        let update_btn_attach = update_send_btn_state.clone();
        btn_attach.connect_clicked(move |_| {
            let chooser = gtk4::FileChooserNative::new(
                Some("Attach File as Context"),
                Some(&win_ref),
                gtk4::FileChooserAction::Open,
                Some("Attach"),
                Some("Cancel"),
            );
            chooser.set_modal(true);
            let st_ref = state_attach.clone();
            let c_ref = chips_ref.clone();
            let upd_ref = update_btn_attach.clone();
            chooser.connect_response(move |dialog, resp| {
                if resp == gtk4::ResponseType::Accept {
                    if let Some(file) = dialog.file() {
                        if let Some(path) = file.path() {
                            let path_str = path.to_string_lossy().to_string();
                            let display_name = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("file")
                                .to_string();
                            let content = if capture::is_image_path(&path_str) {
                                std::fs::read(&path)
                                    .map(|b| capture::base64_encode(&b))
                                    .unwrap_or_default()
                            } else {
                                std::fs::read_to_string(&path).unwrap_or_default()
                            };
                            st_ref.borrow_mut().attached_files.push((path_str.clone(), content));
                            add_context_chip(&c_ref, &display_name, &path_str, st_ref.clone(), Some(upd_ref.clone()));
                        }
                    }
                }
            });
            chooser.show();
        });
    }

    // ── Screen Capture Context Button ────────────────────────────────────────
    let capture_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    capture_box.set_hexpand(true);

    let capture_title = gtk4::Label::builder()
        .label("SCREEN CONTEXT")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    capture_box.append(&capture_title);

    // Attaches a captured screenshot to the composer.
    let attach_capture: Rc<dyn Fn(&std::path::Path)> = {
        let state_cap = state.clone();
        let chips_cap = chips_row.clone();
        let upd_cap = update_send_btn_state.clone();
        Rc::new(move |path: &std::path::Path| {
            let path_str = path.to_string_lossy().to_string();
            let display_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("screenshot.png")
                .to_string();
            let content = std::fs::read(path)
                .map(|b| capture::base64_encode(&b))
                .unwrap_or_default();
            state_cap
                .borrow_mut()
                .attached_files
                .push((path_str.clone(), content));
            add_context_chip(
                &chips_cap,
                &display_name,
                &path_str,
                state_cap.clone(),
                Some(upd_cap.clone()),
            );
        })
    };

    // Capture the whole screen (a clean shot: the panel hides for a frame).
    let capture_screen_fn: Rc<dyn Fn()> = {
        let attached = attach_capture.clone();
        let window_cap = window.clone();
        Rc::new(move || {
            let attached = attached.clone();
            capture::capture_screen_hiding_window(&window_cap, move |res| match res {
                Ok(path) => attached(&path),
                Err(err) => eprintln!("Capture failed: {err}"),
            });
        })
    };

    // Capture, then open the annotation editor over the shot.
    let capture_markup_fn: Rc<dyn Fn()> = {
        let attached = attach_capture.clone();
        let window_cap = window.clone();
        let app_cap = app.clone();
        Rc::new(move || {
            let attached = attached.clone();
            let app_cap = app_cap.clone();
            capture::capture_screen_hiding_window(&window_cap, move |res| match res {
                Ok(path) => {
                    markup::open_user_markup_editor(&app_cap, path, move |result| {
                        if let Some(annotated) = result {
                            attached(&annotated);
                        }
                    });
                }
                Err(err) => eprintln!("Capture failed: {err}"),
            });
        })
    };

    let capture_modes = [
        (
            capture::CaptureMode::FullScreen,
            "Full screen",
            "Capture the whole desktop",
        ),
        (
            capture::CaptureMode::Region,
            "Select region",
            "Click & drag the area to attach",
        ),
        (
            capture::CaptureMode::ActiveWindow,
            "Active window",
            "Capture the focused application",
        ),
    ];

    for (mode, m_label, m_desc) in capture_modes {
        let cap_btn = gtk4::Button::builder()
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();
        let cap_inner = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        cap_inner.append(&gtk4::Label::builder().label(m_label).xalign(0.0).build());
        cap_inner.append(
            &gtk4::Label::builder()
                .label(m_desc)
                .css_classes(["clean-popover-item-desc"])
                .xalign(0.0)
                .build(),
        );
        cap_btn.set_child(Some(&cap_inner));

        let mh_cap_close = menu_host.clone();
        let attached = attach_capture.clone();
        let screen_fn = capture_screen_fn.clone();
        let window_mode_cap = window.clone();

        cap_btn.connect_clicked(move |_| {
            mh_cap_close.close_menu();
            match mode {
                capture::CaptureMode::FullScreen => screen_fn(),
                _ => {
                    let attached = attached.clone();
                    capture::capture_mode_hiding_window(&window_mode_cap, mode, move |res| match res {
                        Ok(path) => attached(&path),
                        Err(err) => eprintln!("Capture failed: {err}"),
                    });
                }
            }
        });
        capture_box.append(&cap_btn);
    }

    // Markup entry: capture, annotate, then attach the flattened PNG.
    {
        let markup_btn = gtk4::Button::builder()
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();
        let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        inner.append(&gtk4::Label::builder().label("Annotate screen").xalign(0.0).build());
        inner.append(
            &gtk4::Label::builder()
                .label("Draw boxes / arrows before asking")
                .css_classes(["clean-popover-item-desc"])
                .xalign(0.0)
                .build(),
        );
        markup_btn.set_child(Some(&inner));

        let mh_close = menu_host.clone();
        let markup_fn = capture_markup_fn.clone();
        markup_btn.connect_clicked(move |_| {
            mh_close.close_menu();
            markup_fn();
        });
        capture_box.append(&markup_btn);
    }

    {
        let mh_ref = menu_host.clone();
        let cap_box_ref = capture_box.clone();
        btn_capture.connect_clicked(move |btn| {
            mh_ref.toggle_menu("capture", btn, &cap_box_ref, 240, menu::MenuPlacement::AboveRight);
        });
    }

    // ── Mode Picker Menu ────────────────────────────────────────────────────
    let mode_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    mode_box.set_hexpand(true);

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
            .label(*m_name)
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

        let mh_m_close = menu_host.clone();
        let lbl_mode = lbl_mode_pill.clone();
        let state_m = state.clone();
        let mode_key = m_id.to_string();
        let mode_label_str = (*m_name).to_string();

        m_btn.connect_clicked(move |_| {
            mh_m_close.close_menu();
            lbl_mode.set_label(&mode_label_str);
            let mut st = state_m.borrow_mut();
            st.current_mode = mode_key.clone();
            st.settings.acp_mode = mode_key.clone();
            // Auto mode implies running tools without asking.
            let auto = st.settings.auto_approve_permissions || mode_key == "auto";
            st.harness.set_auto_approve(auto);
            let _ = st.settings.save();
        });
        mode_box.append(&m_btn);
    }

    {
        let mh_ref = menu_host.clone();
        let m_box_ref = mode_box.clone();
        btn_mode_pill.connect_clicked(move |btn| {
            mh_ref.toggle_menu("mode", btn, &m_box_ref, 240, menu::MenuPlacement::AboveLeft);
        });
    }

    // ── Model Picker Menu ───────────────────────────────────────────────────
    let model_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    model_box.set_hexpand(true);

    let model_title = gtk4::Label::builder()
        .label("SELECT MODEL")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    model_box.append(&model_title);

    let model_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .max_content_height(240)
        .propagate_natural_height(true)
        .build();
    let model_list = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

    for (m_id, m_name) in &initial_models {
        let m_btn = gtk4::Button::builder()
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();
        m_btn.set_child(Some(
            &gtk4::Label::builder()
                .label(m_name)
                .xalign(0.0)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .build(),
        ));

        let mh_mdl_close = menu_host.clone();
        let lbl_mdl = lbl_model_pill.clone();
        let state_mdl = state.clone();
        let model_key = m_id.clone();
        let short_name = model_display_name_with_list(m_id, &initial_models);

        m_btn.connect_clicked(move |_| {
            mh_mdl_close.close_menu();
            lbl_mdl.set_label(&short_name);
            let mut st = state_mdl.borrow_mut();
            st.settings.model = model_key.clone();
            // Apply immediately so a live ACP session switches model.
            st.harness.set_config_option("model", &model_key);
            let _ = st.settings.save();
        });
        model_list.append(&m_btn);
    }
    model_scroll.set_child(Some(&model_list));
    model_box.append(&model_scroll);

    {
        let mh_ref = menu_host.clone();
        let mdl_box_ref = model_box.clone();
        btn_model_pill.connect_clicked(move |btn| {
            mh_ref.toggle_menu("model", btn, &mdl_box_ref, 240, menu::MenuPlacement::AboveLeft);
        });
    }

    // ── Thinking Depth Menu ─────────────────────────────────────────────────
    let think_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    think_box.set_hexpand(true);

    let think_title = gtk4::Label::builder()
        .label("REASONING DEPTH")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    think_box.append(&think_title);

    for (th_id, th_name) in THINKING_LEVELS {
        let th_btn = gtk4::Button::builder()
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();
        th_btn.set_child(Some(&gtk4::Label::builder().label(*th_name).xalign(0.0).build()));

        let mh_th_close = menu_host.clone();
        let lbl_th = lbl_think_pill.clone();
        let state_th = state.clone();
        let think_key = th_id.to_string();
        let label_name = th_name.to_string();

        th_btn.connect_clicked(move |_| {
            mh_th_close.close_menu();
            lbl_th.set_label(&label_name);
            let mut st = state_th.borrow_mut();
            st.settings.thinking_level = think_key.clone();
            // Thought level is an ACP config option; Pi passes it as a CLI flag.
            st.harness.set_config_option("thought_level", &think_key);
            st.harness.set_mode(&st.current_mode.clone());
            let _ = st.settings.save();
        });
        think_box.append(&th_btn);
    }

    {
        let mh_ref = menu_host.clone();
        let th_box_ref = think_box.clone();
        btn_think_pill.connect_clicked(move |btn| {
            mh_ref.toggle_menu("think", btn, &th_box_ref, 240, menu::MenuPlacement::AboveLeft);
        });
    }

    // ── Session History Menu ────────────────────────────────────────────────
    let session_stack = gtk4::Stack::new();
    session_stack.set_vhomogeneous(false);
    session_stack.set_hexpand(true);

    let session_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    session_box.set_hexpand(true);

    let session_pop_title = gtk4::Label::builder()
        .label("SESSION HISTORY")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    session_box.append(&session_pop_title);

    let prompts_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .max_content_height(220)
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
    session_stack.add_named(&session_box, Some("list"));

    // Inline confirmation page for deleting an individual session turn.
    type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;
    let pending_prompt_delete: DeferredAction = Rc::new(RefCell::new(None));
    {
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

        let ss_cancel = session_stack.clone();
        let pending_cancel = pending_prompt_delete.clone();
        btn_cancel_item.connect_clicked(move |_| {
            *pending_cancel.borrow_mut() = None;
            ss_cancel.set_visible_child_name("list");
        });

        let ss_confirm = session_stack.clone();
        let pending_confirm = pending_prompt_delete.clone();
        btn_confirm_item.connect_clicked(move |_| {
            if let Some(action) = pending_confirm.borrow_mut().take() {
                action();
            }
            ss_confirm.set_visible_child_name("list");
        });

        session_stack.add_named(&del_box, Some("confirm_delete"));
    }

    {
        let mh_ref = menu_host.clone();
        let ss_ref = session_stack.clone();
        btn_session_list.connect_clicked(move |btn| {
            ss_ref.set_visible_child_name("list");
            mh_ref.toggle_menu(
                "session",
                btn,
                &ss_ref,
                260,
                menu::MenuPlacement::BelowRight,
            );
        });
    }

    // ── Settings Menu (In-Drawer Stack, No Wayland Grabs) ───────────────────
    let more_stack = gtk4::Stack::new();
    more_stack.set_vhomogeneous(false);
    more_stack.set_hexpand(true);

    let more_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    more_box.set_hexpand(true);

    let more_title = gtk4::Label::builder()
        .label("SESSION SETTINGS")
        .css_classes(["clean-popover-title"])
        .xalign(0.0)
        .build();
    more_box.append(&more_title);

    // 1. UI Scale Selector Row
    let btn_row_scale = gtk4::Button::builder()
        .css_classes(["clean-settings-row"])
        .halign(gtk4::Align::Fill)
        .build();
    let box_rs = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let lbl_rs_title = gtk4::Label::builder()
        .label("UI Scale")
        .css_classes(["clean-popover-item-desc"])
        .xalign(0.0)
        .hexpand(true)
        .build();
    let initial_scale_name = settings::UI_SCALE_PRESETS
        .iter()
        .find(|p| p.scale == initial_settings.ui_scale)
        .map(|p| format!("{} ›", p.label))
        .unwrap_or_else(|| "100% ›".to_string());
    let lbl_rs_val = gtk4::Label::builder()
        .label(&initial_scale_name)
        .css_classes(["clean-settings-val"])
        .xalign(1.0)
        .build();
    box_rs.append(&lbl_rs_title);
    box_rs.append(&lbl_rs_val);
    btn_row_scale.set_child(Some(&box_rs));
    more_box.append(&btn_row_scale);

    // Inline UI Scale subpage
    let prs_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    prs_box.set_hexpand(true);
    {
        let back_btn = gtk4::Button::builder()
            .label("← Session Settings")
            .css_classes(["floating-menu-back"])
            .halign(gtk4::Align::Start)
            .build();
        let ms_back = more_stack.clone();
        back_btn.connect_clicked(move |_| ms_back.set_visible_child_name("main"));
        prs_box.append(&back_btn);
    }
    prs_box.append(
        &gtk4::Label::builder()
            .label("UI SCALE PRESET")
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build(),
    );

    for preset in settings::UI_SCALE_PRESETS {
        let s_btn = gtk4::Button::builder()
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();
        s_btn.set_child(Some(
            &gtk4::Label::builder()
                .label(format!("{}  •  {}", preset.label, preset.desc))
                .xalign(0.0)
                .build(),
        ));

        let ms_done = more_stack.clone();
        let state_sc = state.clone();
        let scale_provider_ref = scale_css_provider.clone();
        let sc_val = preset.scale;
        let sc_lbl = preset.label;
        let lbl_val = lbl_rs_val.clone();

        s_btn.connect_clicked(move |_| {
            let mut st = state_sc.borrow_mut();
            st.settings.ui_scale = sc_val;
            settings::apply_text_scale(&scale_provider_ref, sc_val, st.settings.font_size);
            let _ = st.settings.save();
            lbl_val.set_label(&format!("{} ›", sc_lbl));
            ms_done.set_visible_child_name("main");
        });
        prs_box.append(&s_btn);
    }

    {
        let ms_go = more_stack.clone();
        btn_row_scale.connect_clicked(move |_| {
            ms_go.set_visible_child_name("scale");
        });
    }

    // 2. Font Size Selector Row
    let btn_row_font = gtk4::Button::builder()
        .css_classes(["clean-settings-row"])
        .halign(gtk4::Align::Fill)
        .build();
    let box_rf = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let lbl_rf_title = gtk4::Label::builder()
        .label("Font Size")
        .css_classes(["clean-popover-item-desc"])
        .xalign(0.0)
        .hexpand(true)
        .build();
    let initial_font_name = settings::FONT_SIZE_PRESETS
        .iter()
        .find(|p| p.size == initial_settings.font_size)
        .map(|p| format!("{} ›", p.label))
        .unwrap_or_else(|| "13px ›".to_string());
    let lbl_rf_val = gtk4::Label::builder()
        .label(&initial_font_name)
        .css_classes(["clean-settings-val"])
        .xalign(1.0)
        .build();
    box_rf.append(&lbl_rf_title);
    box_rf.append(&lbl_rf_val);
    btn_row_font.set_child(Some(&box_rf));
    more_box.append(&btn_row_font);

    // Inline Font Size subpage
    let prf_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    prf_box.set_hexpand(true);
    {
        let back_btn = gtk4::Button::builder()
            .label("← Session Settings")
            .css_classes(["floating-menu-back"])
            .halign(gtk4::Align::Start)
            .build();
        let ms_back = more_stack.clone();
        back_btn.connect_clicked(move |_| ms_back.set_visible_child_name("main"));
        prf_box.append(&back_btn);
    }
    prf_box.append(
        &gtk4::Label::builder()
            .label("BASE FONT SIZE")
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build(),
    );

    for preset in settings::FONT_SIZE_PRESETS {
        let f_btn = gtk4::Button::builder()
            .css_classes(["clean-popover-item"])
            .halign(gtk4::Align::Fill)
            .build();
        f_btn.set_child(Some(
            &gtk4::Label::builder()
                .label(format!("{}  •  {}", preset.label, preset.desc))
                .xalign(0.0)
                .build(),
        ));

        let ms_done = more_stack.clone();
        let state_fs = state.clone();
        let scale_provider_ref = scale_css_provider.clone();
        let fs_val = preset.size;
        let fs_lbl = preset.label;
        let lbl_val = lbl_rf_val.clone();

        f_btn.connect_clicked(move |_| {
            let mut st = state_fs.borrow_mut();
            st.settings.font_size = fs_val;
            settings::apply_text_scale(&scale_provider_ref, st.settings.ui_scale, fs_val);
            let _ = st.settings.save();
            lbl_val.set_label(&format!("{} ›", fs_lbl));
            ms_done.set_visible_child_name("main");
        });
        prf_box.append(&f_btn);
    }

    {
        let ms_go = more_stack.clone();
        btn_row_font.connect_clicked(move |_| {
            ms_go.set_visible_child_name("font");
        });
    }

    // Window Toggles
    let pin_check = gtk4::CheckButton::builder()
        .label("Pin sidebar on blur")
        .active(!initial_settings.auto_hide_on_blur)
        .css_classes(["clean-popover-check"])
        .build();
    more_box.append(&pin_check);

    let ephemeral_check = gtk4::CheckButton::builder()
        .label("Ephemeral (Clear on hide)")
        .active(initial_settings.ephemeral)
        .css_classes(["clean-popover-check"])
        .build();
    more_box.append(&ephemeral_check);

    more_box.append(&gtk4::Separator::builder().css_classes(["menu-sep"]).build());

    // Clear Chat Option
    let btn_clear_chat = gtk4::Button::builder()
        .css_classes(["clean-popover-item", "destructive"])
        .halign(gtk4::Align::Fill)
        .build();
    let clear_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    clear_row.append(&icons::image("codicon-trash-symbolic", 13));
    clear_row.append(&gtk4::Label::builder().label("Clear Chat History").xalign(0.0).build());
    btn_clear_chat.set_child(Some(&clear_row));
    more_box.append(&btn_clear_chat);

    more_stack.add_named(&more_box, Some("main"));
    more_stack.add_named(&prs_box, Some("scale"));
    more_stack.add_named(&prf_box, Some("font"));

    {
        let mh_ref = menu_host.clone();
        let ms_ref = more_stack.clone();
        btn_more.connect_clicked(move |btn| {
            ms_ref.set_visible_child_name("main");
            mh_ref.toggle_menu("more", btn, &ms_ref, 260, menu::MenuPlacement::BelowRight);
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
        let mh = menu_host.clone();
        let mp = mention_popup.clone();
        Rc::new(move || {
            mh.close_menu();
            mp.borrow_mut().close();
            let is_ephemeral = state.borrow().settings.ephemeral;
            let is_win = state.borrow().is_window_mode;
            if !is_win {
                overlay::hide_overlay(&win);
            } else {
                win.set_visible(false);
            }
            state.borrow_mut().mode = OverlayMode::Hidden;
            if is_ephemeral {
                state.borrow_mut().harness.abort();
            }
        })
    };

    let expand_overlay_fn = {
        let win = window.clone();
        let state = state.clone();
        let tv = text_view.clone();
        Rc::new(move || {
            let (top, bot, right, is_win, edge) = {
                let st = state.borrow();
                (
                    st.top_margin,
                    st.bottom_margin,
                    st.settings.margin_right,
                    st.is_window_mode,
                    st.settings.edge.clone(),
                )
            };
            if !is_win {
                overlay::expand_overlay(&win, top, bot, right, &edge);
            } else {
                win.set_visible(true);
                win.present();
            }
            state.borrow_mut().mode = OverlayMode::Visible;
            tv.grab_focus();
            let tv_idle = tv.clone();
            glib::idle_add_local_once(move || {
                tv_idle.grab_focus();
            });
        })
    };

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
    let active_markup_overlay: Rc<RefCell<Option<markup::AgentMarkupOverlay>>> =
        Rc::new(RefCell::new(None));

    // Clear transcript helper
    let execute_clear = {
        let messages_box = messages_box.clone();
        let chat_stack = chat_stack.clone();
        let prompts_list_box = prompts_list_box.clone();
        let prompt_empty_label = prompt_empty_label.clone();
        let state_clear = state.clone();
        let active_stream_clear = active_stream.clone();
        let active_markup_clear = active_markup_overlay.clone();
        let is_streaming_clear = is_streaming.clone();
        let update_send_btn_clear = update_send_btn_state.clone();
        let session_title_clear = session_title_label.clone();

        Rc::new(move || {
            state_clear.borrow_mut().harness.new_session();
            *active_stream_clear.borrow_mut() = None;
            if let Some(prev) = active_markup_clear.borrow_mut().take() {
                prev.close();
            }
            is_streaming_clear.set(false);
            update_send_btn_clear();

            while let Some(child) = messages_box.first_child() {
                messages_box.remove(&child);
            }
            chat_stack.set_visible_child_name("welcome");

            while let Some(child) = prompts_list_box.first_child() {
                prompts_list_box.remove(&child);
            }
            prompt_empty_label.set_visible(true);
            prompts_list_box.append(&prompt_empty_label);
            session_title_clear.set_label("");
            state_clear.borrow_mut().prompts.clear();
        })
    };

    // Inline confirmation page inside Settings menu for Clear action
    {
        let confirm_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        confirm_box.set_hexpand(true);
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
        more_stack.add_named(&confirm_box, Some("confirm_clear"));

        let ms_cancel = more_stack.clone();
        btn_cancel.connect_clicked(move |_| {
            ms_cancel.set_visible_child_name("main");
        });

        let do_clear = execute_clear.clone();
        let mh_close = menu_host.clone();
        let ms_reset = more_stack.clone();
        btn_confirm.connect_clicked(move |_| {
            mh_close.close_menu();
            ms_reset.set_visible_child_name("main");
            do_clear();
        });

        let ms_go = more_stack.clone();
        let state_check = state.clone();
        btn_clear_chat.connect_clicked(move |_| {
            if !state_check.borrow().prompts.is_empty() {
                ms_go.set_visible_child_name("confirm_clear");
            }
        });
    }

    // New Chat Confirmation Menu (prompts if unsent draft text exists)
    let new_chat_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    new_chat_box.set_hexpand(true);
    let nc_title = gtk4::Label::builder()
        .label("Discard Unsent Draft?")
        .css_classes(["confirm-dialog-title"])
        .xalign(0.0)
        .build();
    new_chat_box.append(&nc_title);

    let nc_desc = gtk4::Label::builder()
        .label("You have unsent text in the composer. Start a new session and discard it?")
        .css_classes(["confirm-dialog-desc"])
        .wrap(true)
        .wrap_mode(gtk4::pango::WrapMode::WordChar)
        .xalign(0.0)
        .build();
    new_chat_box.append(&nc_desc);

    let nc_btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    nc_btn_row.set_halign(gtk4::Align::End);

    let btn_nc_cancel = gtk4::Button::builder()
        .label("Keep Draft")
        .css_classes(["btn-dialog-cancel"])
        .build();
    let btn_nc_discard = gtk4::Button::builder()
        .label("Discard & New")
        .css_classes(["btn-dialog-destructive"])
        .build();
    nc_btn_row.append(&btn_nc_cancel);
    nc_btn_row.append(&btn_nc_discard);
    new_chat_box.append(&nc_btn_row);

    {
        let mh_close = menu_host.clone();
        btn_nc_cancel.connect_clicked(move |_| {
            mh_close.close_menu();
        });
    }

    {
        let mh_close = menu_host.clone();
        let do_clear = execute_clear.clone();
        let tv_ref = text_view.clone();
        let update_btn = update_send_btn_state.clone();
        btn_nc_discard.connect_clicked(move |_| {
            mh_close.close_menu();
            set_text_view_content(&tv_ref, "");
            update_btn();
            do_clear();
            tv_ref.grab_focus();
        });
    }

    let request_new_chat = {
        let mh_ref = menu_host.clone();
        let nc_anchor = btn_new_chat.clone();
        let nc_box_ref = new_chat_box.clone();
        let do_clear = execute_clear.clone();
        let tv_ref = text_view.clone();
        Rc::new(move || {
            let draft = get_text_view_content(&tv_ref);
            if !draft.is_empty() {
                mh_ref.open_menu(
                    "new_chat",
                    &nc_anchor,
                    &nc_box_ref,
                    260,
                    menu::MenuPlacement::BelowRight,
                );
            } else {
                do_clear();
                tv_ref.grab_focus();
            }
        })
    };

    // New Chat Button (+) in panel header
    {
        let req_nc = request_new_chat.clone();
        btn_new_chat.connect_clicked(move |_| {
            req_nc();
        });
    }

    // Stream consumer loop
    {
        let active_stream_clone = active_stream.clone();
        let active_markup_clone = active_markup_overlay.clone();
        let state_clone = state.clone();
        let scroll_fn = scroll_to_bottom.clone();
        let update_send_btn_stream = update_send_btn_state.clone();
        let is_streaming_ref = is_streaming.clone();
        let app_clone = app.clone();

        glib::spawn_future_local(async move {
            while let Some((gen, event)) = stream_rx.recv().await {
                let st = state_clone.borrow();
                if gen != st.harness.generation() {
                    continue;
                }

                match event {
                    StreamEvent::SessionConfig(cfg) => {
                        // Catalogs are owned by the composer; apply them so the
                        // model / mode / thinking pills match this harness.
                        apply_session_config(&state_clone, &cfg);
                    }
                    StreamEvent::PermissionRequest(req) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            let card = build_permission_card(&req, &state_clone);
                            target.turn_body.append(&card);
                        }
                        scroll_fn();
                    }
                    StreamEvent::ThinkingDelta(delta) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            collapse_and_finalize_progress(target);
                            *target.active_markdown.borrow_mut() = None;

                            let mut th_opt = target.thinking.borrow_mut();
                            if th_opt.is_none() {
                                let disclosure = Disclosure::new("Thinking…", true);

                                let th_scroll = gtk4::ScrolledWindow::builder()
                                    .hscrollbar_policy(gtk4::PolicyType::Never)
                                    .vscrollbar_policy(gtk4::PolicyType::Automatic)
                                    .max_content_height(140)
                                    .propagate_natural_height(true)
                                    .build();

                                let buffer = gtk4::TextBuffer::new(None);
                                let tv = gtk4::TextView::builder()
                                    .buffer(&buffer)
                                    .editable(false)
                                    .cursor_visible(false)
                                    .wrap_mode(gtk4::WrapMode::WordChar)
                                    .css_classes(["chat-thinking-text"])
                                    .build();

                                th_scroll.set_child(Some(&tv));
                                disclosure.rail.append(&th_scroll);
                                target.turn_body.append(&disclosure.root);

                                *th_opt = Some(ThinkingDisclosureState {
                                    disclosure,
                                    text_view: tv,
                                    buffer,
                                    content: String::new(),
                                    start_time: std::time::Instant::now(),
                                    last_elapsed_sec: 0,
                                });
                            }

                            if let Some(ref mut th) = *th_opt {
                                th.content.push_str(&delta);
                                let mut end = th.buffer.end_iter();
                                th.buffer.insert(&mut end, &delta);
                                let mut end_scroll = th.buffer.end_iter();
                                th.text_view
                                    .scroll_to_iter(&mut end_scroll, 0.0, false, 0.0, 1.0);

                                let elapsed = th.start_time.elapsed().as_secs();
                                if elapsed != th.last_elapsed_sec {
                                    th.last_elapsed_sec = elapsed;
                                    th.disclosure
                                        .set_title(&format!("Thinking… ({}s)", elapsed.max(1)));
                                }
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolStart { id, name, args } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            collapse_and_finalize_thinking(target);
                            *target.active_markdown.borrow_mut() = None;

                            let mut p_opt = target.progress.borrow_mut();
                            if p_opt.is_none() {
                                let disclosure = Disclosure::new("Running steps…", true);
                                let steps_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                                steps_box.add_css_class("steps-list-box");
                                disclosure.rail.append(&steps_box);
                                target.turn_body.append(&disclosure.root);

                                *p_opt = Some(ProgressDisclosureState {
                                    disclosure,
                                    steps_box,
                                    steps_count: 0,
                                    has_error: false,
                                    start_time: std::time::Instant::now(),
                                    items: HashMap::new(),
                                });
                            }

                            if let Some(ref mut p) = *p_opt {
                                p.steps_count += 1;
                                let elapsed = p.start_time.elapsed().as_secs();
                                p.disclosure.set_title(&format!(
                                    "Running step {} ({})",
                                    p.steps_count,
                                    format::format_duration_sec(elapsed)
                                ));

                                let item_card = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                                item_card.add_css_class("tool-step-row");

                                let card_hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                                let icon_img = icons::image(format::tool_icon_name(&name), 13);
                                icon_img.add_css_class("tool-step-icon");
                                icon_img.add_css_class("running");
                                card_hdr.append(&icon_img);

                                let action_lbl = format::format_tool_call_label(&name, &args);
                                let name_lbl = gtk4::Label::builder()
                                    .label(&action_lbl)
                                    .css_classes(["tool-step-label"])
                                    .ellipsize(gtk4::pango::EllipsizeMode::End)
                                    .max_width_chars(38)
                                    .hexpand(true)
                                    .halign(gtk4::Align::Start)
                                    .build();
                                card_hdr.append(&name_lbl);

                                let badge = gtk4::Label::builder()
                                    .label("running")
                                    .css_classes(["tool-step-meta"])
                                    .build();
                                card_hdr.append(&badge);
                                item_card.append(&card_hdr);

                                p.steps_box.append(&item_card);
                                p.items.insert(
                                    id,
                                    StepItemEntry {
                                        card: item_card,
                                        icon: icon_img,
                                        badge,
                                        start_time: std::time::Instant::now(),
                                    },
                                );

                                *target.active_markdown.borrow_mut() = None;
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolEnd {
                        id,
                        name,
                        result,
                        is_error,
                        duration_ms,
                    } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            collapse_and_finalize_thinking(target);

                            let mut p_opt = target.progress.borrow_mut();
                            if p_opt.is_none() {
                                let disclosure = Disclosure::new("Running steps…", true);
                                let steps_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                                steps_box.add_css_class("steps-list-box");
                                disclosure.rail.append(&steps_box);
                                target.turn_body.append(&disclosure.root);

                                *p_opt = Some(ProgressDisclosureState {
                                    disclosure,
                                    steps_box,
                                    steps_count: 0,
                                    has_error: false,
                                    start_time: std::time::Instant::now(),
                                    items: HashMap::new(),
                                });
                            }

                            if let Some(ref mut p) = *p_opt {
                                if is_error {
                                    p.has_error = true;
                                }
                                if !p.items.contains_key(&id) {
                                    p.steps_count += 1;
                                    let item_card = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                                    item_card.add_css_class("tool-step-row");

                                    let card_hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                                    let icon_img = icons::image(format::tool_icon_name(&name), 13);
                                    icon_img.add_css_class("tool-step-icon");
                                    card_hdr.append(&icon_img);

                                    let action_lbl = format::format_tool_call_label(&name, "{}");
                                    let name_lbl = gtk4::Label::builder()
                                        .label(&action_lbl)
                                        .css_classes(["tool-step-label"])
                                        .ellipsize(gtk4::pango::EllipsizeMode::End)
                                        .max_width_chars(38)
                                        .hexpand(true)
                                        .halign(gtk4::Align::Start)
                                        .build();
                                    card_hdr.append(&name_lbl);

                                    let badge = gtk4::Label::builder()
                                        .label("")
                                        .css_classes(["tool-step-meta"])
                                        .build();
                                    card_hdr.append(&badge);
                                    item_card.append(&card_hdr);

                                    p.steps_box.append(&item_card);
                                    p.items.insert(
                                        id.clone(),
                                        StepItemEntry {
                                            card: item_card,
                                            icon: icon_img,
                                            badge,
                                            start_time: std::time::Instant::now(),
                                        },
                                    );
                                }

                                if let Some(item) = p.items.get_mut(&id) {
                                    let dur_s = if duration_ms > 0 {
                                        duration_ms / 1000
                                    } else {
                                        item.start_time.elapsed().as_secs()
                                    };
                                    item.icon.remove_css_class("running");
                                    if is_error {
                                        item.icon.set_icon_name(Some("codicon-error-symbolic"));
                                        item.icon.add_css_class("error");
                                        item.badge.set_text(&format!(
                                            "err · {}",
                                            format::format_duration_sec(dur_s)
                                        ));
                                    } else {
                                        item.icon.set_icon_name(Some("codicon-check-symbolic"));
                                        item.icon.add_css_class("done");
                                        item.badge.set_text(&format::format_duration_sec(dur_s));
                                    }

                                    let clean_res = format::strip_ansi_escapes(&result);
                                    let trimmed_res = clean_res.trim();
                                    if !trimmed_res.is_empty() {
                                        let out_disc = Disclosure::new("Output", false);
                                        let out_scroll = gtk4::ScrolledWindow::builder()
                                            .hscrollbar_policy(gtk4::PolicyType::Automatic)
                                            .vscrollbar_policy(gtk4::PolicyType::Automatic)
                                            .max_content_height(150)
                                            .propagate_natural_height(true)
                                            .build();

                                        let out_lbl = gtk4::Label::builder()
                                            .label(trimmed_res)
                                            .selectable(true)
                                            .wrap(true)
                                            .wrap_mode(gtk4::pango::WrapMode::WordChar)
                                            .xalign(0.0)
                                            .css_classes(["tool-output-label"])
                                            .build();
                                        out_scroll.set_child(Some(&out_lbl));
                                        out_disc.rail.append(&out_scroll);
                                        item.card.append(&out_disc.root);
                                    }
                                }
                                let total_elapsed = p.start_time.elapsed().as_secs();
                                let dur_str = format::format_duration_sec(total_elapsed);
                                let lbl = if p.has_error {
                                    format!("Completed {} steps with errors in {}", p.steps_count, dur_str)
                                } else {
                                    format!("Completed {} steps in {}", p.steps_count, dur_str)
                                };
                                p.disclosure.set_title(&lbl);
                            }
                            *target.active_markdown.borrow_mut() = None;
                        }
                        scroll_fn();
                    }
                    StreamEvent::ToolCall { name, args } => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            collapse_and_finalize_thinking(target);

                            let mut p_opt = target.progress.borrow_mut();
                            if p_opt.is_none() {
                                let disclosure = Disclosure::new("Steps", true);
                                let steps_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                                steps_box.add_css_class("steps-list-box");
                                disclosure.rail.append(&steps_box);
                                target.turn_body.append(&disclosure.root);

                                *p_opt = Some(ProgressDisclosureState {
                                    disclosure,
                                    steps_box,
                                    steps_count: 0,
                                    has_error: false,
                                    start_time: std::time::Instant::now(),
                                    items: HashMap::new(),
                                });
                            }
                            if let Some(ref mut p) = *p_opt {
                                p.steps_count += 1;
                                let item_card = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                                item_card.add_css_class("tool-step-row");

                                let icon_img = icons::image(format::tool_icon_name(&name), 13);
                                icon_img.add_css_class("tool-step-icon");
                                icon_img.add_css_class("done");
                                item_card.append(&icon_img);

                                let action_lbl = format::format_tool_call_label(&name, &args);
                                let name_lbl = gtk4::Label::builder()
                                    .label(&action_lbl)
                                    .css_classes(["tool-step-label"])
                                    .ellipsize(gtk4::pango::EllipsizeMode::End)
                                    .max_width_chars(38)
                                    .hexpand(true)
                                    .halign(gtk4::Align::Start)
                                    .build();
                                item_card.append(&name_lbl);
                                p.steps_box.append(&item_card);
                            }
                            *target.active_markdown.borrow_mut() = None;
                        }
                        scroll_fn();
                    }
                    StreamEvent::TextDelta(delta) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            collapse_and_finalize_thinking(target);
                            collapse_and_finalize_progress(target);
                            let full_snapshot = {
                                let mut full = target.full_text.borrow_mut();
                                full.push_str(&delta);
                                full.clone()
                            };

                            let mut md_opt = target.active_markdown.borrow_mut();
                            if md_opt.is_none() {
                                let md_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                                md_box.add_css_class("markdown-stream");
                                target.turn_body.append(&md_box);
                                *md_opt = Some(MarkdownStream::new(md_box));
                            }
                            if let Some(stream) = md_opt.as_mut() {
                                stream.push(&delta);
                            }
                            if delta.contains("]]") && full_snapshot.contains("[[markup:") {
                                sync_live_markup_overlay(
                                    &app_clone,
                                    &active_markup_clone,
                                    &full_snapshot,
                                );
                            }
                        }
                        scroll_fn();
                    }
                    StreamEvent::Content(line) => {
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            collapse_and_finalize_thinking(target);
                            collapse_and_finalize_progress(target);
                            let full_snapshot = {
                                let mut full = target.full_text.borrow_mut();
                                if !full.is_empty() {
                                    full.push('\n');
                                }
                                full.push_str(&line);
                                full.clone()
                            };

                            let mut md_opt = target.active_markdown.borrow_mut();
                            if md_opt.is_none() {
                                let md_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                                md_box.add_css_class("markdown-stream");
                                target.turn_body.append(&md_box);
                                *md_opt = Some(MarkdownStream::new(md_box));
                            }
                            if let Some(stream) = md_opt.as_mut() {
                                let mut delta = String::new();
                                if !stream.text.is_empty() {
                                    delta.push('\n');
                                }
                                delta.push_str(&line);
                                stream.push(&delta);
                            }
                            if line.contains("]]") && full_snapshot.contains("[[markup:") {
                                sync_live_markup_overlay(
                                    &app_clone,
                                    &active_markup_clone,
                                    &full_snapshot,
                                );
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
                            collapse_and_finalize_thinking(target);
                            collapse_and_finalize_progress(target);
                            let clean_err = format::strip_ansi_escapes(&err_line);
                            let err_msg = format!("[Error] {}", clean_err);

                            let mut md_opt = target.active_markdown.borrow_mut();
                            if md_opt.is_none() {
                                let md_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                                md_box.add_css_class("markdown-stream");
                                target.turn_body.append(&md_box);
                                *md_opt = Some(MarkdownStream::new(md_box));
                            }
                            if let Some(stream) = md_opt.as_mut() {
                                stream.push(&format!("\n\n{}\n", err_msg));
                            }
                        }
                        is_streaming_ref.set(false);
                        update_send_btn_stream();
                        scroll_fn();
                    }
                    StreamEvent::Done => {
                        let mut agent_shapes = Vec::new();
                        if let Some(ref target) = *active_stream_clone.borrow() {
                            collapse_and_finalize_thinking(target);
                            collapse_and_finalize_progress(target);
                            // Final full render: normalises any trailing partial
                            // block (e.g. a fence that never closed) now the
                            // stream is complete.
                            let full = target.full_text.borrow().clone();
                            if let Some(stream) = target.active_markdown.borrow().as_ref() {
                                render::render_markdown_streaming_into(
                                    &stream.container,
                                    &mut stream.blocks.clone(),
                                    &stream.text,
                                );
                            }
                            // Any [[markup: ...]] tags become a click-through screen overlay.
                            agent_shapes = sync_live_markup_overlay(
                                &app_clone,
                                &active_markup_clone,
                                &full,
                            );
                        }
                        if !agent_shapes.is_empty() {
                            let count = agent_shapes.len();
                            let app_overlay = app_clone.clone();
                            let slot_overlay = active_markup_clone.clone();
                            let shapes_for_btn = agent_shapes.clone();
                            let pill = gtk4::Button::builder()
                                .css_classes(["composer-pill-btn", "markup-pill"])
                                .tooltip_text("Show the screen annotations again")
                                .build();
                            let pill_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
                            pill_row.append(&icons::image("codicon-screen-full-symbolic", 12));
                            pill_row.append(&gtk4::Label::new(Some(&format!(
                                "{count} highlighted on screen"
                            ))));
                            pill.set_child(Some(&pill_row));
                            pill.connect_clicked(move |_| {
                                if let Some(prev) = slot_overlay.borrow_mut().take() {
                                    prev.close();
                                }
                                let overlay = markup::AgentMarkupOverlay::new(
                                    &app_overlay,
                                    shapes_for_btn.clone(),
                                );
                                overlay.present();
                                *slot_overlay.borrow_mut() = Some(overlay);
                            });
                            if let Some(ref target) = *active_stream_clone.borrow() {
                                target.turn_body.append(&pill);
                            }
                        }
                        if state_clone.borrow().settings.sound_bell_on_done {
                            overlay::ring_completion_bell();
                        }
                        is_streaming_ref.set(false);
                        update_send_btn_stream();
                        scroll_fn();
                    }
                }
            }
        });
    }

    // Submit Prompt handler
    let submit_prompt = {
        let tv = text_view.clone();
        let chips_row = chips_row.clone();
        let messages_box = messages_box.clone();
        let chat_stack = chat_stack.clone();
        let prompts_list_box = prompts_list_box.clone();
        let prompt_empty_label = prompt_empty_label.clone();
        let menu_host_submit = menu_host.clone();
        let session_stack_submit = session_stack.clone();
        let pending_delete_submit = pending_prompt_delete.clone();
        let state = state.clone();
        let active_stream = active_stream.clone();
        let stream_tx = stream_tx.clone();
        let scroll_fn = scroll_to_bottom.clone();
        let scroll_to_widget = scroll_to_widget.clone();
        let is_streaming_submit = is_streaming.clone();
        let update_btn_submit = update_send_btn_state.clone();
        let session_title_submit = session_title_label.clone();
        let window_submit = window.clone();
        // The outer entry closure needs its own handles: the ones above are
        // moved into `deliver` below.
        let tv_entry = text_view.clone();
        let state_entry = state.clone();
        let active_markup_submit = active_markup_overlay.clone();
        let update_btn_entry = update_send_btn_state.clone();
        let is_streaming_entry = is_streaming.clone();
        let clear_fn_entry = execute_clear.clone();
        let capture_fn_entry = capture_screen_fn.clone();
        let markup_fn_entry = capture_markup_fn.clone();

        // The prompt being delivered. `deliver` reads this so the live-screen
        // capture below can complete before the turn is built.
        let pending: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

        // Builds and dispatches one turn from `pending`.
        let deliver: Rc<dyn Fn()> = {
            let pending = pending.clone();
            Rc::new(move || {
                let raw_prompt = pending.borrow().clone();
                if raw_prompt.is_empty() {
                    return;
                }
                let prompt = commands::expand_slash_command(&raw_prompt).unwrap_or(raw_prompt.clone());

            // Clear draft from buffer on send
            update_btn_submit();
            chat_stack.set_visible_child_name("messages");

            let prompt_idx = state.borrow().prompts.len() + 1;

            // Update session title in subheader
            let snippet = if prompt.chars().count() > 24 {
                format!("{}…", prompt.chars().take(24).collect::<String>())
            } else {
                prompt.clone()
            };
            session_title_submit.set_label(&snippet);

            // 1. User Request Card (VS Code full-width left-aligned)
            let user_card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            user_card.add_css_class("card-user");
            user_card.set_halign(gtk4::Align::Fill);

            let attached_for_display = state.borrow().attached_files.clone();
            if !attached_for_display.is_empty() {
                let chips_row_user = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
                chips_row_user.add_css_class("user-attached-chips");
                for (p, _) in &attached_for_display {
                    let fname = std::path::Path::new(p)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(p.as_str());
                    let chip = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
                    chip.add_css_class("user-attached-chip");
                    let icon = icons::image(
                        if capture::is_image_path(p) {
                            "codicon-device-camera-symbolic"
                        } else {
                            "codicon-file-code-symbolic"
                        },
                        11,
                    );
                    icon.add_css_class("user-attached-chip-icon");
                    chip.append(&icon);
                    chip.append(&gtk4::Label::new(Some(fname)));
                    chips_row_user.append(&chip);
                }
                user_card.append(&chips_row_user);
            }

            let user_label = gtk4::Label::builder()
                .label(&prompt)
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .width_chars(1)
                .max_width_chars(64)
                .selectable(true)
                .xalign(0.0)
                .css_classes(["card-user-text"])
                .build();
            user_card.append(&user_label);
            messages_box.append(&user_card);

            // 2. Assistant Turn (VS Code Chat full-width)
            let asst_card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            asst_card.add_css_class("card-assistant");
            asst_card.set_halign(gtk4::Align::Fill);

            let asst_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            asst_header.add_css_class("card-assistant-header");

            let avatar_wrap = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            avatar_wrap.add_css_class("card-assistant-avatar");
            avatar_wrap.set_size_request(24, 24);
            avatar_wrap.set_hexpand(false);
            avatar_wrap.set_vexpand(false);
            avatar_wrap.set_halign(gtk4::Align::Start);
            avatar_wrap.set_valign(gtk4::Align::Center);
            let avatar_icon = icons::image("codicon-sparkle-symbolic", 13);
            avatar_icon.set_hexpand(true);
            avatar_icon.set_halign(gtk4::Align::Center);
            avatar_icon.set_valign(gtk4::Align::Center);
            avatar_wrap.append(&avatar_icon);

            let asst_author = gtk4::Label::builder()
                .label("Assistant")
                .css_classes(["card-assistant-author"])
                .build();
            let cur_model_label = {
                let st = state.borrow();
                model_display_name_with_list(&st.settings.model, &st.available_models)
            };
            let asst_model = gtk4::Label::builder()
                .label(&cur_model_label)
                .css_classes(["card-assistant-model"])
                .build();

            asst_header.append(&avatar_wrap);
            asst_header.append(&asst_author);
            asst_header.append(&asst_model);
            asst_card.append(&asst_header);

            let turn_body = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            asst_card.append(&turn_body);

            // Response Footer Toolbar (Copy + Retry + Timing)
            let feedback_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
            feedback_bar.add_css_class("card-feedback-bar");

            let btn_copy = icons::icon_button(
                "codicon-copy-symbolic",
                13,
                "Copy response",
                &["feedback-btn"],
            );
            let btn_retry = icons::icon_button(
                "codicon-refresh-symbolic",
                13,
                "Retry prompt",
                &["feedback-btn"],
            );

            let metrics_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            metrics_spacer.set_hexpand(true);

            let metrics_badge = gtk4::Label::new(None);
            metrics_badge.add_css_class("card-metrics-pill");
            metrics_badge.set_visible(false);

            feedback_bar.append(&btn_copy);
            feedback_bar.append(&btn_retry);
            feedback_bar.append(&metrics_spacer);
            feedback_bar.append(&metrics_badge);
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
                    icons::set_button_icon(&copy_btn_clone, "codicon-check-symbolic", 13);
                    copy_btn_clone.add_css_class("copied");
                    let reset_btn = copy_btn_clone.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                        icons::set_button_icon(&reset_btn, "codicon-copy-symbolic", 13);
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
            let mh_nav = menu_host_submit.clone();
            let scroll_fn_widget = scroll_to_widget.clone();

            nav_btn.connect_clicked(move |_| {
                mh_nav.close_menu();
                scroll_fn_widget(&target_widget);
            });
            row_box.append(&nav_btn);

            let del_btn = icons::icon_button(
                "codicon-trash-symbolic",
                12,
                "Delete prompt from session",
                &["prompt-delete-btn"],
            );
            row_box.append(&del_btn);

            prompts_list_box.append(&row_box);

            // Per-item prompt deletion using inline confirmation page in History menu.
            {
                let row_ref = row_box.clone();
                let u_card = user_card.clone();
                let a_card = asst_card.clone();
                let msg_box_ref = messages_box.clone();
                let p_list_box_ref = prompts_list_box.clone();
                let p_empty_lbl = prompt_empty_label.clone();
                let chat_stack_ref = chat_stack.clone();
                let st_ref = state.clone();
                let active_stream_ref = active_stream.clone();
                let is_str_ref = is_streaming_submit.clone();
                let update_btn_ref = update_btn_submit.clone();
                let this_idx = prompt_idx;
                let ss_del = session_stack_submit.clone();
                let pending_del = pending_delete_submit.clone();

                del_btn.connect_clicked(move |_| {
                    let row_ref = row_ref.clone();
                    let u_card = u_card.clone();
                    let a_card = a_card.clone();
                    let msg_box_ref = msg_box_ref.clone();
                    let p_list_box_ref = p_list_box_ref.clone();
                    let p_empty_lbl = p_empty_lbl.clone();
                    let chat_stack_ref = chat_stack_ref.clone();
                    let st_ref = st_ref.clone();
                    let active_stream_ref = active_stream_ref.clone();
                    let is_str_ref = is_str_ref.clone();
                    let update_btn_ref = update_btn_ref.clone();

                    *pending_del.borrow_mut() = Some(Box::new(move || {
                        msg_box_ref.remove(&u_card);
                        msg_box_ref.remove(&a_card);
                        p_list_box_ref.remove(&row_ref);

                        let mut st = st_ref.borrow_mut();
                        if st.prompts.last().map(|p| p.index == this_idx).unwrap_or(false) {
                            st.harness.abort();
                            *active_stream_ref.borrow_mut() = None;
                            is_str_ref.set(false);
                            update_btn_ref();
                        }
                        st.prompts.retain(|p| p.index != this_idx);
                        let remaining = st.prompts.len();

                        if remaining == 0 {
                            p_empty_lbl.set_visible(true);
                            chat_stack_ref.set_visible_child_name("welcome");
                        }
                    }));
                    ss_del.set_visible_child_name("confirm_delete");
                });
            }

            state.borrow_mut().prompts.push(TrackedPrompt {
                index: prompt_idx,
                user_card: user_card.clone(),
            });

            *active_stream.borrow_mut() = Some(ActiveStreamTarget {
                turn_body: turn_body.clone(),
                full_text: full_text.clone(),
                thinking: Rc::new(RefCell::new(None)),
                progress: Rc::new(RefCell::new(None)),
                active_markdown: Rc::new(RefCell::new(None)),
                metrics_badge: metrics_badge.clone(),
            });

            scroll_fn();

            // Switch Send button to Stop button during active stream
            is_streaming_submit.set(true);
            update_btn_submit();

            let mut st = state.borrow_mut();
            let model_opt = Some(st.settings.model.clone());
            let thinking_opt = Some(st.settings.thinking_level.clone());
            let mode_opt = Some(st.current_mode.clone());
            let attached = st.attached_files.clone();
            if let Err(e) =
                st.harness
                    .spawn_prompt(prompt.clone(), attached, model_opt, thinking_opt, mode_opt, stream_tx.clone())
            {
                // On failure, restore prompt to buffer so user doesn't lose draft
                set_text_view_content(&tv, &prompt);
                let err_str = format!("[Error] Failed to spawn agent: {}", e);
                full_text.borrow_mut().push_str(&err_str);
                let err_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                turn_body.append(&err_box);
                crate::render::render_markdown_blocks_into(&err_box, &err_str);
                is_streaming_submit.set(false);
                update_btn_submit();
                scroll_fn();
            } else {
                // Clear attached chips after submitting
                st.attached_files.clear();
                while let Some(child) = chips_row.first_child() {
                    chips_row.remove(&child);
                }
            }
            })
        };

        // Entry point: capture the live screen first when the prompt asks for
        // it, then hand off to `deliver` so the turn includes the screenshot.
        Rc::new(move || {
            if is_streaming_entry.get() {
                return;
            }
            let raw_prompt = get_text_view_content(&tv_entry);
            if raw_prompt.is_empty() {
                return;
            }

            // Client-handled commands never reach a harness.
            if let Some(cmd) = commands::parse_local_command(&raw_prompt) {
                use commands::LocalCommand;
                set_text_view_content(&tv_entry, "");
                match cmd {
                    LocalCommand::NewSession | LocalCommand::ClearConversation => {
                        clear_fn_entry();
                    }
                    LocalCommand::CaptureScreen | LocalCommand::CaptureRegion
                    | LocalCommand::CaptureWindow => {
                        capture_fn_entry();
                    }
                    LocalCommand::OpenMarkup => {
                        markup_fn_entry();
                    }
                    LocalCommand::ShowHelp => {
                        set_text_view_content(
                            &tv_entry,
                            "Commands: /new /clear /screen /region /window /markup /help",
                        );
                    }
                }
                return;
            }

            if let Some(prev) = active_markup_submit.borrow_mut().take() {
                prev.close();
            }

            let live_capture_mode = {
                let st = state_entry.borrow();
                if raw_prompt.contains("@region") {
                    Some(capture::CaptureMode::Region)
                } else if raw_prompt.contains("@window") {
                    Some(capture::CaptureMode::ActiveWindow)
                } else if st.settings.pin_live_screen || raw_prompt.contains("@screen") {
                    Some(capture::CaptureMode::FullScreen)
                } else {
                    None
                }
            };
            let has_screen = state_entry
                .borrow()
                .attached_files
                .iter()
                .any(|(p, _)| capture::is_image_path(p));

            *pending.borrow_mut() = raw_prompt;

            // Set up the composer for the turn now, so the UI reacts to Send
            // immediately even while the capture is in flight.
            set_text_view_content(&tv_entry, "");
            update_btn_entry();

            if let (Some(cap_mode), false) = (live_capture_mode, has_screen) {
                let state_cap = state_entry.clone();
                let deliver_after = deliver.clone();
                capture::capture_mode_hiding_window(&window_submit, cap_mode, move |res| {
                    match res {
                        Ok(path) => {
                            let path_str = path.to_string_lossy().to_string();
                            let content = std::fs::read(&path)
                                .map(|b| capture::base64_encode(&b))
                                .unwrap_or_default();
                            state_cap
                                .borrow_mut()
                                .attached_files
                                .push((path_str, content));
                        }
                        Err(err) => eprintln!("Live screen capture failed: {err}"),
                    }
                    deliver_after();
                });
            } else {
                deliver();
            }
        })
    };

    // Keyboard Controller on Multi-line TextView: Enter sends, Shift+Enter new line
    {
        let key_controller = gtk4::EventControllerKey::new();
        let submit_on_enter = submit_prompt.clone();
        let popup_state = mention_popup.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
            // While the autocomplete is open it owns Up/Down/Enter/Tab/Escape.
            if popup_state.borrow().is_visible() {
                match keyval {
                    gdk::Key::Escape => {
                        popup_state.borrow_mut().close();
                        return glib::Propagation::Stop;
                    }
                    gdk::Key::Down | gdk::Key::Up => {
                        let delta = if keyval == gdk::Key::Down { 1 } else { -1 };
                        popup_state.borrow_mut().move_selection(delta);
                        return glib::Propagation::Stop;
                    }
                    gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::Tab => {
                        let accepted = popup_state.borrow_mut().accept();
                        if accepted {
                            return glib::Propagation::Stop;
                        }
                    }
                    _ => {}
                }
            }
            if keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter {
                if modifier.contains(gdk::ModifierType::SHIFT_MASK) {
                    glib::Propagation::Proceed
                } else {
                    popup_state.borrow_mut().close();
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
        let update_btn_stop = update_send_btn_state.clone();

        btn_send_stop.connect_clicked(move |_| {
            if is_str_btn.get() {
                // Active streaming: Stop request
                state_stop.borrow_mut().harness.abort();
                if let Some(ref target) = *active_stream_stop.borrow() {
                    collapse_and_finalize_thinking(target);
                    if let Some(ref p) = *target.progress.borrow() {
                        let total_elapsed = p.start_time.elapsed().as_secs();
                        p.disclosure.set_title(&format!(
                            "Stopped after {} steps ({})",
                            p.steps_count,
                            format::format_duration_sec(total_elapsed)
                        ));
                        p.disclosure.set_expanded(false);
                        for entry in p.items.values() {
                            if entry.icon.has_css_class("running") {
                                entry.icon.remove_css_class("running");
                                entry.icon.set_icon_name(Some("codicon-error-symbolic"));
                                entry.icon.add_css_class("error");
                                entry.badge.set_text("stopped");
                            }
                        }
                    }
                }
                *active_stream_stop.borrow_mut() = None;
                is_str_btn.set(false);
                update_btn_stop();
            } else {
                submit_btn();
            }
        });
    }

    // Global Window Keyboard Shortcuts (Esc to close menu/popup/drawer, Ctrl+N for new chat, Ctrl+Shift+S for capture)
    {
        let key_controller = gtk4::EventControllerKey::new();
        let hide_fn = hide_overlay_fn.clone();
        let mh_esc = menu_host.clone();
        let mp_esc = mention_popup.clone();
        let req_nc_key = request_new_chat.clone();
        let btn_cap_key = btn_capture.clone();

        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            if key == gdk::Key::Escape {
                if mp_esc.borrow().is_visible() {
                    mp_esc.borrow_mut().close();
                    return glib::Propagation::Stop;
                }
                if mh_esc.is_menu_open() {
                    mh_esc.close_menu();
                    return glib::Propagation::Stop;
                }
                hide_fn();
                return glib::Propagation::Stop;
            }
            if modifier.contains(gdk::ModifierType::CONTROL_MASK) && (key == gdk::Key::n || key == gdk::Key::N) {
                req_nc_key();
                return glib::Propagation::Stop;
            }
            if modifier.contains(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK)
                && (key == gdk::Key::S || key == gdk::Key::s)
            {
                btn_cap_key.emit_clicked();
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
        let mh_focus = menu_host.clone();
        let mp_focus = mention_popup.clone();
        let state_focus = state.clone();
        let had_focus = Rc::new(Cell::new(false));

        focus_controller.connect_contains_focus_notify(move |controller| {
            let has_focus = controller.contains_focus();
            let prev = had_focus.get();
            had_focus.set(has_focus);

            if prev && !has_focus {
                let auto_hide = state_focus.borrow().settings.auto_hide_on_blur;
                if auto_hide {
                    mh_focus.close_menu();
                    mp_focus.borrow_mut().close();
                    hide_fn();
                }
            }
        });
        window.add_controller(focus_controller);
    }

    // Start IPC socket listener (skipped in audit mode so it never disturbs a live daemon)
    if audit_dir.is_none() {
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
    }

    // Expand overlay on startup unless launched as hidden daemon
    if !is_daemon {
        expand_overlay_fn();
    }

    #[cfg(feature = "audit")]
    if let Some(dir) = audit_dir {
        window.set_default_size(initial_settings.width, 820);
        window.set_size_request(initial_settings.width, 820);
        state.borrow_mut().settings.auto_hide_on_blur = false;

        let app_ref = app.clone();
        let win_ref = window.clone();
        let tv_ref = text_view.clone();
        let chips_ref = chips_row.clone();
        let state_ref = state.clone();
        let update_btn_ref = update_send_btn_state.clone();
        let mh_audit = menu_host.clone();
        let mp_ref = mention_popup.clone();
        let btn_cap_ref = btn_capture.clone();
        let cap_box_audit = capture_box.clone();
        let btn_more_ref = btn_more.clone();
        let more_stack_audit = more_stack.clone();
        let btn_sess_ref = btn_session_list.clone();
        let sess_stack_audit = session_stack.clone();
        let msg_box_ref = messages_box.clone();
        let chat_stack_ref = chat_stack.clone();
        let p_empty_ref = prompt_empty_label.clone();
        let p_list_ref = prompts_list_box.clone();
        let active_stream_ref = active_stream.clone();
        let stream_tx_ref = stream_tx.clone();
        let scroll_ref = scroll_to_bottom.clone();
        let is_stream_ref = is_streaming.clone();

        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            let out_dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&out_dir);

            // 1. Welcome
            set_text_view_content(&tv_ref, "");
            let _ = audit::snapshot_window_with_popovers(&win_ref, &[], &out_dir.join("after-welcome.png"));

            // 2. Mention (@screen / files) + attached chip
            state_ref.borrow_mut().attached_files.push(("src/main.rs".into(), String::new()));
            add_context_chip(&chips_ref, "main.rs", "src/main.rs", state_ref.clone(), Some(update_btn_ref.clone()));
            set_text_view_content(&tv_ref, "Look at @");
            {
                let buf = tv_ref.buffer();
                buf.place_cursor(&buf.end_iter());
            }
            mp_ref.borrow_mut().refresh();
            audit::settle_ui(60);
            let _ = audit::snapshot_window_with_popovers(
                &win_ref,
                &[],
                &out_dir.join("after-mention.png"),
            );
            mp_ref.borrow_mut().close();

            // 3. Slash commands
            set_text_view_content(&tv_ref, "/");
            {
                let buf = tv_ref.buffer();
                buf.place_cursor(&buf.end_iter());
            }
            mp_ref.borrow_mut().refresh();
            audit::settle_ui(60);
            let _ = audit::snapshot_window_with_popovers(
                &win_ref,
                &[],
                &out_dir.join("after-slash.png"),
            );
            mp_ref.borrow_mut().close();
            set_text_view_content(&tv_ref, "");

            // 4. Screen Capture floating menu
            mh_audit.open_menu("capture", &btn_cap_ref, &cap_box_audit, 240, menu::MenuPlacement::AboveRight);
            audit::settle_ui(80);
            let _ = audit::snapshot_window_with_popovers(
                &win_ref,
                &[],
                &out_dir.join("after-capture.png"),
            );
            mh_audit.close_menu();

            // 5. Settings floating menu
            more_stack_audit.set_visible_child_name("main");
            mh_audit.open_menu("more", &btn_more_ref, &more_stack_audit, 260, menu::MenuPlacement::BelowRight);
            audit::settle_ui(80);
            let _ = audit::snapshot_window_with_popovers(
                &win_ref,
                &[],
                &out_dir.join("after-settings.png"),
            );
            mh_audit.close_menu();

            // 6. Conversation + Tool + Code + Table + Permission (synthetic, no CLI spawn)
            chat_stack_ref.set_visible_child_name("messages");
            let gen = state_ref.borrow().harness.generation();

            let u_card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            u_card.add_css_class("card-user");
            u_card.set_halign(gtk4::Align::Fill);
            let chips_u = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            chips_u.add_css_class("user-attached-chips");
            let c_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            c_box.add_css_class("user-attached-chip");
            let c_ic = icons::image("codicon-file-code-symbolic", 11);
            c_ic.add_css_class("user-attached-chip-icon");
            c_box.append(&c_ic);
            c_box.append(&gtk4::Label::new(Some("main.rs")));
            chips_u.append(&c_box);
            u_card.append(&chips_u);
            u_card.append(
                &gtk4::Label::builder()
                    .label("Refactor the UI so it matches VS Code Chat layout cleanly.")
                    .wrap(true)
                    .wrap_mode(gtk4::pango::WrapMode::WordChar)
                    .width_chars(1)
                    .max_width_chars(64)
                    .xalign(0.0)
                    .css_classes(["card-user-text"])
                    .build(),
            );
            msg_box_ref.append(&u_card);

            let a_card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            a_card.add_css_class("card-assistant");
            a_card.set_halign(gtk4::Align::Fill);
            let a_hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            a_hdr.add_css_class("card-assistant-header");
            let av_wrap = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            av_wrap.add_css_class("card-assistant-avatar");
            av_wrap.set_size_request(24, 24);
            av_wrap.set_hexpand(false);
            av_wrap.set_vexpand(false);
            av_wrap.set_halign(gtk4::Align::Start);
            av_wrap.set_valign(gtk4::Align::Center);
            let av_ic = icons::image("codicon-sparkle-symbolic", 13);
            av_ic.set_hexpand(true);
            av_ic.set_halign(gtk4::Align::Center);
            av_ic.set_valign(gtk4::Align::Center);
            av_wrap.append(&av_ic);
            a_hdr.append(&av_wrap);
            a_hdr.append(
                &gtk4::Label::builder()
                    .label("Assistant")
                    .css_classes(["card-assistant-author"])
                    .build(),
            );
            a_hdr.append(
                &gtk4::Label::builder()
                    .label("Auto")
                    .css_classes(["card-assistant-model"])
                    .build(),
            );
            a_card.append(&a_hdr);

            let t_body = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            a_card.append(&t_body);

            let fb = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
            fb.add_css_class("card-feedback-bar");
            fb.append(&icons::icon_button("codicon-copy-symbolic", 13, "Copy", &["feedback-btn"]));
            fb.append(&icons::icon_button("codicon-refresh-symbolic", 13, "Retry", &["feedback-btn"]));
            let fb_sp = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            fb_sp.set_hexpand(true);
            fb.append(&fb_sp);
            let m_badge = gtk4::Label::builder()
                .label("142 tok · 1.8s")
                .css_classes(["card-metrics-pill"])
                .build();
            fb.append(&m_badge);
            a_card.append(&fb);
            msg_box_ref.append(&a_card);

            p_empty_ref.set_visible(false);
            let h_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
            h_row.add_css_class("prompt-row-box");
            let h_btn = gtk4::Button::builder().css_classes(["prompt-item-btn"]).hexpand(true).build();
            let h_inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            h_inner.append(&gtk4::Label::builder().label("#1").css_classes(["prompt-idx-pill"]).build());
            h_inner.append(&gtk4::Label::builder().label("Refactor the UI so it looks…").css_classes(["prompt-snippet-text"]).hexpand(true).xalign(0.0).build());
            h_btn.set_child(Some(&h_inner));
            h_row.append(&h_btn);
            h_row.append(&icons::icon_button("codicon-trash-symbolic", 12, "Delete", &["prompt-delete-btn"]));
            p_list_ref.append(&h_row);

            *active_stream_ref.borrow_mut() = Some(ActiveStreamTarget {
                turn_body: t_body,
                full_text: Rc::new(RefCell::new(String::new())),
                thinking: Rc::new(RefCell::new(None)),
                progress: Rc::new(RefCell::new(None)),
                active_markdown: Rc::new(RefCell::new(None)),
                metrics_badge: m_badge,
            });
            is_stream_ref.set(true);
            update_btn_ref();
            let _ = stream_tx_ref.send((gen, StreamEvent::ToolStart { id: "t1".into(), name: "read_file".into(), args: r#"{"path":"resources/style.css"}"#.into() }));
            let _ = stream_tx_ref.send((gen, StreamEvent::ToolEnd { id: "t1".into(), name: "read_file".into(), result: "".into(), is_error: false, duration_ms: 1200 }));
            let _ = stream_tx_ref.send((gen, StreamEvent::ToolStart { id: "t2".into(), name: "bash".into(), args: r#"{"command":"cargo test --quiet"}"#.into() }));
            audit::settle_ui(150);
            let _ = audit::snapshot_window_with_popovers(&win_ref, &[], &out_dir.join("after-streaming.png"));

            let _ = stream_tx_ref.send((gen, StreamEvent::ToolEnd { id: "t2".into(), name: "bash".into(), result: "".into(), is_error: false, duration_ms: 800 }));
            let _ = stream_tx_ref.send((gen, StreamEvent::ThinkingDelta("Inspecting resources/style.css and composer hierarchy...".into())));
            let _ = stream_tx_ref.send((gen, StreamEvent::TextDelta("Updated `.chat-input-container` with unified borders and bundled Codicon SVGs:\n\n```css\n.chat-input-container {\n  background: #262626;\n  border-radius: 8px;\n}\n```\n\n| Component | Status |\n| --- | --- |\n| Composer | Unified |\n| Icons | Codicon |\n".into())));
            let _ = stream_tx_ref.send((gen, StreamEvent::PermissionRequest(backend::PermissionRequest {
                request_id: 99,
                title: "shell command".into(),
                detail: "cargo test --workspace".into(),
                options: vec![("allow_once".into(), "Allow".into()), ("reject_once".into(), "Skip".into())],
            })));
            let _ = stream_tx_ref.send((gen, StreamEvent::Done));

            audit::settle_ui(180);
            scroll_ref();
            let _ = audit::snapshot_window_with_popovers(&win_ref, &[], &out_dir.join("after-conversation.png"));

            // 7. History floating menu
            sess_stack_audit.set_visible_child_name("list");
            mh_audit.open_menu("session", &btn_sess_ref, &sess_stack_audit, 260, menu::MenuPlacement::BelowRight);
            audit::settle_ui(80);
            let _ = audit::snapshot_window_with_popovers(
                &win_ref,
                &[],
                &out_dir.join("after-history.png"),
            );
            mh_audit.close_menu();

            win_ref.close();
            app_ref.quit();
        });
    }

    AppContext {
        window,
        state,
        _hold_guard: hold_guard,
    }
}

/// Merges a harness-reported catalog into the app settings.
///
/// Only fills gaps and never clobbers a user choice, so switching harnesses
/// does not silently reset the composer.
fn apply_session_config(state: &Rc<RefCell<AppState>>, cfg: &backend::SessionConfig) {
    let mut st = state.borrow_mut();

    if !cfg.models.is_empty() {
        st.available_models = cfg.models.clone();
    }
    if !cfg.modes.is_empty() {
        st.available_modes = cfg.modes.clone();
    }
    if !cfg.thinking_levels.is_empty() {
        st.available_thinking_levels = cfg.thinking_levels.clone();
    }
    if !cfg.commands.is_empty() {
        // Harness commands join the `/` popover alongside the client's own.
        st.slash_commands
            .borrow_mut()
            .clone_from(&mentions::merge_commands(&cfg.commands));
    }

    if st.settings.model.is_empty() && !cfg.current_model.is_empty() {
        st.settings.model = cfg.current_model.clone();
    }
    if !cfg.current_mode.is_empty() {
        st.current_mode = cfg.current_mode.clone();
        st.settings.acp_mode = cfg.current_mode.clone();
        // Auto mode runs tools without prompting, matching the agent's view.
        let auto = st.settings.auto_approve_permissions || cfg.current_mode == "auto";
        st.harness.set_auto_approve(auto);
    }
}

/// Inline approval card shown when the agent asks to use a tool.
fn build_permission_card(
    req: &backend::PermissionRequest,
    state: &Rc<RefCell<AppState>>,
) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    card.add_css_class("chat-permission-card");

    let hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let icon = icons::image("codicon-shield-symbolic", 14);
    icon.add_css_class("permission-icon");
    hdr.append(&icon);

    let title = gtk4::Label::builder()
        .label(format!("Allow {}?", req.title))
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk4::pango::WrapMode::WordChar)
        .hexpand(true)
        .css_classes(["permission-title"])
        .build();
    hdr.append(&title);
    card.append(&hdr);

    if !req.detail.trim().is_empty() {
        let detail = gtk4::Label::builder()
            .label(&req.detail)
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::Char)
            .selectable(true)
            .hexpand(true)
            .css_classes(["permission-detail"])
            .build();
        card.append(&detail);
    }

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    actions.add_css_class("permission-actions");

    let request_id = req.request_id;
    for (option_id, label) in &req.options {
        let btn = gtk4::Button::builder()
            .label(label)
            .css_classes(["permission-btn"])
            .build();
        if option_id == "allow_once" || option_id == "allow" {
            btn.add_css_class("permission-btn-primary");
        }

        let option = option_id.clone();
        let card_clone = card.clone();
        let state_clone = state.clone();
        btn.connect_clicked(move |_| {
            // The harness ignores this for Pi; only ACP agents ask.
            state_clone
                .borrow()
                .harness
                .respond_permission(request_id, &option);
            card_clone.set_visible(false);
        });
        actions.append(&btn);
    }

    card.append(&actions);
    card
}

/// Autocomplete popover for `@` / `#` mentions and `/` commands.
///
/// Owns the list, the selection index and the text replacement, so the text
/// view only has to forward keys and text changes.
struct MentionPopup {
    menu_host: Rc<menu::FloatingMenuHost>,
    composer_widget: gtk4::Widget,
    container: gtk4::Box,
    header_lbl: gtk4::Label,
    list: gtk4::ListBox,
    /// Currently offered items, in display order.
    items: Vec<MentionItemKind>,
    selected: usize,
    /// Byte range in the buffer that the accepted item replaces.
    replace_range: Option<(usize, usize)>,
    text_view: gtk4::TextView,
    workspace_index: Rc<RefCell<mentions::WorkspaceIndex>>,
    commands: Rc<RefCell<Vec<mentions::SlashCommand>>>,
}

/// What a popup row inserts when accepted.
#[derive(Clone, Debug)]
enum MentionItemKind {
    /// A file or context mention, inserted as text.
    Mention(mentions::MentionItem),
    /// A slash command; carries its name for dispatch.
    Command { name: String, description: String },
}

impl MentionPopup {
    fn is_visible(&self) -> bool {
        self.menu_host.is_suggest_visible()
    }

    fn close(&mut self) {
        self.selected = 0;
        self.replace_range = None;
        self.menu_host.hide_suggest();
    }

    /// Moves the highlight, wrapping at both ends.
    fn move_selection(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as i32;
        self.selected = ((self.selected as i32 + delta).rem_euclid(len)) as usize;
        self.list.select_row(self.list.row_at_index(self.selected as i32).as_ref());
    }

    /// Inserts the highlighted item. Returns whether anything was accepted.
    fn accept(&mut self) -> bool {
        let Some((start, end)) = self.replace_range else {
            self.close();
            return false;
        };
        let Some(item) = self.items.get(self.selected).cloned() else {
            self.close();
            return false;
        };

        let insert = match &item {
            MentionItemKind::Mention(m) => format!("{} ", m.insert),
            MentionItemKind::Command { name, .. } => format!("/{name} "),
        };

        let buffer = self.text_view.buffer();
        // Offsets come from the buffer, so they are already in characters.
        let mut iter_start = buffer.iter_at_offset(start as i32);
        let mut iter_end = buffer.iter_at_offset(end as i32);
        buffer.delete(&mut iter_start, &mut iter_end);
        buffer.insert(&mut buffer.iter_at_offset(start as i32), &insert);

        self.close();
        true
    }

    /// Recomputes the offered items for the current caret position.
    fn refresh(&mut self) {
        let buffer = self.text_view.buffer();
        let insert_mark = buffer.get_insert();
        let offset = buffer.iter_at_mark(&insert_mark).offset() as usize;
        // Only the current line matters, and the trigger is `@`/`#`/`/`.
        let text_before = {
            let line_start = buffer
                .iter_at_offset(insert_mark_offset_line_start(&buffer, offset))
                .offset() as usize;
            buffer
                .text(&buffer.iter_at_offset(line_start as i32), &buffer.iter_at_offset(offset as i32), false)
                .to_string()
        };

        let Some(trigger) = mentions::extract_active_trigger(&text_before) else {
            self.close();
            return;
        };

        // Character offsets within the buffer, not bytes.
        let query_chars = trigger.query.chars().count();
        let trigger_chars = 1;
        let start = offset.saturating_sub(query_chars + trigger_chars);
        let end = offset;

        match trigger.kind {
            mentions::TriggerKind::Slash => {
                self.header_lbl.set_label("SLASH COMMANDS");
                let commands = self.commands.borrow();
                let matches = mentions::filter_commands(&commands, &trigger.query);
                self.items = matches
                    .into_iter()
                    .map(|c| MentionItemKind::Command {
                        name: c.name,
                        description: c.description,
                    })
                    .collect();
            }
            mentions::TriggerKind::At | mentions::TriggerKind::Hash => {
                self.header_lbl.set_label("CONTEXT & FILES");
                let index = self.workspace_index.borrow();
                let mut items = index.suggest(&trigger.query, 12);
                // `#` is for context only, so files are filtered out.
                if trigger.kind == mentions::TriggerKind::Hash {
                    items.retain(|i| i.is_context);
                }
                self.items = items.into_iter().map(MentionItemKind::Mention).collect();
            }
        }

        if self.items.is_empty() {
            self.close();
            return;
        }

        self.replace_range = Some((start, end));
        self.selected = 0;
        self.render_rows();
        self.menu_host.show_suggest(&self.composer_widget, &self.container);
    }

    /// Rebuilds the row widgets for `self.items`.
    fn render_rows(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        for item in &self.items {
            let row = gtk4::ListBoxRow::new();
            row.add_css_class("mention-row");

            let outer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            outer.set_margin_top(4);
            outer.set_margin_bottom(4);
            outer.set_margin_start(8);
            outer.set_margin_end(8);

            let (icon_name, label, detail) = match item {
                MentionItemKind::Mention(m) => {
                    let ic = match m.insert.as_str() {
                        "@screen" => "codicon-device-camera-symbolic",
                        "@markup" => "codicon-edit-symbolic",
                        _ if m.is_context => "codicon-sparkle-symbolic",
                        _ => "codicon-file-code-symbolic",
                    };
                    (ic, m.label.clone(), m.detail.clone())
                }
                MentionItemKind::Command { name, description } => (
                    "codicon-symbol-namespace-symbolic",
                    format!("/{name}"),
                    description.clone(),
                ),
            };

            let ic_img = icons::image(icon_name, 14);
            ic_img.add_css_class("mention-row-icon");
            outer.append(&ic_img);

            outer.append(
                &gtk4::Label::builder()
                    .label(&label)
                    .xalign(0.0)
                    .css_classes(["mention-label"])
                    .build(),
            );

            if !detail.is_empty() {
                outer.append(
                    &gtk4::Label::builder()
                        .label(&detail)
                        .xalign(1.0)
                        .hexpand(true)
                        .ellipsize(gtk4::pango::EllipsizeMode::End)
                        .css_classes(["mention-detail"])
                        .build(),
                );
            }
            row.set_child(Some(&outer));
            self.list.append(&row);
        }

        self.list.select_row(self.list.row_at_index(0).as_ref());
    }
}

/// Synchronizes the single active click-through `AgentMarkupOverlay` with any
/// `[[markup: ...]]` tags parsed from `text` so annotations stream live.
fn sync_live_markup_overlay(
    app: &gtk4::Application,
    slot: &Rc<RefCell<Option<markup::AgentMarkupOverlay>>>,
    text: &str,
) -> Vec<markup::MarkupShape> {
    let shapes = markup::parse_agent_markup(text);
    if shapes.is_empty() {
        return shapes;
    }
    let mut guard = slot.borrow_mut();
    if let Some(ref existing) = *guard {
        if existing.is_open() {
            existing.update_shapes(shapes.clone());
            return shapes;
        }
    }
    let overlay = markup::AgentMarkupOverlay::new(app, shapes.clone());
    overlay.present();
    *guard = Some(overlay);
    shapes
}

/// Character offset of the start of the line containing `offset`.
fn insert_mark_offset_line_start(buffer: &gtk4::TextBuffer, offset: usize) -> i32 {
    let mut iter = buffer.iter_at_offset(offset as i32);
    iter.set_line_offset(0);
    iter.offset()
}

#[cfg(test)]
mod tests {
    use super::*;

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
