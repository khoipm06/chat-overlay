mod backend;
mod ipc;
mod overlay;

use backend::{PiRunner, StreamEvent};
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;

const APP_ID: &str = "com.github.khoipm.pi-overlay";

struct AppState {
    visible: bool,
    runner: PiRunner,
}

struct AppContext {
    #[allow(dead_code)]
    window: gtk4::ApplicationWindow,
    toggle: Rc<dyn Fn()>,
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();

    // If invoked with --toggle: send IPC signal and exit immediately
    if args.contains(&"--toggle".to_string()) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        if let Err(e) = rt.block_on(ipc::send_toggle()) {
            eprintln!("Failed to toggle overlay: {}", e);
            std::process::exit(1);
        }
        return glib::ExitCode::SUCCESS;
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
            "[Desktop Entry]\nName=Pi\nExec={}\nType=Application\nX-KDE-GlobalAccel-CommandShortcut=true\nNoDisplay=true\n",
            exe_str
        );
        let _ = std::fs::write(&desktop_file, content);

        let check = std::process::Command::new("kreadconfig6")
            .args([
                "--file",
                "kglobalshortcutsrc",
                "--group",
                "services",
                "--group",
                "pi-overlay.desktop",
                "--key",
                "_launch",
            ])
            .output();

        let needs_write = match check {
            Ok(out) => !String::from_utf8_lossy(&out.stdout).contains("Ctrl+Alt+L"),
            Err(_) => true,
        };

        if needs_write {
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
                    "org.kde.KWin",
                    "/KWin",
                    "org.kde.KWin",
                    "reconfigure",
                ])
                .status();
        }
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
        .default_width(480)
        .build();

    overlay::init_layer_shell(&window);

    // Root overlay container
    let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root_box.set_hexpand(true);
    root_box.set_vexpand(true);
    root_box.set_halign(gtk4::Align::End);

    let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    drawer.set_widget_name("drawer-container");
    drawer.set_width_request(480);
    drawer.set_vexpand(true);

    // Header Area: Title + Status + Action Buttons
    let header_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    header_box.add_css_class("header-box");

    let title_label = gtk4::Label::new(Some("Pi"));
    title_label.add_css_class("title-label");
    title_label.set_halign(gtk4::Align::Start);
    header_box.append(&title_label);

    let status_label = gtk4::Label::new(Some("ready"));
    status_label.add_css_class("status-badge");
    status_label.set_halign(gtk4::Align::Start);
    status_label.set_hexpand(true);
    header_box.append(&status_label);

    let btn_clear = gtk4::Button::builder()
        .label("Clear")
        .css_classes(["btn-header"])
        .tooltip_text("Clear conversation (Ctrl+L)")
        .build();
    header_box.append(&btn_clear);

    let btn_close = gtk4::Button::builder()
        .label("✕")
        .css_classes(["btn-header"])
        .tooltip_text("Close (Esc)")
        .build();
    header_box.append(&btn_close);

    drawer.append(&header_box);

    // Chat transcript container
    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .css_classes(["chat-scroll"])
        .build();

    let text_view = gtk4::TextView::builder()
        .editable(false)
        .wrap_mode(gtk4::WrapMode::WordChar)
        .cursor_visible(false)
        .css_classes(["chat-view"])
        .build();
    let text_buffer = text_view.buffer();

    let tag_table = text_buffer.tag_table();

    let user_tag = gtk4::TextTag::builder()
        .name("user_prompt")
        .weight(600)
        .foreground("#ffffff")
        .build();
    tag_table.add(&user_tag);

    let err_tag = gtk4::TextTag::builder()
        .name("error_msg")
        .foreground("#f87171")
        .build();
    tag_table.add(&err_tag);

    scrolled.set_child(Some(&text_view));
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
    }));

    let scroll_to_bottom = {
        let text_view = text_view.clone();
        let text_buffer = text_buffer.clone();
        Rc::new(move || {
            let iter = text_buffer.end_iter();
            let mark = text_buffer.create_mark(None, &iter, false);
            text_view.scroll_to_mark(&mark, 0.0, false, 0.0, 1.0);
        })
    };

    // Hide and reset ephemeral state
    let hide_and_clean = {
        let window = window.clone();
        let entry = entry.clone();
        let text_buffer = text_buffer.clone();
        let status_label = status_label.clone();
        let state = state.clone();

        Rc::new(move || {
            let mut st = state.borrow_mut();
            if !st.visible {
                return;
            }
            st.visible = false;

            overlay::hide_overlay(&window);
            st.runner.abort();

            text_buffer.set_text("");
            entry.set_text("");
            status_label.set_text("ready");
            status_label.remove_css_class("working");
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
        let hide_and_clean = hide_and_clean.clone();
        let state = state.clone();

        Rc::new(move || {
            let is_vis = state.borrow().visible;
            if is_vis {
                hide_and_clean();
            } else {
                show_drawer();
            }
        })
    };

    // Connect Close button
    let on_close_click = hide_and_clean.clone();
    btn_close.connect_clicked(move |_| {
        on_close_click();
    });

    // Connect Clear button
    let buffer_for_clear_btn = text_buffer.clone();
    let status_for_clear_btn = status_label.clone();
    let state_for_clear_btn = state.clone();
    btn_clear.connect_clicked(move |_| {
        buffer_for_clear_btn.set_text("");
        status_for_clear_btn.set_text("ready");
        status_for_clear_btn.remove_css_class("working");
        state_for_clear_btn.borrow_mut().runner.abort();
    });

    // Handle Keyboard Shortcuts on Window
    let key_controller = gtk4::EventControllerKey::new();
    let toggle_on_hotkey = toggle.clone();
    let hide_on_esc = hide_and_clean.clone();
    let buffer_for_ctrl_l = text_buffer.clone();
    let status_for_ctrl_l = status_label.clone();
    let state_for_ctrl_l = state.clone();

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
            buffer_for_ctrl_l.set_text("");
            status_for_ctrl_l.set_text("ready");
            status_for_ctrl_l.remove_css_class("working");
            state_for_ctrl_l.borrow_mut().runner.abort();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(key_controller);

    // Channel for streamed output from Pi
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel::<(u64, StreamEvent)>();
    let buffer_clone = text_buffer.clone();
    let status_clone = status_label.clone();
    let state_clone = state.clone();
    let scroll_fn = scroll_to_bottom.clone();

    glib::spawn_future_local(async move {
        while let Some((gen, event)) = stream_rx.recv().await {
            let st = state_clone.borrow();
            if !st.visible || gen != st.runner.generation() {
                continue;
            }

            match event {
                StreamEvent::Content(line) => {
                    let mut iter = buffer_clone.end_iter();
                    buffer_clone.insert(&mut iter, &format!("{}\n", line));
                    scroll_fn();
                }
                StreamEvent::Error(err_line) => {
                    let mut iter = buffer_clone.end_iter();
                    buffer_clone.insert_with_tags_by_name(
                        &mut iter,
                        &format!("[Error] {}\n", err_line),
                        &["error_msg"],
                    );
                    scroll_fn();
                }
                StreamEvent::Done => {
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
        let text_buffer = text_buffer.clone();
        let status_label = status_label.clone();
        let state = state.clone();
        let stream_tx = stream_tx.clone();
        let scroll_fn = scroll_to_bottom.clone();

        Rc::new(move || {
            let prompt = entry.text().to_string();
            if prompt.trim().is_empty() {
                return;
            }

            entry.set_text("");

            let mut iter = text_buffer.end_iter();
            text_buffer.insert_with_tags_by_name(
                &mut iter,
                &format!("❯ {}\n\n", prompt),
                &["user_prompt"],
            );
            scroll_fn();

            status_label.set_text("thinking...");
            status_label.add_css_class("working");

            let mut st = state.borrow_mut();
            if let Err(e) = st.runner.spawn_prompt(prompt, stream_tx.clone()) {
                let mut iter = text_buffer.end_iter();
                text_buffer.insert_with_tags_by_name(
                    &mut iter,
                    &format!("[Error] Failed to spawn pi: {}\n\n", e),
                    &["error_msg"],
                );
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
