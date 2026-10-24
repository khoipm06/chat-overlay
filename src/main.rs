mod backend;
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
use std::rc::Rc;
use tokio::sync::mpsc;

const APP_ID: &str = "com.github.khoipm.pi-overlay";

pub const AVAILABLE_THEMES: &[(&str, &str, &str)] = &[
    ("theme-dark-glass", "Dark Glass", "Translucent dark glass"),
    (
        "theme-dracula",
        "Dracula",
        "Official Dracula vampire dark theme",
    ),
    (
        "theme-catppuccin-mocha",
        "Catppuccin Mocha",
        "Soothing dark pastel (Mocha)",
    ),
    (
        "theme-catppuccin-macchiato",
        "Catppuccin Macchiato",
        "Medium dark pastel (Macchiato)",
    ),
    (
        "theme-catppuccin-frappe",
        "Catppuccin Frappé",
        "Muted dark pastel (Frappé)",
    ),
    (
        "theme-catppuccin-latte",
        "Catppuccin Latte",
        "Light pastel flavor (Latte)",
    ),
    ("theme-oled", "OLED Black", "Pure deep black"),
    ("theme-slate", "Slate Graphite", "Deep slate & graphite"),
    ("theme-forest", "Forest Charcoal", "Obsidian & subtle green"),
    ("theme-zinc-light", "Zinc Light", "Minimal bright zinc"),
    (
        "theme-tokyo-night",
        "Tokyo Night",
        "Deep blue-black neon night",
    ),
    (
        "theme-nord",
        "Nord Arctic",
        "Arctic cool north-bluish palette",
    ),
    (
        "theme-gruvbox-dark",
        "Gruvbox Dark",
        "Retro groove warm earthy dark",
    ),
    (
        "theme-one-dark",
        "One Dark Pro",
        "Atom / VSCode iconic deep dark",
    ),
    (
        "theme-solarized-dark",
        "Solarized Dark",
        "Precision engineered teal & amber",
    ),
    (
        "theme-solarized-light",
        "Solarized Light",
        "Warm paper cream solarized light",
    ),
];
const THEMES: &[(&str, &str, &str)] = AVAILABLE_THEMES;

