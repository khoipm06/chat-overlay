//! Synthetic UI audit walkthrough generating screenshots without calling external CLIs.

use super::mentions_popup::MentionPopup;
use super::state::{ActiveStreamTarget, AppState};
use super::widgets::{add_context_chip, set_text_view_content};
use crate::audit;
use crate::backend::{self, StreamEvent};
use crate::icons;
use crate::menu::{FloatingMenuHost, MenuPlacement};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio::sync::mpsc;

pub struct AuditArgs {
    pub app: gtk4::Application,
    pub window: gtk4::ApplicationWindow,
    pub dir: String,
    pub width: i32,
    pub text_view: gtk4::TextView,
    pub chips_row: gtk4::Box,
    pub state: Rc<RefCell<AppState>>,
    pub update_send_btn_state: Rc<dyn Fn()>,
    pub menu_host: Rc<FloatingMenuHost>,
    pub mention_popup: Rc<RefCell<MentionPopup>>,
    pub btn_capture: gtk4::Button,
    pub capture_box: gtk4::Box,
    pub btn_more: gtk4::Button,
    pub more_stack: gtk4::Stack,
    pub btn_session_list: gtk4::Button,
    pub session_stack: gtk4::Stack,
    pub open_model_picker: Rc<dyn Fn()>,
    pub messages_box: gtk4::Box,
    pub chat_stack: gtk4::Stack,
    pub prompt_empty_label: gtk4::Label,
    pub prompts_list_box: gtk4::Box,
    pub active_stream: Rc<RefCell<Option<ActiveStreamTarget>>>,
    pub stream_tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    pub scroll_to_bottom: Rc<dyn Fn()>,
    pub is_streaming: Rc<Cell<bool>>,
}

