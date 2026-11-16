//! Top-level GTK4 overlay drawer UI wiring layout, controllers, menus, and harnesses.

#[cfg(feature = "audit")]
mod audit_flow;
mod mentions_popup;
mod menus;
pub mod state;
mod stream;
mod submit;
mod widgets;

use crate::backend::{self, StreamEvent};
use crate::format;
use crate::icons;
use crate::ipc;
use crate::markup;
use crate::mentions;
use crate::menu::FloatingMenuHost;
use crate::overlay;
use crate::settings::AppSettings;
use crate::tray;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use menus::{build_menus, MenuBuildArgs};
use state::{
    collapse_and_finalize_thinking, model_display_name_with_list, mode_display_name,
    thinking_display_name, workspace_label, ActiveStreamTarget, AppContext, AppState, OverlayMode,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use stream::{spawn_stream_consumer, StreamConsumerArgs};
use submit::{build_submit_prompt, SubmitArgs};
use tokio::sync::mpsc;
use widgets::{get_text_view_content, make_picker_button, set_text_view_content};

pub fn build_ui(
    app: &gtk4::Application,
    is_window_mode: bool,
    is_daemon: bool,
    scale_css_provider: gtk4::CssProvider,
    #[allow(unused_variables)] audit_dir: Option<String>,
) -> AppContext {
    let hold_guard = app.hold();
    let initial_settings = AppSettings::load();
    let initial_models = backend::query_pi_models_sync();

    crate::settings::apply_text_scale(
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

    let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    root_box.set_halign(gtk4::Align::End);
    root_box.set_vexpand(true);
    root_box.set_hexpand(false);

    let left_col = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    left_col.set_width_request(3);
    left_col.set_vexpand(true);
    let resize_handle_w = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    resize_handle_w.add_css_class("resize-handle");
    resize_handle_w.set_cursor_from_name(Some("ew-resize"));
    resize_handle_w.set_vexpand(true);
    left_col.append(&resize_handle_w);
    root_box.append(&left_col);

    let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    drawer.set_widget_name("drawer-container");
    drawer.add_css_class("drawer-container");
    drawer.set_hexpand(false);
    drawer.set_width_request(initial_settings.width);

    // ── 1. Panel Header (VS Code 35px View Header) ──────────────────────────
    let panel_header = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    panel_header.add_css_class("panel-header");
    panel_header.set_hexpand(false);
    panel_header.append(
        &gtk4::Label::builder()
            .label("CHAT")
            .css_classes(["panel-title"])
            .halign(gtk4::Align::Start)
            .build(),
    );
    let header_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    header_spacer.set_hexpand(true);
    panel_header.append(&header_spacer);

    let btn_new_chat = icons::icon_button("codicon-add-symbolic", 15, "New Chat (Ctrl+N)", &["header-icon-btn"]);
    let btn_session_list = icons::icon_button("codicon-history-symbolic", 15, "Show Chats / History", &["header-icon-btn"]);
    let btn_more = icons::icon_button("codicon-settings-gear-symbolic", 15, "Settings & Session Options", &["header-icon-btn"]);
    let btn_close = icons::icon_button("codicon-close-symbolic", 15, "Close Overlay (Esc)", &["header-icon-btn", "close"]);
    panel_header.append(&btn_new_chat);
    panel_header.append(&btn_session_list);
    panel_header.append(&btn_more);
    panel_header.append(&btn_close);
    drawer.append(&panel_header);

    // ── 2. Session Sub-Header (workspace + harness) ─────────────────────────
    let session_subheader = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    session_subheader.add_css_class("session-subheader");

    let btn_workspace = gtk4::Button::builder()
        .css_classes(["session-chip-btn"])
        .tooltip_text(&initial_settings.workspace_dir)
        .valign(gtk4::Align::Center)
        .build();
    let ws_inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    ws_inner.set_valign(gtk4::Align::Center);
    ws_inner.append(&icons::image("codicon-folder-symbolic", 12));
    let lbl_workspace = gtk4::Label::builder()
        .label(workspace_label(&initial_settings.workspace_dir))
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .max_width_chars(20)
        .valign(gtk4::Align::Center)
        .yalign(0.5)
        .build();
    ws_inner.append(&lbl_workspace);
    btn_workspace.set_child(Some(&ws_inner));
    session_subheader.append(&btn_workspace);

    session_subheader.append(&gtk4::Label::builder().label("/").css_classes(["session-sep"]).build());

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

    // ── 3. Chat Content Stack (Welcome vs Scrollable Stream) ────────────────
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
    welcome_box.append(
        &gtk4::Label::builder()
            .label("Build with agent")
            .css_classes(["welcome-title"])
            .halign(gtk4::Align::Center)
            .build(),
    );
    welcome_box.append(
        &gtk4::Label::builder()
            .label("Ask about your workspace, run edits, or attach live screen context with @screen.")
            .css_classes(["welcome-subtitle"])
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::WordChar)
            .justify(gtk4::Justification::Center)
            .max_width_chars(38)
            .halign(gtk4::Align::Center)
            .build(),
    );

    let starters_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    starters_box.set_margin_top(6);
    starters_box.set_width_request(290);
    starters_box.set_halign(gtk4::Align::Center);

    let mut starter_buttons: Vec<(gtk4::Button, &'static str)> = Vec::new();
    for (icon_name, title, prompt_template) in [
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
    ] {
        let s_btn = gtk4::Button::builder()
            .css_classes(["welcome-starter-btn"])
            .halign(gtk4::Align::Fill)
            .build();
        let s_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        let s_icon = icons::image(icon_name, 14);
        s_icon.add_css_class("welcome-starter-icon");
        s_row.append(&s_icon);
        s_row.append(
            &gtk4::Label::builder()
                .label(title)
                .xalign(0.0)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .hexpand(true)
                .build(),
        );
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
    let btn_scroll_bottom = icons::icon_button("codicon-arrow-down-symbolic", 13, "Scroll to bottom", &["btn-scroll-bottom"]);
    btn_scroll_bottom.set_halign(gtk4::Align::End);
    btn_scroll_bottom.set_valign(gtk4::Align::End);
    btn_scroll_bottom.set_margin_end(14);
    btn_scroll_bottom.set_margin_bottom(10);
    btn_scroll_bottom.set_visible(false);
    scrolled_overlay.add_overlay(&btn_scroll_bottom);
    chat_stack.add_named(&scrolled_overlay, Some("messages"));
    chat_stack.set_visible_child_name("welcome");
    drawer.append(&chat_stack);

    // ── 4. Unified Composer (.chat-input-container) ─────────────────────────
    let composer_outer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    composer_outer.add_css_class("composer-outer");
    let composer_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    composer_box.add_css_class("chat-input-container");

    let context_top_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    context_top_row.add_css_class("composer-context-row");

    let btn_attach = gtk4::Button::builder()
        .css_classes(["add-context-btn"])
        .tooltip_text("Add file or context")
        .valign(gtk4::Align::Center)
        .build();
    let add_ctx_inner = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    add_ctx_inner.set_valign(gtk4::Align::Center);
    add_ctx_inner.append(&icons::image("codicon-add-symbolic", 11));
    add_ctx_inner.append(
        &gtk4::Label::builder()
            .label("Add Context…")
            .valign(gtk4::Align::Center)
            .yalign(0.5)
            .build(),
    );
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

    let composer_toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    composer_toolbar.add_css_class("composer-toolbar");

    let initial_mode_name = mode_display_name(&initial_settings.acp_mode);
    let (btn_mode_pill, lbl_mode_pill) = make_picker_button(&initial_mode_name, "composer-picker-btn", "Switch Agent Mode");
    composer_toolbar.append(&btn_mode_pill);

    let initial_model_name = model_display_name_with_list(&initial_settings.model, &initial_models);
    let (btn_model_pill, lbl_model_pill) = make_picker_button(&initial_model_name, "composer-picker-btn", "Select Language Model (Ctrl+.)");
    lbl_model_pill.set_max_width_chars(24);
    composer_toolbar.append(&btn_model_pill);

    let initial_think_name = thinking_display_name(&initial_settings.thinking_level);
    let (btn_think_pill, lbl_think_pill) = make_picker_button(&initial_think_name, "composer-picker-btn", "Reasoning Depth");
    composer_toolbar.append(&btn_think_pill);

    let tb_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    tb_spacer.set_hexpand(true);
    composer_toolbar.append(&tb_spacer);

    let btn_capture = icons::icon_button("codicon-device-camera-symbolic", 14, "Capture screen context (Ctrl+Shift+S)", &["composer-tool-btn"]);
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

    let btn_send_stop = icons::icon_button("codicon-arrow-up-symbolic", 13, "Send (Enter)", &["composer-send-btn"]);
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
    let update_send_btn_state: Rc<dyn Fn()> = {
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
            pl_ref.set_visible(buf.char_count() == 0);
            update_btn();
        });
    }

    let menu_host = FloatingMenuHost::new(&drawer);
    let workspace_index = Rc::new(RefCell::new(mentions::WorkspaceIndex::build(
        std::path::Path::new(&initial_settings.workspace_dir),
    )));
    let slash_commands = Rc::new(RefCell::new(mentions::builtin_commands()));
    let mention_popup = mentions_popup::MentionPopup::attach(
        menu_host.clone(),
        &composer_box,
        &text_view,
        workspace_index.clone(),
        slash_commands.clone(),
    );

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
        available_models: initial_models,
        available_modes: Vec::new(),
        available_thinking_levels: Vec::new(),
        slash_commands,
    }));

    let (stream_tx, stream_rx) = mpsc::unbounded_channel::<(u64, StreamEvent)>();
    let active_stream: Rc<RefCell<Option<ActiveStreamTarget>>> = Rc::new(RefCell::new(None));
    let active_markup_overlay: Rc<RefCell<Option<markup::AgentMarkupOverlay>>> =
        Rc::new(RefCell::new(None));

    // Width resize drag on left grip
    {
        let drag = gtk4::GestureDrag::new();
        let start_w = Rc::new(Cell::new(initial_settings.width));
        let drawer_d = drawer.clone();
        let st_d = state.clone();
        let sw = start_w.clone();
        drag.connect_drag_begin(move |_, _, _| {
            sw.set(st_d.borrow().settings.width);
        });
        let drawer_u = drawer.clone();
        let st_u = state.clone();
        drag.connect_drag_update(move |_, dx, _| {
            let next_w = (start_w.get() - dx as i32).clamp(crate::settings::MIN_WIDTH, crate::settings::MAX_WIDTH);
            drawer_u.set_width_request(next_w);
            st_u.borrow_mut().settings.width = next_w;
        });
        let st_e = state.clone();
        let _ = drawer_d;
        drag.connect_drag_end(move |_, _, _| {
            let _ = st_e.borrow().settings.save();
        });
        resize_handle_w.add_controller(drag);
    }

    {
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
                let Some(file) = dialog.file() else { return };
                let Some(path) = file.path() else { return };
                let dir = path.to_string_lossy().to_string();
                {
                    let mut st = state_cb.borrow_mut();
                    st.settings.workspace_dir = dir.clone();
                    st.harness.set_workspace_dir(&dir);
                    let _ = st.settings.save();
                }
                *index_cb.borrow_mut() = mentions::WorkspaceIndex::build(&path);
                lbl_cb.set_label(&workspace_label(&dir));
                btn_cb.set_tooltip_text(Some(&dir));
            });
            chooser.show();
        });
    }

    let menu_handles = build_menus(MenuBuildArgs {
        app: app.clone(),
        window: window.clone(),
        menu_host: menu_host.clone(),
        state: state.clone(),
        scale_css_provider,
        workspace_index,
        stream_tx: stream_tx.clone(),
        text_view: text_view.clone(),
        chips_row: chips_row.clone(),
        messages_box: messages_box.clone(),
        chat_stack: chat_stack.clone(),
        active_stream: active_stream.clone(),
        active_markup_overlay: active_markup_overlay.clone(),
        is_streaming: is_streaming.clone(),
        update_send_btn_state: update_send_btn_state.clone(),
        btn_harness,
        lbl_harness,
        btn_attach,
        btn_capture: btn_capture.clone(),
        btn_mode_pill,
        lbl_mode_pill: lbl_mode_pill.clone(),
        btn_model_pill,
        lbl_model_pill: lbl_model_pill.clone(),
        btn_think_pill,
        lbl_think_pill,
        btn_session_list: btn_session_list.clone(),
        btn_more: btn_more.clone(),
        btn_new_chat,
        session_title_label: session_title_label.clone(),
        initial_settings: initial_settings.clone(),
    });

    let scroll_to_bottom: Rc<dyn Fn()> = {
        let scrolled = scrolled.clone();
        Rc::new(move || {
            glib::idle_add_local_once({
                let scrolled = scrolled.clone();
                move || {
                    let adj = scrolled.vadjustment();
                    adj.set_value(adj.upper() - adj.page_size());
                }
            });
        })
    };

    let scroll_to_widget: Rc<dyn Fn(&gtk4::Box)> = {
        let scrolled = scrolled.clone();
        Rc::new(move |widget: &gtk4::Box| {
            let (_, y) = widget.translate_coordinates(&scrolled, 0.0, 0.0).unwrap_or((0.0, 0.0));
            let adj = scrolled.vadjustment();
            let target_val = (adj.value() + y - 40.0).clamp(adj.lower(), adj.upper() - adj.page_size());
            adj.set_value(target_val);
        })
    };

    {
        let btn_scroll_ref = btn_scroll_bottom.clone();
        scrolled.vadjustment().connect_value_changed(move |adj| {
            let at_bottom = adj.value() >= adj.upper() - adj.page_size() - 40.0;
            btn_scroll_ref.set_visible(!at_bottom);
        });
        let scroll_fn = scroll_to_bottom.clone();
        btn_scroll_bottom.connect_clicked(move |_| scroll_fn());
    }

    let hide_overlay_fn: Rc<dyn Fn()> = {
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

    let expand_overlay_fn: Rc<dyn Fn()> = {
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

    {
        let expand_fn = expand_overlay_fn.clone();
        let hide_fn = hide_overlay_fn.clone();
        let state_toggle = state.clone();
        tray::setup_tray(move || {
            if state_toggle.borrow().mode == OverlayMode::Visible {
                hide_fn();
            } else {
                expand_fn();
            }
        });
    }

    {
        let hide_fn = hide_overlay_fn.clone();
        btn_close.connect_clicked(move |_| hide_fn());
    }

    spawn_stream_consumer(StreamConsumerArgs {
        app: app.clone(),
        stream_rx,
        active_stream: active_stream.clone(),
        active_markup_overlay: active_markup_overlay.clone(),
        state: state.clone(),
        scroll_to_bottom: scroll_to_bottom.clone(),
        update_send_btn_state: update_send_btn_state.clone(),
        is_streaming: is_streaming.clone(),
        session_title_label: session_title_label.clone(),
        lbl_model_pill,
        lbl_mode_pill,
        rebuild_catalogs: menu_handles.rebuild_catalogs.clone(),
    });

    let submit_prompt = build_submit_prompt(SubmitArgs {
        window: window.clone(),
        text_view: text_view.clone(),
        chips_row: chips_row.clone(),
        messages_box: messages_box.clone(),
        chat_stack: chat_stack.clone(),
        prompts_list_box: menu_handles.prompts_list_box.clone(),
        prompt_empty_label: menu_handles.prompt_empty_label.clone(),
        menu_host: menu_host.clone(),
        session_stack: menu_handles.session_stack.clone(),
        pending_prompt_delete: menu_handles.pending_prompt_delete,
        state: state.clone(),
        active_stream: active_stream.clone(),
        active_markup_overlay,
        stream_tx: stream_tx.clone(),
        scroll_to_bottom: scroll_to_bottom.clone(),
        scroll_to_widget,
        is_streaming: is_streaming.clone(),
        update_send_btn_state: update_send_btn_state.clone(),
        session_title_label,
        execute_clear: menu_handles.execute_clear,
        capture_screen_fn: menu_handles.capture_screen_fn,
        capture_markup_fn: menu_handles.capture_markup_fn,
    });

    // Keyboard controller on composer TextView
    {
        let key_controller = gtk4::EventControllerKey::new();
        let submit_on_enter = submit_prompt.clone();
        let popup_state = mention_popup.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, modifier| {
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
                    gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::Tab
                        if mentions_popup::accept_mention(&popup_state) =>
                    {
                        return glib::Propagation::Stop;
                    }
                    _ => {}
                }
            }
            if (keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter)
                && !modifier.contains(gdk::ModifierType::SHIFT_MASK)
            {
                submit_on_enter();
                return glib::Propagation::Stop;
            }
            if keyval == gdk::Key::space && modifier.contains(gdk::ModifierType::CONTROL_MASK) {
                popup_state.borrow_mut().refresh();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        text_view.add_controller(key_controller);
    }

    {
        let is_str_btn = is_streaming.clone();
        let state_stop = state.clone();
        let submit_btn = submit_prompt.clone();
        let active_stream_stop = active_stream.clone();
        let update_btn_stop = update_send_btn_state.clone();
        btn_send_stop.connect_clicked(move |_| {
            if is_str_btn.get() {
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

    // Global Window Shortcuts: Esc, Ctrl+Alt+L / Meta+Alt+L, Ctrl+., Ctrl+N, Ctrl+Shift+S
    {
        let key_controller = gtk4::EventControllerKey::new();
        let hide_fn = hide_overlay_fn.clone();
        let mh_esc = menu_host.clone();
        let mp_esc = mention_popup.clone();
        let req_nc_key = menu_handles.request_new_chat.clone();
        let open_mdl_key = menu_handles.open_model_picker.clone();
        let btn_cap_key = btn_capture.clone();
        let tv_esc = text_view.clone();

        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            let ctrl = modifier.contains(gdk::ModifierType::CONTROL_MASK);
            let alt = modifier.contains(gdk::ModifierType::ALT_MASK);
            let super_mod = modifier.contains(gdk::ModifierType::SUPER_MASK)
                || modifier.contains(gdk::ModifierType::META_MASK);

            if ((ctrl && alt) || (super_mod && alt)) && (key == gdk::Key::l || key == gdk::Key::L) {
                hide_fn();
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Escape {
                if mp_esc.borrow().is_visible() {
                    mp_esc.borrow_mut().close();
                    tv_esc.grab_focus();
                    return glib::Propagation::Stop;
                }
                if mh_esc.is_menu_open() {
                    mh_esc.close_menu();
                    tv_esc.grab_focus();
                    return glib::Propagation::Stop;
                }
                hide_fn();
                return glib::Propagation::Stop;
            }
            if ctrl && key == gdk::Key::period {
                open_mdl_key();
                return glib::Propagation::Stop;
            }
            if ctrl && (key == gdk::Key::n || key == gdk::Key::N) {
                req_nc_key();
                return glib::Propagation::Stop;
            }
            if ctrl
                && modifier.contains(gdk::ModifierType::SHIFT_MASK)
                && (key == gdk::Key::S || key == gdk::Key::s)
            {
                btn_cap_key.emit_clicked();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(key_controller);
    }

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
            if prev && !has_focus && state_focus.borrow().settings.auto_hide_on_blur {
                mh_focus.close_menu();
                mp_focus.borrow_mut().close();
                hide_fn();
            }
        });
        window.add_controller(focus_controller);
    }

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
                        if state_ipc.borrow().mode == OverlayMode::Visible {
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

    if !is_daemon {
        expand_overlay_fn();
    }

    if audit_dir.is_none() && initial_settings.harness.is_acp() {
        state.borrow_mut().harness.warmup(stream_tx.clone());
    }

    #[cfg(feature = "audit")]
    if let Some(dir) = audit_dir {
        audit_flow::run_audit_suite(audit_flow::AuditArgs {
            app: app.clone(),
            window: window.clone(),
            dir,
            width: initial_settings.width,
            text_view,
            chips_row,
            state: state.clone(),
            update_send_btn_state,
            menu_host,
            mention_popup,
            btn_capture,
            capture_box: menu_handles.capture_box,
            btn_more,
            more_stack: menu_handles.more_stack,
            btn_session_list,
            session_stack: menu_handles.session_stack,
            open_model_picker: menu_handles.open_model_picker,
            messages_box,
            chat_stack,
            prompt_empty_label: menu_handles.prompt_empty_label,
            prompts_list_box: menu_handles.prompts_list_box,
            active_stream,
            stream_tx,
            scroll_to_bottom,
            is_streaming,
        });
    }

    AppContext {
        window,
        state,
        hold_guard,
    }
}