const POPULAR_MODELS: &[(&str, &str)] = &[
    ("default", "Default (System preset)"),
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

struct TrackedPrompt {
    #[allow(dead_code)]
    index: usize,
    #[allow(dead_code)]
    text: String,
    #[allow(dead_code)]
    widget: gtk4::Box,
    #[allow(dead_code)]
    bubble: gtk4::Box,
}

type ActiveThoughtState = Option<(gtk4::Expander, gtk4::Label, Rc<RefCell<String>>)>;

struct ActiveStreamTarget {
    label: gtk4::Label,
    parent_expander: gtk4::Expander,
    steps_expander: gtk4::Expander,
    timeline_box: gtk4::Box,
    active_thought_expander: Rc<RefCell<ActiveThoughtState>>,
    start_time: std::time::Instant,
    metrics_badge: gtk4::Label,
    full_text: Rc<RefCell<String>>,
    tool_count: Rc<Cell<usize>>,
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
}

struct AppContext {
    #[allow(dead_code)]
    window: gtk4::ApplicationWindow,
    toggle: Rc<dyn Fn()>,
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // Fast path: if an instance is already running, toggle it and exit immediately (< 2ms)
    if ipc::try_send_toggle() {
        return glib::ExitCode::SUCCESS;
    }

    // If explicit --toggle requested but no running instance, return failure
    if args.contains(&"--toggle".to_string()) {
        eprintln!("[pi-overlay] Overlay is not running.");
        return glib::ExitCode::FAILURE;
    }

    // Suppress benign GtkGizmo slider sizing warnings from upstream GTK4
    glib::log_set_writer_func(|level, fields| {
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
        glib::log_writer_default(level, fields)
    });

    // Auto-register KDE global shortcut Ctrl+Alt+L if on KDE Plasma
    ensure_kde_global_shortcut();

    // Initialize Tokio runtime for background IPC and subprocesses
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize tokio multi-thread runtime");
    let _guard = rt.enter();

    let app = gtk4::Application::builder().application_id(APP_ID).build();

    app.connect_startup(|_| {
        load_css();
    });

    let context_holder: Rc<RefCell<Option<AppContext>>> = Rc::new(RefCell::new(None));
    let ctx_clone = context_holder.clone();

    app.connect_activate(move |app| {
        let mut ctx = ctx_clone.borrow_mut();
        if let Some(ref c) = *ctx {
            (c.toggle)();
        } else {
            let new_ctx = build_ui(app);
            *ctx = Some(new_ctx);
        }
    });

    app.run()
}

fn ensure_kde_global_shortcut() {
    let desktop_dir = match std::env::var("XDG_DATA_HOME") {
        Ok(d) => std::path::PathBuf::from(d).join("applications"),
        Err(_) => match std::env::var("HOME") {
            Ok(h) => std::path::PathBuf::from(h).join(".local/share/applications"),
            Err(_) => return,
        },
    };

    let _ = std::fs::create_dir_all(&desktop_dir);
    let desktop_file = desktop_dir.join("pi-overlay.desktop");

    if let Ok(exe_path) = std::env::current_exe() {
        let exe_str = exe_path.to_string_lossy();
        let content = format!(
            "[Desktop Entry]\nName=Pi\nExec={}\nType=Application\nX-KDE-GlobalAccel-CommandShortcut=true\nNoDisplay=true\nStartupNotify=false\n",
            exe_str
        );
        let _ = std::fs::write(&desktop_file, content);

        let _ = std::process::Command::new("kbuildsycoca6").status();

        let _ = std::process::Command::new("kwriteconfig6")
            .args([
                "--file",
                "kglobalshortcutsrc",
                "--group",
                "services",
                "--group",
                "pi-overlay.desktop",
                "--key",
                "_launch",
                "Ctrl+Alt+L,none,Pi",
            ])
            .status();

        let _ = std::process::Command::new("busctl")
            .args([
                "--user",
                "call",
                "org.kde.kglobalaccel",
                "/kglobalaccel",
                "org.kde.KGlobalAccel",
                "doRegister",
                "as",
                "4",
                "pi-overlay.desktop",
                "_launch",
                "Pi",
                "Pi",
            ])
            .status();

        let _ = std::process::Command::new("busctl")
            .args([
                "--user",
                "call",
                "org.kde.kglobalaccel",
                "/kglobalaccel",
                "org.kde.KGlobalAccel",
                "setForeignShortcut",
                "asai",
                "4",
                "pi-overlay.desktop",
                "_launch",
                "Pi",
                "Pi",
                "1",
                "201326668",
            ])
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
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn build_ui(app: &gtk4::Application) -> AppContext {
    let initial_settings = AppSettings::load();

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("Pi")
        .default_width(initial_settings.width)
        .build();

    overlay::init_layer_shell(&window);

    // Root overlay container: Drag resize handle + drawer directly in root_box
    let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root_box.set_halign(gtk4::Align::End);
    root_box.set_vexpand(true);

    // Multi-directional Resize Frame Container
    // Left edge resize handle
    let resize_handle_w = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    resize_handle_w.add_css_class("resize-handle");
    resize_handle_w.set_cursor_from_name(Some("ew-resize"));
    resize_handle_w.set_width_request(12);
    resize_handle_w.set_vexpand(true);

    // Visual grip bar
    let resize_grip = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    resize_grip.add_css_class("resize-grip");
    resize_grip.set_valign(gtk4::Align::Center);
    resize_grip.set_vexpand(true);
    resize_handle_w.append(&resize_grip);

    root_box.append(&resize_handle_w);

    // Center Vertical Box holding top handle, drawer, bottom handle
    let center_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    center_box.set_vexpand(true);

    // Top edge resize handle
    let resize_handle_n = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    resize_handle_n.add_css_class("resize-handle-horiz");
    resize_handle_n.set_cursor_from_name(Some("ns-resize"));
    resize_handle_n.set_height_request(8);
    center_box.append(&resize_handle_n);

    let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    drawer.set_widget_name("drawer-container");
    drawer.add_css_class(&initial_settings.theme);
    drawer.set_width_request(initial_settings.width);
    drawer.set_vexpand(true);

    // Header Area: Title + Model Badge + Status + Action Buttons
    let header_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header_box.add_css_class("header-box");

    let title_label = gtk4::Label::new(Some("Pi"));
    title_label.add_css_class("title-label");
    title_label.set_halign(gtk4::Align::Start);
    header_box.append(&title_label);

    let model_badge = gtk4::Label::new(Some(&initial_settings.model));
    model_badge.add_css_class("model-badge");
    model_badge.set_halign(gtk4::Align::Start);
    header_box.append(&model_badge);

    let status_label = gtk4::Label::new(Some("ready"));
    status_label.add_css_class("status-badge");
    status_label.set_halign(gtk4::Align::Start);
    header_box.append(&status_label);

    let header_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    header_spacer.set_hexpand(true);
    header_box.append(&header_spacer);

    // Prompt Navigator Button & Popover
    let btn_prompts = gtk4::MenuButton::builder()
        .label("Prompts (0)")
        .css_classes(["btn-header"])
        .tooltip_text("Jump to previous prompts")
        .build();

    let prompt_popover = gtk4::Popover::new();
    prompt_popover.add_css_class("prompt-popover");

    let popover_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let popover_title = gtk4::Label::builder()
        .label("Session Prompts")
        .css_classes(["popover-header-title"])
        .xalign(0.0)
        .build();
    popover_box.append(&popover_title);

    let nav_scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .max_content_height(260)
        .propagate_natural_height(true)
        .build();

    let prompt_list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    prompt_list_box.set_width_request(240);

    let prompt_empty_placeholder = gtk4::Label::builder()
        .label("No prompts yet in this session")
        .css_classes(["prompt-nav-text"])
        .xalign(0.0)
        .margin_top(8)
        .margin_bottom(8)
        .margin_start(4)
        .build();
    prompt_list_box.append(&prompt_empty_placeholder);

    nav_scrolled.set_child(Some(&prompt_list_box));
    popover_box.append(&nav_scrolled);
    prompt_popover.set_child(Some(&popover_box));
    btn_prompts.set_popover(Some(&prompt_popover));
    header_box.append(&btn_prompts);

    // Settings Menu Button & Popover (Model, Theme, Auto-hide, Reset)
    let btn_settings = gtk4::MenuButton::builder()
        .label("Settings")
        .css_classes(["btn-header"])
        .tooltip_text("Configure model, theme and behavior")
        .build();

    let settings_popover = gtk4::Popover::new();
    settings_popover.add_css_class("settings-popover");

    let settings_pop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    settings_pop_box.set_width_request(220);

    let settings_title = gtk4::Label::builder()
        .label("Settings")
        .css_classes(["popover-header-title"])
        .xalign(0.0)
        .build();
    settings_pop_box.append(&settings_title);

    // Model Selector Section
    let model_section_label = gtk4::Label::builder()
        .label("Model")
        .css_classes(["settings-section-label"])
        .xalign(0.0)
        .build();
    settings_pop_box.append(&model_section_label);

    let model_dropdown = gtk4::DropDown::from_strings(
        &POPULAR_MODELS
            .iter()
            .map(|(_, name)| *name)
            .collect::<Vec<&str>>(),
    );
    // Find current index
    let current_model_idx = POPULAR_MODELS
        .iter()
        .position(|(id, _)| *id == initial_settings.model)
        .unwrap_or(0);
    model_dropdown.set_selected(current_model_idx as u32);
    settings_pop_box.append(&model_dropdown);

    // Theme Selector Section
    let theme_section_label = gtk4::Label::builder()
        .label("Theme")
        .css_classes(["settings-section-label"])
        .xalign(0.0)
        .build();
    settings_pop_box.append(&theme_section_label);

    let theme_dropdown = gtk4::DropDown::from_strings(
        &THEMES
            .iter()
            .map(|(_, name, _)| *name)
            .collect::<Vec<&str>>(),
    );
    let current_theme_idx = THEMES
        .iter()
        .position(|(id, _, _)| *id == initial_settings.theme)
        .unwrap_or(0);
    theme_dropdown.set_selected(current_theme_idx as u32);
    settings_pop_box.append(&theme_dropdown);

    // Auto-hide Checkbox
    let auto_hide_check = gtk4::CheckButton::builder()
        .label("Auto-hide on focus loss")
        .active(initial_settings.auto_hide_on_blur)
        .css_classes(["settings-checkbox"])
        .build();
    settings_pop_box.append(&auto_hide_check);

    // Ephemeral session Checkbox
    let ephemeral_check = gtk4::CheckButton::builder()
        .label("Ephemeral session (clear on hide)")
        .active(initial_settings.ephemeral)
        .css_classes(["settings-checkbox"])
        .build();
    settings_pop_box.append(&ephemeral_check);

    settings_popover.set_child(Some(&settings_pop_box));
    btn_settings.set_popover(Some(&settings_popover));
    header_box.append(&btn_settings);

    // Clear Button
    let btn_clear = gtk4::Button::builder()
        .label("Clear")
        .css_classes(["btn-header"])
        .tooltip_text("Clear conversation (Ctrl+L)")
        .build();
    header_box.append(&btn_clear);

    // Close Button (hides overlay completely)
    let btn_close = gtk4::Button::builder()
        .label("✕")
        .css_classes(["btn-header"])
        .tooltip_text("Close overlay (Esc)")
        .build();
    header_box.append(&btn_close);

    drawer.append(&header_box);

    // Chat transcript scroll area
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .css_classes(["chat-scroll"])
        .build();

    let messages_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    messages_box.add_css_class("messages-container");
    messages_box.set_vexpand(true);
    messages_box.set_valign(gtk4::Align::Start);

    // Welcome placeholder: minimal refined hints
    let welcome_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    welcome_box.add_css_class("welcome-box");
    welcome_box.set_halign(gtk4::Align::Center);
    welcome_box.set_valign(gtk4::Align::Center);
    welcome_box.set_vexpand(true);

    let welcome_title = gtk4::Label::builder()
        .label("Pi Ephemeral Assistant")
        .css_classes(["welcome-title"])
        .xalign(0.5)
        .build();
    welcome_box.append(&welcome_title);

    let welcome_sub = gtk4::Label::builder()
        .label("Type a query below • Ctrl+Alt+L to toggle • Esc to close")
        .css_classes(["welcome-subtitle"])
        .xalign(0.5)
        .build();
    welcome_box.append(&welcome_sub);

    messages_box.append(&welcome_box);
    scrolled.set_child(Some(&messages_box));
    drawer.append(&scrolled);

    // Input Area: Entry + Send button
    let input_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    input_box.add_css_class("input-box");

    let entry = gtk4::Entry::builder()
        .placeholder_text("Ask anything... (Enter to send, Esc to close)")
        .hexpand(true)
        .css_classes(["prompt-entry"])
        .build();
    input_box.append(&entry);

    let btn_send = gtk4::Button::builder()
        .label("Send")
        .css_classes(["btn-send"])
        .build();
    input_box.append(&btn_send);

    drawer.append(&input_box);

    // Footer with refined keyboard shortcut hints
    let footer_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    footer_box.add_css_class("footer-box");
    let shortcut_hint = gtk4::Label::new(Some(
        "Drag borders to resize  •  Ctrl+Alt+L: Toggle  •  Esc: Close",
    ));
    shortcut_hint.add_css_class("shortcut-label");
    shortcut_hint.set_halign(gtk4::Align::Center);
    shortcut_hint.set_hexpand(true);
    footer_box.append(&shortcut_hint);
    drawer.append(&footer_box);

    center_box.append(&drawer);

    // Bottom edge resize handle
    let resize_handle_s = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    resize_handle_s.add_css_class("resize-handle-horiz");
    resize_handle_s.set_cursor_from_name(Some("ns-resize"));
    resize_handle_s.set_height_request(8);
    center_box.append(&resize_handle_s);

    root_box.append(&center_box);
    window.set_child(Some(&root_box));

    let state = Rc::new(RefCell::new(AppState {
        mode: OverlayMode::Hidden,
        runner: PiRunner::new(),
        prompts: Vec::new(),
        settings: initial_settings,
        top_margin: 12,
        bottom_margin: 12,
    }));

    // Connect Dropdown callbacks to persist settings
    {
        let state_model = state.clone();
        let model_badge_clone = model_badge.clone();
        model_dropdown.connect_selected_notify(move |dd| {
            let idx = dd.selected() as usize;
            if idx < POPULAR_MODELS.len() {
                let model_id = POPULAR_MODELS[idx].0.to_string();
                model_badge_clone.set_text(&model_id);
                let mut st = state_model.borrow_mut();
                st.settings.model = model_id;
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

        let state_autohide = state.clone();
        auto_hide_check.connect_toggled(move |cb| {
            let mut st = state_autohide.borrow_mut();
            st.settings.auto_hide_on_blur = cb.is_active();
            let _ = st.settings.save();
        });

        let state_ephemeral = state.clone();
        ephemeral_check.connect_toggled(move |cb| {
            let mut st = state_ephemeral.borrow_mut();
            st.settings.ephemeral = cb.is_active();
            let _ = st.settings.save();
        });
    }

    // Setup Horizontal Drag Gesture (West Handle)
    {
        let drag_gesture_w = gtk4::GestureDrag::new();
        let initial_width = Rc::new(Cell::new(settings::DEFAULT_WIDTH));
        let start_w = initial_width.clone();
        let state_drag = state.clone();
        drag_gesture_w.connect_drag_begin(move |_, _, _| {
            start_w.set(state_drag.borrow().settings.width);
        });

        let drawer_drag = drawer.clone();
        let window_drag = window.clone();
        let cur_w = initial_width.clone();
        let state_update = state.clone();
        drag_gesture_w.connect_drag_update(move |_, offset_x, _| {
            let target_width =
                (cur_w.get() - offset_x as i32).clamp(settings::MIN_WIDTH, settings::MAX_WIDTH);

            let mut st = state_update.borrow_mut();
            st.settings.width = target_width;
            let _ = st.settings.save();
            drawer_drag.set_width_request(target_width);
            window_drag.set_default_size(target_width + 12, -1);
            window_drag.queue_resize();
        });
        resize_handle_w.add_controller(drag_gesture_w);
    }

    // Setup Top Vertical Drag Gesture (North Handle)
    {
        let drag_gesture_n = gtk4::GestureDrag::new();
        let initial_margin_top = Rc::new(Cell::new(12));
        let start_top = initial_margin_top.clone();
        let state_top = state.clone();
        drag_gesture_n.connect_drag_begin(move |_, _, _| {
            start_top.set(state_top.borrow().top_margin);
        });

        let window_top = window.clone();
        let cur_top = initial_margin_top.clone();
        let state_update_top = state.clone();
        drag_gesture_n.connect_drag_update(move |_, _, offset_y| {
            let new_top = (cur_top.get() + offset_y as i32).clamp(8, 600);
            let mut st = state_update_top.borrow_mut();
            st.top_margin = new_top;
            st.settings.margin_top = new_top;
            let _ = st.settings.save();
            overlay::update_margins(
                &window_top,
                new_top,
                st.bottom_margin,
                st.settings.margin_right,
            );
        });
        resize_handle_n.add_controller(drag_gesture_n);
    }

    // Setup Bottom Vertical Drag Gesture (South Handle)
    {
        let drag_gesture_s = gtk4::GestureDrag::new();
        let initial_margin_bottom = Rc::new(Cell::new(12));
        let start_bottom = initial_margin_bottom.clone();
        let state_bottom = state.clone();
        drag_gesture_s.connect_drag_begin(move |_, _, _| {
            start_bottom.set(state_bottom.borrow().bottom_margin);
        });

        let window_bottom = window.clone();
        let cur_bottom = initial_margin_bottom.clone();
        let state_update_bottom = state.clone();
        drag_gesture_s.connect_drag_update(move |_, _, offset_y| {
            let new_bottom = (cur_bottom.get() - offset_y as i32).clamp(8, 600);
            let mut st = state_update_bottom.borrow_mut();
            st.bottom_margin = new_bottom;
            overlay::update_margins(
                &window_bottom,
                st.top_margin,
                new_bottom,
                st.settings.margin_right,
            );
        });
        resize_handle_s.add_controller(drag_gesture_s);
    }

    let active_stream: Rc<RefCell<Option<ActiveStreamTarget>>> = Rc::new(RefCell::new(None));

    let autoscroll_enabled = Rc::new(Cell::new(true));

    // Auto-scroll to bottom whenever content size changes if autoscroll is active
    {
        let autoscroll = autoscroll_enabled.clone();
        scrolled.vadjustment().connect_changed(move |adj| {
            if autoscroll.get() {
                adj.set_value((adj.upper() - adj.page_size()).max(0.0));
            }
        });
    }

    // Smooth scroll to bottom function
    let scroll_to_bottom = {
        let scrolled = scrolled.clone();
        let autoscroll = autoscroll_enabled.clone();
        Rc::new(move || {
            autoscroll.set(true);
            let scrolled_clone = scrolled.clone();
            glib::idle_add_local_once(move || {
                let vadj = scrolled_clone.vadjustment();
                vadj.set_value((vadj.upper() - vadj.page_size()).max(0.0));
            });
        })
    };

    // Scroll to specific widget function (for Prompt Navigator)
    let scroll_to_widget = {
        let scrolled = scrolled.clone();
        let messages_box = messages_box.clone();
        let autoscroll = autoscroll_enabled.clone();
        Rc::new(move |widget: &gtk4::Box| {
            autoscroll.set(false);
            if let Some((_, y)) = widget.translate_coordinates(&messages_box, 0.0, 0.0) {
                let scrolled_clone = scrolled.clone();
                glib::idle_add_local_once(move || {
                    let vadj = scrolled_clone.vadjustment();
                    vadj.set_value(y.max(0.0));
                });
            }
        })
    };

    // Clear all conversation state
    let clear_conversation = {
        let messages_box = messages_box.clone();
        let welcome_box = welcome_box.clone();
        let prompt_list_box = prompt_list_box.clone();
        let prompt_empty_placeholder = prompt_empty_placeholder.clone();
        let btn_prompts = btn_prompts.clone();
        let status_label = status_label.clone();
        let state = state.clone();
        let active_stream = active_stream.clone();
        let autoscroll = autoscroll_enabled.clone();
        let entry = entry.clone();

        Rc::new(move || {
            while let Some(child) = messages_box.first_child() {
                messages_box.remove(&child);
            }
            messages_box.append(&welcome_box);
            welcome_box.set_visible(true);

            state.borrow_mut().prompts.clear();

            while let Some(child) = prompt_list_box.first_child() {
                prompt_list_box.remove(&child);
            }
            prompt_list_box.append(&prompt_empty_placeholder);
            prompt_empty_placeholder.set_visible(true);
            btn_prompts.set_label("Prompts (0)");

            status_label.set_text("ready");
            status_label.remove_css_class("working");
            state.borrow_mut().runner.abort();
            *active_stream.borrow_mut() = None;
            autoscroll.set(true);
            entry.set_text("");
            entry.grab_focus();
        })
    };

    let just_expanded = Rc::new(Cell::new(false));

    // Hide overlay completely: mode = Hidden, unmap Wayland surface (100% click-through)
    let hide_overlay_fn = {
        let window = window.clone();
        let state = state.clone();
        let clear_conversation = clear_conversation.clone();

        Rc::new(move || {
            let is_ephemeral = {
                let mut st = state.borrow_mut();
                if st.mode == OverlayMode::Hidden {
                    return;
                }
                st.mode = OverlayMode::Hidden;
                st.settings.ephemeral
            };
            if is_ephemeral {
                clear_conversation();
            }
            overlay::hide_overlay(&window);
        })
    };

    // Expand overlay: mode = Visible, map surface, focus input
    let expand_overlay_fn = {
        let window = window.clone();
        let entry = entry.clone();
        let state = state.clone();
        let just_expanded = just_expanded.clone();

        Rc::new(move || {
            let (top, bottom, right) = {
                let mut st = state.borrow_mut();
                if st.mode == OverlayMode::Visible {
                    return;
                }
                st.mode = OverlayMode::Visible;
                (
                    st.settings.margin_top,
                    st.bottom_margin,
                    st.settings.margin_right,
                )
            };
            just_expanded.set(true);

            overlay::expand_overlay(&window, top, bottom, right);
            entry.grab_focus();

            let je = just_expanded.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
                je.set(false);
            });
        })
    };

    // Toggle between Visible and Hidden
    let toggle = {
        let expand_fn = expand_overlay_fn.clone();
        let hide_fn = hide_overlay_fn.clone();
        let state = state.clone();

        Rc::new(move || {
            let mode = state.borrow().mode;
            match mode {
                OverlayMode::Visible => hide_fn(),
                OverlayMode::Hidden => expand_fn(),
            }
        })
    };

    // Close button hides overlay
    let on_close_click = hide_overlay_fn.clone();
    btn_close.connect_clicked(move |_| {
        on_close_click();
    });

    // Clear button
    let on_clear_click = clear_conversation.clone();
    btn_clear.connect_clicked(move |_| {
        on_clear_click();
    });

    // Auto-hide on focus loss (Click Outside)
    {
        let hide_on_blur = hide_overlay_fn.clone();
        let state_blur = state.clone();
        let just_expanded_blur = just_expanded.clone();
        let prompt_pop_blur = prompt_popover.clone();
        let settings_pop_blur = settings_popover.clone();

        let is_hiding = Rc::new(Cell::new(false));
        let is_hiding_clone = is_hiding.clone();

        window.connect_is_active_notify(move |win| {
            if is_hiding_clone.get() {
                return;
            }
            let active = win.is_active();
            if !active {
                if just_expanded_blur.get() {
                    return;
                }
                if prompt_pop_blur.is_visible() || settings_pop_blur.is_visible() {
                    return;
                }
                let (is_visible, auto_hide) = {
                    if let Ok(st) = state_blur.try_borrow() {
                        (
                            st.mode == OverlayMode::Visible,
                            st.settings.auto_hide_on_blur,
                        )
                    } else {
                        (false, false)
                    }
                };
                if is_visible && auto_hide {
                    is_hiding_clone.set(true);
                    hide_on_blur();
                    let reset_hiding = is_hiding_clone.clone();
                    glib::idle_add_local_once(move || {
                        reset_hiding.set(false);
                    });
                }
            }
        });

        let hide_on_pop_close = hide_overlay_fn.clone();
        let state_pop_close = state.clone();
        let win_pop_close = window.clone();
        let je_pop_close = just_expanded.clone();

        let check_focus_after_popover = Rc::new(move || {
            let win = win_pop_close.clone();
            let hide = hide_on_pop_close.clone();
            let st = state_pop_close.clone();
            let je = je_pop_close.clone();

            glib::idle_add_local_once(move || {
                let (is_visible, auto_hide) = {
                    let s = st.borrow();
                    (s.mode == OverlayMode::Visible, s.settings.auto_hide_on_blur)
                };
                if !win.is_active() && !je.get() && is_visible && auto_hide {
                    hide();
                }
            });
        });

        let p1 = check_focus_after_popover.clone();
        prompt_popover.connect_closed(move |_| {
            p1();
        });

        let p2 = check_focus_after_popover.clone();
        settings_popover.connect_closed(move |_| {
            p2();
        });
    }

    // Setup Taskbar System Tray Applet (StatusNotifierItem)
    {
        let toggle_tray = toggle.clone();
        tray::setup_tray(move || {
            toggle_tray();
        });
    }

    // Handle Keyboard Shortcuts on Window
    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let toggle_on_hotkey = toggle.clone();
    let hide_on_esc = hide_overlay_fn.clone();
    let clear_on_ctrl_l = clear_conversation.clone();
    let prompt_pop_esc = prompt_popover.clone();
    let settings_pop_esc = settings_popover.clone();

    key_controller.connect_key_pressed(move |_, keyval, _, state_modifier| {
        let is_ctrl = state_modifier.contains(gdk::ModifierType::CONTROL_MASK);
        let is_alt = state_modifier.contains(gdk::ModifierType::ALT_MASK);

        // Shortcut: Ctrl + Alt + L to toggle
        if (keyval == gdk::Key::l || keyval == gdk::Key::L) && is_ctrl && is_alt {
            toggle_on_hotkey();
            glib::Propagation::Stop
        } else if keyval == gdk::Key::Escape {
            if prompt_pop_esc.is_visible() {
                prompt_pop_esc.popdown();
                return glib::Propagation::Stop;
            }
            if settings_pop_esc.is_visible() {
                settings_pop_esc.popdown();
                return glib::Propagation::Stop;
            }
            hide_on_esc();
            glib::Propagation::Stop
        } else if (keyval == gdk::Key::l || keyval == gdk::Key::L) && is_ctrl && !is_alt {
            // Ctrl + L to clear
            clear_on_ctrl_l();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key_controller);

    // Channel for streamed output from Pi
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<(u64, StreamEvent)>();
    let status_clone = status_label.clone();
    let state_clone = state.clone();
    let scroll_fn = scroll_to_bottom.clone();
    let active_stream_clone = active_stream.clone();

    glib::spawn_future_local(async move {
        while let Some((gen, event)) = stream_rx.recv().await {
            let st = state_clone.borrow();
            if gen != st.runner.generation() {
                continue;
            }

            match event {
                StreamEvent::TextDelta(delta) => {
                    if let Some(ref target) = *active_stream_clone.borrow() {
                        let mut full = target.full_text.borrow_mut();
                        full.push_str(&delta);
                        format::apply_assistant_markup(&target.label, &full);
                    }
                    scroll_fn();
                }
                StreamEvent::ThinkingDelta(delta) => {
                    if let Some(ref target) = *active_stream_clone.borrow() {
                        target.parent_expander.set_visible(true);
                        target.steps_expander.set_visible(true);

                        // If there is no active thought sub-expander, create one
                        let mut cur_thought = target.active_thought_expander.borrow_mut();
                        if cur_thought.is_none() {
                            let item_expander = gtk4::Expander::builder()
                                .label("Thinking...")
                                .css_classes(["timeline-sub-expander", "thought-sub-item"])
                                .expanded(true)
                                .build();
                            let item_label = gtk4::Label::builder()
                                .selectable(true)
                                .wrap(true)
                                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                                .width_chars(10)
                                .max_width_chars(0)
                                .xalign(0.0)
                                .css_classes(["timeline-item-text", "thought-text"])
                                .build();
                            item_expander.set_child(Some(&item_label));
                            target.timeline_box.append(&item_expander);
                            *cur_thought = Some((
                                item_expander,
                                item_label,
                                Rc::new(RefCell::new(String::new())),
                            ));
                        }

                        if let Some((ref exp, ref lbl, ref txt)) = *cur_thought {
                            let mut t = txt.borrow_mut();
                            t.push_str(&delta);
                            lbl.set_text(&t);
                            let elapsed = target.start_time.elapsed().as_secs();
                            exp.set_label(Some(&format!("Thought for {}s ›", elapsed.max(1))));
                        }
                    }
                    scroll_fn();
                }
                StreamEvent::ToolCall { name, args } => {
                    if let Some(ref target) = *active_stream_clone.borrow() {
                        target.parent_expander.set_visible(true);
                        target.steps_expander.set_visible(true);

                        // Close active thinking block on tool execution
                        if let Some((ref exp, _, _)) = *target.active_thought_expander.borrow() {
                            exp.set_expanded(false);
                        }
                        *target.active_thought_expander.borrow_mut() = None;

                        target.tool_count.set(target.tool_count.get() + 1);
                        let steps_lbl = format!(
                            "Explored files, ran {} command(s) ▾",
                            target.tool_count.get()
                        );
                        target.steps_expander.set_label(Some(&steps_lbl));

                        let tool_expander = gtk4::Expander::builder()
                            .label(format!("Ran tool: {} ›", name))
                            .css_classes(["timeline-sub-expander", "tool-sub-item"])
                            .expanded(false)
                            .build();

                        let args_label = gtk4::Label::builder()
                            .label(&args)
                            .selectable(true)
                            .wrap(true)
                            .wrap_mode(gtk4::pango::WrapMode::WordChar)
                            .width_chars(10)
                            .max_width_chars(0)
                            .xalign(0.0)
                            .css_classes(["timeline-item-text", "tool-args-text"])
                            .build();
                        tool_expander.set_child(Some(&args_label));
                        target.timeline_box.append(&tool_expander);
                    }
                    scroll_fn();
                }
                StreamEvent::Metrics(m) => {
                    if let Some(ref target) = *active_stream_clone.borrow() {
                        target.metrics_badge.set_visible(true);
                        let sec = (m.duration_ms as f64) / 1000.0;
                        target
                            .metrics_badge
                            .set_text(&format!("{} tok • {:.1}s", m.total_tokens, sec));
                    }
                }
                StreamEvent::Content(line) => {
                    if let Some(ref target) = *active_stream_clone.borrow() {
                        let mut full = target.full_text.borrow_mut();
                        if !full.is_empty() {
                            full.push('\n');
                        }
                        full.push_str(&line);
                        format::apply_assistant_markup(&target.label, &full);
                    }
                    scroll_fn();
                }
                StreamEvent::Error(err_line) => {
                    if let Some(ref target) = *active_stream_clone.borrow() {
                        let mut full = target.full_text.borrow_mut();
                        if !full.is_empty() {
                            full.push('\n');
                        }
                        full.push_str(&format!("[Error] {}", err_line));
                        format::apply_assistant_markup(&target.label, &full);
                    }
                    scroll_fn();
                }
                StreamEvent::Done => {
                    if let Some(ref target) = *active_stream_clone.borrow() {
                        // Collapse thought and tool expanders automatically once done (clean Open-WebUI standard)
                        target.parent_expander.set_expanded(false);
                        target.steps_expander.set_expanded(false);
                    }
                    status_clone.set_text("ready");
                    status_clone.remove_css_class("working");
                    scroll_fn();
                }
            }
        }
    });

    // Prompt submission logic
    let submit_prompt = {
        let entry = entry.clone();
        let messages_box = messages_box.clone();
        let welcome_box = welcome_box.clone();
        let prompt_list_box = prompt_list_box.clone();
        let prompt_empty_placeholder = prompt_empty_placeholder.clone();
        let btn_prompts = btn_prompts.clone();
        let prompt_popover = prompt_popover.clone();
        let status_label = status_label.clone();
        let state = state.clone();
        let active_stream = active_stream.clone();
        let stream_tx = stream_tx.clone();
        let scroll_fn = scroll_to_bottom.clone();
        let scroll_to_widget = scroll_to_widget.clone();

        Rc::new(move || {
            let prompt = entry.text().to_string();
            if prompt.trim().is_empty() {
                return;
            }

            entry.set_text("");
            welcome_box.set_visible(false);

            let prompt_idx = state.borrow().prompts.len() + 1;

            // 1. Create User Message Block (Linear-style flat command block)
            let user_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            user_row.set_halign(gtk4::Align::Fill);
            user_row.add_css_class("bubble-user-row");

            let user_bubble = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
            user_bubble.add_css_class("chat-bubble-user");

            let user_label = gtk4::Label::builder()
                .label(&prompt)
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .width_chars(10)
                .max_width_chars(0)
                .selectable(true)
                .xalign(0.0)
                .css_classes(["bubble-user-text"])
                .build();
            user_bubble.append(&user_label);
            user_row.append(&user_bubble);
            messages_box.append(&user_row);

            // 2. Track in Prompt Navigator
            state.borrow_mut().prompts.push(TrackedPrompt {
                index: prompt_idx,
                text: prompt.clone(),
                widget: user_row.clone(),
                bubble: user_bubble.clone(),
            });

            btn_prompts.set_label(&format!("Prompts ({})", prompt_idx));
            prompt_empty_placeholder.set_visible(false);

            let nav_btn = gtk4::Button::builder()
                .css_classes(["btn-menu-item"])
                .build();
            let nav_btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            let nav_badge = gtk4::Label::builder()
                .label(format!("#{}", prompt_idx))
                .css_classes(["prompt-nav-badge"])
                .build();
            let snippet = if prompt.chars().count() > 24 {
                format!("{}…", prompt.chars().take(24).collect::<String>())
            } else {
                prompt.clone()
            };
            let nav_text = gtk4::Label::builder()
                .label(&snippet)
                .css_classes(["prompt-nav-text"])
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .xalign(0.0)
                .build();
            nav_text.set_hexpand(true);
            nav_btn_box.append(&nav_badge);
            nav_btn_box.append(&nav_text);
            nav_btn.set_child(Some(&nav_btn_box));

            let target_row = user_row.clone();
            let target_bubble = user_bubble.clone();
            let popover_clone = prompt_popover.clone();
            let scroll_fn_widget = scroll_to_widget.clone();

            nav_btn.connect_clicked(move |_| {
                popover_clone.popdown();
                scroll_fn_widget(&target_row);
                target_bubble.add_css_class("bubble-highlight");
                let b_clone = target_bubble.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    b_clone.remove_css_class("bubble-highlight");
                });
            });
            prompt_list_box.append(&nav_btn);

            // 3. Create Assistant Message Block (Clean full-bleed layout)
            let asst_row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            asst_row.set_halign(gtk4::Align::Fill);
            asst_row.add_css_class("bubble-assistant-row");

            let asst_bubble = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            asst_bubble.add_css_class("chat-bubble-assistant");

            let asst_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            asst_header.add_css_class("bubble-assistant-header");

            let asst_badge = gtk4::Label::new(Some("Pi"));
            asst_badge.add_css_class("bubble-assistant-badge");
            asst_header.append(&asst_badge);

            let metrics_badge = gtk4::Label::new(None);
            metrics_badge.add_css_class("bubble-metrics-badge");
            metrics_badge.set_visible(false);
            asst_header.append(&metrics_badge);

            let asst_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            asst_spacer.set_hexpand(true);
            asst_header.append(&asst_spacer);

            let btn_copy = gtk4::Button::builder()
                .label("Copy")
                .css_classes(["btn-bubble-copy"])
                .tooltip_text("Copy response to clipboard")
                .build();
            asst_header.append(&btn_copy);
            asst_bubble.append(&asst_header);

            // Antigravity-Style Nested Timeline Collapsible Box
            let parent_expander = gtk4::Expander::builder()
                .label("Working... ▾")
                .css_classes(["antigravity-parent-expander"])
                .expanded(true)
                .visible(false)
                .build();

            let parent_content_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
            parent_content_box.add_css_class("antigravity-parent-content");

            let steps_expander = gtk4::Expander::builder()
                .label("Explored files, ran commands ▾")
                .css_classes(["antigravity-steps-expander"])
                .expanded(true)
                .visible(false)
                .build();

            let timeline_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
            timeline_box.add_css_class("antigravity-timeline-list");
            steps_expander.set_child(Some(&timeline_box));
            parent_content_box.append(&steps_expander);

            parent_expander.set_child(Some(&parent_content_box));
            asst_bubble.append(&parent_expander);

            let divider = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            divider.add_css_class("bubble-divider");
            asst_bubble.append(&divider);

            let asst_label = gtk4::Label::builder()
                .selectable(true)
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .width_chars(10)
                .max_width_chars(0)
                .xalign(0.0)
                .css_classes(["bubble-assistant-text"])
                .build();
            asst_bubble.append(&asst_label);
            asst_row.append(&asst_bubble);
            messages_box.append(&asst_row);

            let full_text = Rc::new(RefCell::new(String::new()));

            let copy_text_ref = full_text.clone();
            let copy_btn_clone = btn_copy.clone();
            btn_copy.connect_clicked(move |_| {
                let content = copy_text_ref.borrow().clone();
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&content);
                }
                copy_btn_clone.set_label("Copied");
                let reset_btn = copy_btn_clone.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    reset_btn.set_label("Copy");
                });
            });

            *active_stream.borrow_mut() = Some(ActiveStreamTarget {
                label: asst_label.clone(),
                parent_expander: parent_expander.clone(),
                steps_expander: steps_expander.clone(),
                timeline_box: timeline_box.clone(),
                active_thought_expander: Rc::new(RefCell::new(None)),
                start_time: std::time::Instant::now(),
                metrics_badge: metrics_badge.clone(),
                full_text: full_text.clone(),
                tool_count: Rc::new(Cell::new(0)),
            });

            scroll_fn();

            status_label.set_text("thinking...");
            status_label.add_css_class("working");

            let mut st = state.borrow_mut();
            let model_opt = Some(st.settings.model.clone());
            let thinking_opt = Some(st.settings.thinking_level.clone());
            if let Err(e) =
                st.runner
                    .spawn_prompt(prompt, model_opt, thinking_opt, stream_tx.clone())
            {
                let err_str = format!("[Error] Failed to spawn pi: {}", e);
                full_text.borrow_mut().push_str(&err_str);
                format::apply_assistant_markup(&asst_label, &err_str);
                parent_expander.set_expanded(false);
                status_label.set_text("error");
                status_label.remove_css_class("working");
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

    // IPC Socket listener
    let (ipc_tx, mut ipc_rx) = mpsc::unbounded_channel::<()>();
    ipc::start_ipc_listener(ipc_tx);

    let toggle_action = toggle.clone();
    glib::spawn_future_local(async move {
        while let Some(()) = ipc_rx.recv().await {
            toggle_action();
        }
    });

    // Show GUI window immediately on launch
    expand_overlay_fn();

    AppContext { window, toggle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_mode_enum_and_state() {
        let mut state = AppState {
            mode: OverlayMode::Visible,
            runner: PiRunner::new(),
            prompts: Vec::new(),
            settings: AppSettings::default(),
            top_margin: 12,
            bottom_margin: 12,
        };

        assert_eq!(state.mode, OverlayMode::Visible);

        // Transition to Hidden
        state.mode = OverlayMode::Hidden;
        assert_eq!(state.mode, OverlayMode::Hidden);

        // Transition back to Visible
        state.mode = OverlayMode::Visible;
        assert_eq!(state.mode, OverlayMode::Visible);
    }

    #[test]
    fn test_theme_list_integrity() {
        assert_eq!(AVAILABLE_THEMES.len(), 16);
        let ids: Vec<&str> = AVAILABLE_THEMES.iter().map(|(id, _, _)| *id).collect();
        assert!(ids.contains(&"theme-dark-glass"));
        assert!(ids.contains(&"theme-dracula"));
        assert!(ids.contains(&"theme-catppuccin-mocha"));
        assert!(ids.contains(&"theme-catppuccin-macchiato"));
        assert!(ids.contains(&"theme-catppuccin-frappe"));
        assert!(ids.contains(&"theme-catppuccin-latte"));
        assert!(ids.contains(&"theme-oled"));
        assert!(ids.contains(&"theme-slate"));
        assert!(ids.contains(&"theme-forest"));
        assert!(ids.contains(&"theme-zinc-light"));
        assert!(ids.contains(&"theme-tokyo-night"));
        assert!(ids.contains(&"theme-nord"));
        assert!(ids.contains(&"theme-gruvbox-dark"));
        assert!(ids.contains(&"theme-one-dark"));
        assert!(ids.contains(&"theme-solarized-dark"));
        assert!(ids.contains(&"theme-solarized-light"));
    }

    #[test]
    fn test_popular_models_list() {
        assert!(POPULAR_MODELS.len() >= 5);
        let ids: Vec<&str> = POPULAR_MODELS.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&"default"));
        assert!(ids.contains(&"claude-3-7-sonnet"));
        assert!(ids.contains(&"gpt-4o"));
    }
}