pub fn run_audit_suite(args: AuditArgs) {
    let AuditArgs {
        app,
        window,
        dir,
        width,
        text_view,
        chips_row,
        state,
        update_send_btn_state,
        menu_host,
        mention_popup,
        btn_capture,
        capture_box,
        btn_more,
        more_stack,
        btn_session_list,
        session_stack,
        open_model_picker,
        messages_box,
        chat_stack,
        prompt_empty_label,
        prompts_list_box,
        active_stream,
        stream_tx,
        scroll_to_bottom,
        is_streaming,
    } = args;

    window.set_default_size(width, 820);
    window.set_size_request(width, 820);
    state.borrow_mut().settings.auto_hide_on_blur = false;

    glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
        let out_dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&out_dir);

        // 1. Welcome
        set_text_view_content(&text_view, "");
        let _ =
            audit::snapshot_window_with_popovers(&window, &[], &out_dir.join("after-welcome.png"));

        // 2. Mention + chip
        state
            .borrow_mut()
            .attached_files
            .push(("src/main.rs".into(), String::new()));
        add_context_chip(
            &chips_row,
            "main.rs",
            "src/main.rs",
            state.clone(),
            Some(update_send_btn_state.clone()),
        );
        set_text_view_content(&text_view, "Look at @");
        {
            let buf = text_view.buffer();
            buf.place_cursor(&buf.end_iter());
        }
        mention_popup.borrow_mut().refresh();
        audit::settle_ui(60);
        let _ =
            audit::snapshot_window_with_popovers(&window, &[], &out_dir.join("after-mention.png"));
        mention_popup.borrow_mut().close();

        // 3. Slash commands
        set_text_view_content(&text_view, "/");
        {
            let buf = text_view.buffer();
            buf.place_cursor(&buf.end_iter());
        }
        mention_popup.borrow_mut().refresh();
        audit::settle_ui(60);
        let _ =
            audit::snapshot_window_with_popovers(&window, &[], &out_dir.join("after-slash.png"));
        mention_popup.borrow_mut().close();
        set_text_view_content(&text_view, "");

        // 4. Screen Capture menu
        menu_host.open_menu(
            "capture",
            &btn_capture,
            &capture_box,
            240,
            MenuPlacement::AboveRight,
        );
        audit::settle_ui(80);
        let _ =
            audit::snapshot_window_with_popovers(&window, &[], &out_dir.join("after-capture.png"));
        menu_host.close_menu();

        // 5. Settings menu
        more_stack.set_visible_child_name("main");
        menu_host.open_menu(
            "more",
            &btn_more,
            &more_stack,
            260,
            MenuPlacement::BelowRight,
        );
        audit::settle_ui(80);
        let _ =
            audit::snapshot_window_with_popovers(&window, &[], &out_dir.join("after-settings.png"));
        menu_host.close_menu();

        // 6. Searchable Model Picker (Ctrl+.)
        open_model_picker();
        audit::settle_ui(80);
        let _ = audit::snapshot_window_with_popovers(
            &window,
            &[],
            &out_dir.join("after-model-picker.png"),
        );
        menu_host.close_menu();

        // 7. Conversation + Streaming + Permission
        chat_stack.set_visible_child_name("messages");
        let gen = state.borrow().harness.generation();

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
        messages_box.append(&u_card);

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
        fb.append(&icons::icon_button(
            "codicon-copy-symbolic",
            13,
            "Copy",
            &["feedback-btn"],
        ));
        fb.append(&icons::icon_button(
            "codicon-refresh-symbolic",
            13,
            "Retry",
            &["feedback-btn"],
        ));
        let fb_sp = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        fb_sp.set_hexpand(true);
        fb.append(&fb_sp);
        let m_badge = gtk4::Label::builder()
            .label("142 tok · 1.8s")
            .css_classes(["card-metrics-pill"])
            .build();
        fb.append(&m_badge);
        a_card.append(&fb);
        messages_box.append(&a_card);

        prompt_empty_label.set_visible(false);
        let h_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        h_row.add_css_class("prompt-row-box");
        let h_btn = gtk4::Button::builder()
            .css_classes(["prompt-item-btn"])
            .hexpand(true)
            .build();
        let h_inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        h_inner.append(
            &gtk4::Label::builder()
                .label("#1")
                .css_classes(["prompt-idx-pill"])
                .build(),
        );
        h_inner.append(
            &gtk4::Label::builder()
                .label("Refactor the UI so it looks…")
                .css_classes(["prompt-snippet-text"])
                .hexpand(true)
                .xalign(0.0)
                .build(),
        );
        h_btn.set_child(Some(&h_inner));
        h_row.append(&h_btn);
        h_row.append(&icons::icon_button(
            "codicon-trash-symbolic",
            12,
            "Delete",
            &["prompt-delete-btn"],
        ));
        prompts_list_box.append(&h_row);

        *active_stream.borrow_mut() = Some(ActiveStreamTarget {
            turn_body: t_body,
            full_text: Rc::new(RefCell::new(String::new())),
            thinking: Rc::new(RefCell::new(None)),
            progress: Rc::new(RefCell::new(None)),
            active_markdown: Rc::new(RefCell::new(None)),
            metrics_badge: m_badge,
        });
        is_streaming.set(true);
        update_send_btn_state();
        let _ = stream_tx.send((
            gen,
            StreamEvent::ToolStart {
                id: "t1".into(),
                name: "read_file".into(),
                args: r#"{"path":"resources/style.css"}"#.into(),
            },
        ));
        let _ = stream_tx.send((
            gen,
            StreamEvent::ToolEnd {
                id: "t1".into(),
                name: "read_file".into(),
                result: "".into(),
                is_error: false,
                duration_ms: 1200,
            },
        ));
        let _ = stream_tx.send((
            gen,
            StreamEvent::ToolStart {
                id: "t2".into(),
                name: "bash".into(),
                args: r#"{"command":"cargo test --quiet"}"#.into(),
            },
        ));
        audit::settle_ui(150);
        let _ = audit::snapshot_window_with_popovers(
            &window,
            &[],
            &out_dir.join("after-streaming.png"),
        );

        let _ = stream_tx.send((
            gen,
            StreamEvent::ToolEnd {
                id: "t2".into(),
                name: "bash".into(),
                result: "".into(),
                is_error: false,
                duration_ms: 800,
            },
        ));
        let _ = stream_tx.send((
            gen,
            StreamEvent::ThinkingDelta(
                "Inspecting resources/style.css and composer hierarchy...".into(),
            ),
        ));
        let _ = stream_tx.send((gen, StreamEvent::TextDelta("Updated `.chat-input-container` with unified borders and bundled Codicon SVGs:\n\n```css\n.chat-input-container {\n  background: #262626;\n  border-radius: 8px;\n}\n```\n\n| Component | Status |\n| --- | --- |\n| Composer | Unified |\n| Icons | Codicon |\n".into())));
        let _ = stream_tx.send((
            gen,
            StreamEvent::PermissionRequest(backend::PermissionRequest {
                request_id: 99,
                title: "shell command".into(),
                detail: "cargo test --workspace".into(),
                options: vec![
                    ("allow_once".into(), "Allow".into()),
                    ("reject_once".into(), "Skip".into()),
                ],
            }),
        ));
        let _ = stream_tx.send((gen, StreamEvent::Done));

        audit::settle_ui(180);
        scroll_to_bottom();
        let _ = audit::snapshot_window_with_popovers(
            &window,
            &[],
            &out_dir.join("after-conversation.png"),
        );

        // 8. History menu
        session_stack.set_visible_child_name("list");
        menu_host.open_menu(
            "session",
            &btn_session_list,
            &session_stack,
            260,
            MenuPlacement::BelowRight,
        );
        audit::settle_ui(80);
        let _ =
            audit::snapshot_window_with_popovers(&window, &[], &out_dir.join("after-history.png"));
        menu_host.close_menu();

        window.close();
        app.quit();
    });
}
