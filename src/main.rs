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

const THEMES: &[(&str, &str, &str)] = &[
    ("theme-acrylic", "🌌 Frosted Acrylic", "Translucent dark glass"),
    ("theme-oled", "⬛ OLED Midnight", "Pure deep black"),
    ("theme-ocean", "🌊 Deep Ocean", "Deep navy & cyan"),
    ("theme-emerald", "🌲 Cyber Emerald", "Obsidian & emerald"),
    ("theme-light", "☀️ Frosted Light", "Bright translucent glass"),
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
}

struct AppState {
    visible: bool,
    runner: PiRunner,
    prompts: Vec<TrackedPrompt>,
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
        load_css();
    });

    let context_holder: Rc<RefCell<Option<AppContext>>> = Rc::new(RefCell::new(None));
    let ctx_clone = context_holder.clone();

    app.connect_activate(move |app| {
        let mut ctx = ctx_clone.borrow_mut();
        if let Some(ref c) = *ctx {
            // Already running: toggle window
            (c.toggle)();
        } else {
            // First launch: build UI and store context
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
        .default_width(500)
        .build();

    overlay::init_layer_shell(&window);

    // Root overlay container
    let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root_box.set_hexpand(true);
    root_box.set_vexpand(true);
    root_box.set_halign(gtk4::Align::End);

    let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    drawer.set_widget_name("drawer-container");
    drawer.add_css_class("theme-acrylic");
    drawer.set_width_request(500);
    drawer.set_vexpand(true);

    // Header Area: Title + Status + Action Buttons
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

    // ChatGPT-Style Prompt Navigator Button & Popover
    let btn_prompts = gtk4::MenuButton::builder()
        .label("💬 Prompts (0)")
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
    prompt_list_box.set_width_request(260);

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
        .label("🎨 Theme")
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
    theme_list_box.set_width_request(190);

    let theme_buttons: Rc<RefCell<Vec<(&'static str, gtk4::Button)>>> = Rc::new(RefCell::new(Vec::new()));

    for &(theme_id, theme_name, _) in THEMES {
        let t_btn = gtk4::Button::builder()
            .label(theme_name)
            .css_classes(["btn-menu-item"])
            .build();
        if theme_id == "theme-acrylic" {
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

    // Close Button
    let btn_close = gtk4::Button::builder()
        .label("✕")
        .css_classes(["btn-header"])
        .tooltip_text("Close (Esc)")
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

    let messages_box = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    messages_box.add_css_class("messages-container");
    messages_box.set_vexpand(true);
    messages_box.set_valign(gtk4::Align::Start);

    // Welcome placeholder
    let welcome_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    welcome_box.add_css_class("welcome-box");
    welcome_box.set_halign(gtk4::Align::Center);
    welcome_box.set_valign(gtk4::Align::Center);
    welcome_box.set_vexpand(true);

    let welcome_icon = gtk4::Label::builder()
        .label("✨")
        .css_classes(["welcome-icon"])
        .xalign(0.5)
        .build();
    welcome_box.append(&welcome_icon);

    let welcome_title = gtk4::Label::builder()
        .label("How can I help you today?")
        .css_classes(["welcome-title"])
        .xalign(0.5)
        .build();
    welcome_box.append(&welcome_title);

    let welcome_sub = gtk4::Label::builder()
        .label("Type your query below • Ctrl+Alt+L to toggle overlay")
        .css_classes(["welcome-subtitle"])
        .xalign(0.5)
        .build();
    welcome_box.append(&welcome_sub);

    messages_box.append(&welcome_box);
    scrolled.set_child(Some(&messages_box));
    drawer.append(&scrolled);

    // Input Area: Entry + Send button
    let input_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
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

    // Footer with keyboard shortcut hints
    let footer_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    footer_box.add_css_class("footer-box");
    let shortcut_hint = gtk4::Label::new(Some("Ctrl+Alt+L: Toggle  •  Esc: Close  •  Ctrl+L: Clear"));
    shortcut_hint.add_css_class("shortcut-label");
    shortcut_hint.set_halign(gtk4::Align::Center);
    shortcut_hint.set_hexpand(true);
    footer_box.append(&shortcut_hint);
    drawer.append(&footer_box);

    root_box.append(&drawer);
    window.set_child(Some(&root_box));

    let state = Rc::new(RefCell::new(AppState {
        visible: true,
        runner: PiRunner::new(),
        prompts: Vec::new(),
    }));

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
            btn_prompts.set_label("💬 Prompts (0)");

            status_label.set_text("ready");
            status_label.remove_css_class("working");
            state.borrow_mut().runner.abort();
            *active_stream.borrow_mut() = None;
            autoscroll.set(true);
            entry.set_text("");
            entry.grab_focus();
        })
    };

    // Hide overlay window (preserves conversation transcript)
    let hide_overlay = {
        let window = window.clone();
        let state = state.clone();

        Rc::new(move || {
            let mut st = state.borrow_mut();
            if !st.visible {
                return;
            }
            st.visible = false;
            overlay::hide_overlay(&window);
        })
    };

    // Show drawer and focus input
    let show_drawer = {
        let window = window.clone();
        let entry = entry.clone();
        let state = state.clone();

        Rc::new(move || {
            let mut st = state.borrow_mut();
            if st.visible {
                return;
            }
            st.visible = true;

            overlay::show_overlay(&window);
            entry.grab_focus();
        })
    };

    // Toggle drawer
    let toggle = {
        let show_drawer = show_drawer.clone();
        let hide_overlay = hide_overlay.clone();
        let state = state.clone();

        Rc::new(move || {
            let is_vis = state.borrow().visible;
            if is_vis {
                hide_overlay();
            } else {
                show_drawer();
            }
        })
    };

    // Connect Close button
    let on_close_click = hide_overlay.clone();
    btn_close.connect_clicked(move |_| {
        on_close_click();
    });

    // Connect Clear button
    let on_clear_click = clear_conversation.clone();
    btn_clear.connect_clicked(move |_| {
        on_clear_click();
    });

    // Handle Keyboard Shortcuts on Window
    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let toggle_on_hotkey = toggle.clone();
    let hide_on_esc = hide_overlay.clone();
    let clear_on_ctrl_l = clear_conversation.clone();

    key_controller.connect_key_pressed(move |_, keyval, _, state_modifier| {
        let is_ctrl = state_modifier.contains(gdk::ModifierType::CONTROL_MASK);
        let is_alt = state_modifier.contains(gdk::ModifierType::ALT_MASK);

        // Shortcut: Ctrl + Alt + L to toggle
        if (keyval == gdk::Key::l || keyval == gdk::Key::L) && is_ctrl && is_alt {
            toggle_on_hotkey();
            glib::Propagation::Stop
        } else if keyval == gdk::Key::Escape {
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

            // 1. Create User Message Bubble (Messenger / iMessage pill)
            let user_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            user_row.set_halign(gtk4::Align::End);
            user_row.add_css_class("bubble-user-row");

            let user_bubble = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
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

            btn_prompts.set_label(&format!("💬 Prompts ({})", prompt_idx));
            prompt_empty_placeholder.set_visible(false);

            let nav_btn = gtk4::Button::builder()
                .css_classes(["btn-menu-item"])
                .build();
            let nav_btn_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let nav_badge = gtk4::Label::builder()
                .label(&format!("#{}", prompt_idx))
                .css_classes(["prompt-nav-badge"])
                .build();
            let snippet = if prompt.chars().count() > 28 {
                format!("{}…", prompt.chars().take(28).collect::<String>())
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

            // 3. Create Assistant Message Bubble
            let asst_row = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
            asst_row.set_halign(gtk4::Align::Fill);
            asst_row.add_css_class("bubble-assistant-row");

            let asst_bubble = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
            asst_bubble.add_css_class("chat-bubble-assistant");

            let asst_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            asst_header.add_css_class("bubble-assistant-header");

            let asst_badge = gtk4::Label::new(Some("⚡ Pi"));
            asst_badge.add_css_class("bubble-assistant-badge");
            asst_header.append(&asst_badge);

            let thinking_badge = gtk4::Label::new(Some("thinking..."));
            thinking_badge.add_css_class("bubble-thinking-badge");
            asst_header.append(&thinking_badge);

            let asst_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            asst_spacer.set_hexpand(true);
            asst_header.append(&asst_spacer);

            let btn_copy = gtk4::Button::builder()
                .label("📋 Copy")
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
            let copy_text_ref = full_text.clone();
            let copy_btn_clone = btn_copy.clone();
            btn_copy.connect_clicked(move |_| {
                let content = copy_text_ref.borrow().clone();
                if let Some(display) = gdk::Display::default() {
                    display.clipboard().set_text(&content);
                }
                copy_btn_clone.set_label("✓ Copied");
                let reset_btn = copy_btn_clone.clone();
                glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                    reset_btn.set_label("📋 Copy");
                });
            });

            *active_stream.borrow_mut() = Some(ActiveStreamTarget {
                label: asst_label.clone(),
                thinking_badge: thinking_badge.clone(),
                full_text: full_text.clone(),
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
    window.present();
    entry.grab_focus();

    AppContext {
        window,
        toggle,
    }
}
