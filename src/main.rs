mod backend;
mod format;
mod ipc;
mod overlay;

use backend::{PiRunner, StreamEvent};
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio::sync::mpsc;

const APP_ID: &str = "com.github.khoipm.pi-overlay";
pub const MIN_DRAWER_WIDTH: i32 = 320;
pub const MAX_DRAWER_WIDTH: i32 = 900;
pub const DEFAULT_DRAWER_WIDTH: i32 = 480;

const THEMES: &[(&str, &str, &str)] = &[
    ("theme-dark-glass", "Dark Glass", "Translucent dark glass"),
    ("theme-oled", "OLED Black", "Pure deep black"),
    ("theme-slate", "Slate Graphite", "Deep slate & graphite"),
    ("theme-forest", "Forest Charcoal", "Obsidian & subtle green"),
    ("theme-zinc-light", "Zinc Light", "Minimal bright zinc"),
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

struct ActiveStreamTarget {
    label: gtk4::Label,
    thinking_badge: gtk4::Label,
    full_text: Rc<RefCell<String>>,
    thinking_text: Rc<RefCell<String>>,
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
    drawer_width: i32,
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

    // Auto-register KDE global shortcut Ctrl+Alt+L if on KDE Plasma
    ensure_kde_global_shortcut();

    // Initialize Tokio runtime for background IPC and subprocesses
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize tokio multi-thread runtime");
    let _guard = rt.enter();

    let app = gtk4::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|_| {
        eprintln!("[pi-overlay] app startup");
        load_css();
    });

    let context_holder: Rc<RefCell<Option<AppContext>>> = Rc::new(RefCell::new(None));
    let ctx_clone = context_holder.clone();

    app.connect_activate(move |app| {
        eprintln!("[pi-overlay] app activate");
        let mut ctx = ctx_clone.borrow_mut();
        if let Some(ref c) = *ctx {
            // Already running: toggle window
            eprintln!("[pi-overlay] already running, calling toggle");
            (c.toggle)();
        } else {
            // First launch: build UI and store context
            eprintln!("[pi-overlay] first launch, building UI");
            let new_ctx = build_ui(app);
            *ctx = Some(new_ctx);
        }
    });

    eprintln!("[pi-overlay] running app.run()");
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

        // Update KDE Sycoca cache so desktop file is indexed
        let _ = std::process::Command::new("kbuildsycoca6").status();

        // Write configuration into kglobalshortcutsrc for persistence across reboots
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

        // Register action with live kglobalacceld session
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

        // Bind foreign shortcut Ctrl+Alt+L (Qt key code: 201326668) in live kglobalacceld
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
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("Pi")
        .default_width(DEFAULT_DRAWER_WIDTH)
        .build();

    overlay::init_layer_shell(&window);

    // Root overlay container: Drag resize handle + drawer directly in root_box
    let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root_box.set_halign(gtk4::Align::End);
    root_box.set_vexpand(true);

    // Interactive Drag Handle on the left edge (like Messenger Chat Head resizing)
    let resize_handle = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    resize_handle.add_css_class("resize-handle");
    resize_handle.set_cursor_from_name(Some("ew-resize"));
    resize_handle.set_width_request(10);
    resize_handle.set_vexpand(true);

    // Visual grip bar inside handle
    let resize_grip = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    resize_grip.add_css_class("resize-grip");
    resize_grip.set_valign(gtk4::Align::Center);
    resize_grip.set_vexpand(true);
    resize_handle.append(&resize_grip);

    root_box.append(&resize_handle);

    let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    drawer.set_widget_name("drawer-container");
    drawer.add_css_class("theme-dark-glass");
    drawer.set_width_request(DEFAULT_DRAWER_WIDTH);
    drawer.set_vexpand(true);

    // Header Area: Title + Status + Action Buttons (No decorative emojis)
    let header_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header_box.add_css_class("header-box");

    let title_label = gtk4::Label::new(Some("Pi"));
    title_label.add_css_class("title-label");
    title_label.set_halign(gtk4::Align::Start);
    header_box.append(&title_label);

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

    // Theme Switcher Button & Popover
    let btn_theme = gtk4::MenuButton::builder()
        .label("Theme")
        .css_classes(["btn-header"])
        .tooltip_text("Switch visual theme")
        .build();

    let theme_popover = gtk4::Popover::new();
    theme_popover.add_css_class("theme-popover");

    let theme_pop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let theme_title = gtk4::Label::builder()
        .label("Choose Theme")
        .css_classes(["popover-header-title"])
        .xalign(0.0)
        .build();
    theme_pop_box.append(&theme_title);

    let theme_list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    theme_list_box.set_width_request(180);

    let theme_buttons: Rc<RefCell<Vec<(&'static str, gtk4::Button)>>> = Rc::new(RefCell::new(Vec::new()));

    for &(theme_id, theme_name, _) in THEMES {
        let t_btn = gtk4::Button::builder()
            .label(theme_name)
            .css_classes(["btn-menu-item"])
            .build();
        if theme_id == "theme-dark-glass" {
            t_btn.add_css_class("active");
        }

        let drawer_for_theme = drawer.clone();
        let popover_for_theme = theme_popover.clone();
        let buttons_list = theme_buttons.clone();

        t_btn.connect_clicked(move |_| {
            for &(other_id, _, _) in THEMES {
                drawer_for_theme.remove_css_class(other_id);
            }
            drawer_for_theme.add_css_class(theme_id);

            for (id, btn) in buttons_list.borrow().iter() {
                if *id == theme_id {
                    btn.add_css_class("active");
                } else {
                    btn.remove_css_class("active");
                }
            }
            popover_for_theme.popdown();
        });

        theme_buttons.borrow_mut().push((theme_id, t_btn.clone()));
        theme_list_box.append(&t_btn);
    }
    theme_pop_box.append(&theme_list_box);
    theme_popover.set_child(Some(&theme_pop_box));
    btn_theme.set_popover(Some(&theme_popover));
    header_box.append(&btn_theme);

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

    // Welcome placeholder: minimal refined hints, no emojis
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
    let shortcut_hint = gtk4::Label::new(Some("Drag edge to resize  •  Ctrl+Alt+L: Toggle  •  Esc: Close"));
    shortcut_hint.add_css_class("shortcut-label");
    shortcut_hint.set_halign(gtk4::Align::Center);
    shortcut_hint.set_hexpand(true);
    footer_box.append(&shortcut_hint);
    drawer.append(&footer_box);

    root_box.append(&drawer);
    window.set_child(Some(&root_box));

    let state = Rc::new(RefCell::new(AppState {
        mode: OverlayMode::Hidden,
        runner: PiRunner::new(),
        prompts: Vec::new(),
        drawer_width: DEFAULT_DRAWER_WIDTH,
    }));

    // Setup GestureDrag on resize handle (Dragging left increases width, dragging right decreases)
    let drag_gesture = gtk4::GestureDrag::new();
    let initial_width = Rc::new(Cell::new(DEFAULT_DRAWER_WIDTH));
    let start_w = initial_width.clone();
    let state_drag = state.clone();

    drag_gesture.connect_drag_begin(move |_, _, _| {
        start_w.set(state_drag.borrow().drawer_width);
    });

    let drawer_drag = drawer.clone();
    let window_drag = window.clone();
    let cur_w = initial_width.clone();
    let state_update = state.clone();

    drag_gesture.connect_drag_update(move |_, offset_x, _| {
        // Dragging handle towards left (negative offset_x) widens drawer
        let target_width = (cur_w.get() - offset_x as i32)
            .clamp(MIN_DRAWER_WIDTH, MAX_DRAWER_WIDTH);

        state_update.borrow_mut().drawer_width = target_width;
        drawer_drag.set_width_request(target_width);
        window_drag.set_default_size(target_width + 10, -1);
        window_drag.queue_resize();
    });

    resize_handle.add_controller(drag_gesture);

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

        Rc::new(move || {
            {
                let mut st = state.borrow_mut();
                if st.mode == OverlayMode::Hidden {
                    return;
                }
                st.mode = OverlayMode::Hidden;
            }
            // Call overlay::hide_overlay AFTER dropping the borrow_mut
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
            {
                let mut st = state.borrow_mut();
                if st.mode == OverlayMode::Visible {
                    return;
                }
                st.mode = OverlayMode::Visible;
            }
            just_expanded.set(true);

            overlay::expand_overlay(&window);
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
        let theme_pop_blur = theme_popover.clone();

        window.connect_is_active_notify(move |win| {
            let active = win.is_active();
            if !active {
                if just_expanded_blur.get() {
                    return;
                }
                if prompt_pop_blur.is_visible() || theme_pop_blur.is_visible() {
                    return;
                }
                let is_visible = state_blur.borrow().mode == OverlayMode::Visible;
                if is_visible {
                    hide_on_blur();
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
                if !win.is_active() && !je.get() && st.borrow().mode == OverlayMode::Visible {
                    hide();
                }
            });
        });

        let p1 = check_focus_after_popover.clone();
        prompt_popover.connect_closed(move |_| {
            p1();
        });

        let p2 = check_focus_after_popover.clone();
        theme_popover.connect_closed(move |_| {
            p2();
        });
    }

    // Handle Keyboard Shortcuts on Window
    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let toggle_on_hotkey = toggle.clone();
    let hide_on_esc = hide_overlay_fn.clone();
    let clear_on_ctrl_l = clear_conversation.clone();
    let prompt_pop_esc = prompt_popover.clone();
    let theme_pop_esc = theme_popover.clone();

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
            if theme_pop_esc.is_visible() {
                theme_pop_esc.popdown();
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
                        target.thinking_badge.set_text("");
                        let mut full = target.full_text.borrow_mut();
                        full.push_str(&delta);
                        format::apply_assistant_markup(&target.label, &full);
                    }
                    scroll_fn();
                }
                StreamEvent::ThinkingDelta(delta) => {
                    if let Some(ref target) = *active_stream_clone.borrow() {
                        let mut think = target.thinking_text.borrow_mut();
                        think.push_str(&delta);
                        target.thinking_badge.set_text("thinking...");
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
                        target.thinking_badge.set_text("");
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
                .label(&format!("#{}", prompt_idx))
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

            let thinking_badge = gtk4::Label::new(Some("thinking..."));
            thinking_badge.add_css_class("bubble-thinking-badge");
            asst_header.append(&thinking_badge);

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

            let divider = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            divider.add_css_class("bubble-divider");
            asst_bubble.append(&divider);

            let asst_label = gtk4::Label::builder()
                .selectable(true)
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .xalign(0.0)
                .css_classes(["bubble-assistant-text"])
                .build();
            asst_bubble.append(&asst_label);
            asst_row.append(&asst_bubble);
            messages_box.append(&asst_row);

            let full_text = Rc::new(RefCell::new(String::new()));
            let thinking_text = Rc::new(RefCell::new(String::new()));
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
                thinking_badge: thinking_badge.clone(),
                full_text: full_text.clone(),
                thinking_text: thinking_text.clone(),
            });

            scroll_fn();

            status_label.set_text("thinking...");
            status_label.add_css_class("working");

            let mut st = state.borrow_mut();
            if let Err(e) = st.runner.spawn_prompt(prompt, stream_tx.clone()) {
                let err_str = format!("[Error] Failed to spawn pi: {}", e);
                full_text.borrow_mut().push_str(&err_str);
                format::apply_assistant_markup(&asst_label, &err_str);
                thinking_badge.set_text("");
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
    eprintln!("[pi-overlay] calling expand_overlay_fn at startup");
    expand_overlay_fn();

    AppContext {
        window,
        toggle,
    }
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
            drawer_width: DEFAULT_DRAWER_WIDTH,
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
        assert_eq!(THEMES.len(), 5);
        let ids: Vec<&str> = THEMES.iter().map(|(id, _, _)| *id).collect();
        assert!(ids.contains(&"theme-dark-glass"));
        assert!(ids.contains(&"theme-oled"));
        assert!(ids.contains(&"theme-slate"));
        assert!(ids.contains(&"theme-forest"));
        assert!(ids.contains(&"theme-zinc-light"));
    }

    #[test]
    fn test_drawer_width_clamps() {
        assert!(MIN_DRAWER_WIDTH >= 300);
        assert!(MAX_DRAWER_WIDTH >= MIN_DRAWER_WIDTH);
        assert!(DEFAULT_DRAWER_WIDTH >= MIN_DRAWER_WIDTH && DEFAULT_DRAWER_WIDTH <= MAX_DRAWER_WIDTH);
    }
}
