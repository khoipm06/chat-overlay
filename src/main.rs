//! `pi-overlay` binary entrypoint, CLI dispatch, and stylesheet setup.

pub mod acp;
#[cfg(feature = "audit")]
mod audit;
mod backend;
pub mod capture;
mod commands;
mod desktop;
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
mod ui;

use gtk4::gdk;
use gtk4::glib::{self, LogWriterOutput};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

const APP_ID: &str = "com.github.khoipm.pi-overlay";

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();

    let current_path = std::env::var("PATH").unwrap_or_default();
    let augmented = if let Ok(home) = std::env::var("HOME") {
        format!(
            "{}/.local/share/mise/installs/node/lts/bin:{}/.local/share/mise/shims:{}/.bun/bin:{}/.opencode/bin:{}/.local/bin:{}/.cargo/bin:/home/linuxbrew/.linuxbrew/bin:/usr/local/bin:{}",
            home, home, home, home, home, home, current_path
        )
    } else {
        format!(
            "/home/linuxbrew/.linuxbrew/bin:/usr/local/bin:{}",
            current_path
        )
    };
    std::env::set_var("PATH", augmented);

    if args.len() > 1 && !args[1].starts_with('-') {
        let prompt_text = args[1..].join(" ");
        if ipc::try_send_command(&format!("PROMPT:{prompt_text}")) {
            return glib::ExitCode::SUCCESS;
        }
    } else if let Some(pos) = args.iter().position(|a| a == "--prompt" || a == "-p") {
        if pos + 1 < args.len() {
            let prompt_text = args[pos + 1..].join(" ");
            if ipc::try_send_command(&format!("PROMPT:{prompt_text}")) {
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

    #[cfg(feature = "audit")]
    let audit_dir: Option<String> = args
        .iter()
        .position(|a| a == "--audit")
        .and_then(|i| args.get(i + 1))
        .cloned();
    #[cfg(not(feature = "audit"))]
    let audit_dir: Option<String> = None;

    let is_daemon =
        args.contains(&"--daemon".to_string()) || args.contains(&"--hidden".to_string());

    if audit_dir.is_none() {
        if is_daemon {
            if ipc::is_daemon_running() {
                return glib::ExitCode::SUCCESS;
            }
        } else if ipc::try_send_toggle() {
            return glib::ExitCode::SUCCESS;
        }
    }

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

    desktop::ensure_desktop_integration();

    let is_window_mode = args.contains(&"--window".to_string()) || audit_dir.is_some();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to initialize Tokio runtime");
    let _rt_guard = rt.enter();

    let app = gtk4::Application::builder()
        .application_id(APP_ID)
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    let app_ctx: Rc<RefCell<Option<ui::state::AppContext>>> = Rc::new(RefCell::new(None));
    let app_ctx_clone = app_ctx.clone();
    app.connect_activate(move |application| {
        if app_ctx_clone.borrow().is_none() {
            let scale_provider = load_css();
            let ctx = ui::build_ui(
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
