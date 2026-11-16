//! Prompt submission handler, local slash commands, live screen capture, and turn cards.

use super::state::{model_display_name_with_list, ActiveStreamTarget, AppState, DeferredAction, TrackedPrompt};
use super::widgets::{get_text_view_content, set_text_view_content};
use crate::backend::StreamEvent;
use crate::capture;
use crate::commands;
use crate::icons;
use crate::markup;
use crate::menu::FloatingMenuHost;
use gtk4::gdk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio::sync::mpsc;

pub struct SubmitArgs {
    pub window: gtk4::ApplicationWindow,
    pub text_view: gtk4::TextView,
    pub chips_row: gtk4::Box,
    pub messages_box: gtk4::Box,
    pub chat_stack: gtk4::Stack,
    pub prompts_list_box: gtk4::Box,
    pub prompt_empty_label: gtk4::Label,
    pub menu_host: Rc<FloatingMenuHost>,
    pub session_stack: gtk4::Stack,
    pub pending_prompt_delete: DeferredAction,
    pub state: Rc<RefCell<AppState>>,
    pub active_stream: Rc<RefCell<Option<ActiveStreamTarget>>>,
    pub active_markup_overlay: Rc<RefCell<Option<markup::AgentMarkupOverlay>>>,
    pub stream_tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    pub scroll_to_bottom: Rc<dyn Fn()>,
    pub scroll_to_widget: Rc<dyn Fn(&gtk4::Box)>,
    pub is_streaming: Rc<Cell<bool>>,
    pub update_send_btn_state: Rc<dyn Fn()>,
    pub session_title_label: gtk4::Label,
    pub execute_clear: Rc<dyn Fn()>,
    pub capture_screen_fn: Rc<dyn Fn()>,
    pub capture_markup_fn: Rc<dyn Fn()>,
}

pub fn build_submit_prompt(args: SubmitArgs) -> Rc<dyn Fn()> {
    let SubmitArgs {
        window,
        text_view,
        chips_row,
        messages_box,
        chat_stack,
        prompts_list_box,
        prompt_empty_label,
        menu_host,
        session_stack,
        pending_prompt_delete,
        state,
        active_stream,
        active_markup_overlay,
        stream_tx,
        scroll_to_bottom,
        scroll_to_widget,
        is_streaming,
        update_send_btn_state,
        session_title_label,
        execute_clear,
        capture_screen_fn,
        capture_markup_fn,
    } = args;

    let tv_entry = text_view.clone();
    let state_entry = state.clone();
    let active_markup_entry = active_markup_overlay.clone();
    let update_btn_entry = update_send_btn_state.clone();
    let is_streaming_entry = is_streaming.clone();
    let clear_fn_entry = execute_clear;
    let capture_fn_entry = capture_screen_fn;
    let markup_fn_entry = capture_markup_fn;
    let window_submit = window;

    let pending: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    let deliver: Rc<dyn Fn()> = {
        let pending = pending.clone();
        Rc::new(move || {
            let raw_prompt = pending.borrow().clone();
            if raw_prompt.is_empty() {
                return;
            }
            let prompt = commands::expand_slash_command(&raw_prompt).unwrap_or(raw_prompt);

            update_send_btn_state();
            chat_stack.set_visible_child_name("messages");

            let prompt_idx = state.borrow().prompts.len() + 1;
            let snippet = if prompt.chars().count() > 24 {
                format!("{}…", prompt.chars().take(24).collect::<String>())
            } else {
                prompt.clone()
            };
            session_title_label.set_label(&snippet);

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

            user_card.append(
                &gtk4::Label::builder()
                    .label(&prompt)
                    .wrap(true)
                    .wrap_mode(gtk4::pango::WrapMode::WordChar)
                    .width_chars(1)
                    .max_width_chars(64)
                    .selectable(true)
                    .xalign(0.0)
                    .css_classes(["card-user-text"])
                    .build(),
            );
            messages_box.append(&user_card);

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

            let cur_model_label = {
                let st = state.borrow();
                model_display_name_with_list(&st.settings.model, &st.available_models)
            };
            asst_header.append(&avatar_wrap);
            asst_header.append(
                &gtk4::Label::builder()
                    .label("Assistant")
                    .css_classes(["card-assistant-author"])
                    .build(),
            );
            asst_header.append(
                &gtk4::Label::builder()
                    .label(&cur_model_label)
                    .css_classes(["card-assistant-model"])
                    .build(),
            );
            asst_card.append(&asst_header);

            let turn_body = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
            asst_card.append(&turn_body);

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
                    gtk4::glib::timeout_add_local_once(
                        std::time::Duration::from_millis(1500),
                        move || {
                            icons::set_button_icon(&reset_btn, "codicon-copy-symbolic", 13);
                            reset_btn.remove_css_class("copied");
                        },
                    );
                });
            }

            {
                let retry_prompt = prompt.clone();
                let tv_retry = text_view.clone();
                btn_retry.connect_clicked(move |_| {
                    set_text_view_content(&tv_retry, &retry_prompt);
                    tv_retry.grab_focus();
                });
            }

            prompt_empty_label.set_visible(false);
            let row_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
            row_box.add_css_class("prompt-row-box");

            let nav_btn = gtk4::Button::builder()
                .css_classes(["prompt-item-btn"])
                .hexpand(true)
                .build();
            let nav_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            nav_row.append(
                &gtk4::Label::builder()
                    .label(format!("#{}", prompt_idx))
                    .css_classes(["prompt-idx-pill"])
                    .build(),
            );
            nav_row.append(
                &gtk4::Label::builder()
                    .label(&snippet)
                    .css_classes(["prompt-snippet-text"])
                    .ellipsize(gtk4::pango::EllipsizeMode::End)
                    .xalign(0.0)
                    .hexpand(true)
                    .build(),
            );
            nav_btn.set_child(Some(&nav_row));

            let target_widget = user_card.clone();
            let mh_nav = menu_host.clone();
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
                let is_str_ref = is_streaming.clone();
                let update_btn_ref = update_send_btn_state.clone();
                let this_idx = prompt_idx;
                let ss_del = session_stack.clone();
                let pending_del = pending_prompt_delete.clone();

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
                        if st.prompts.is_empty() {
                            p_empty_lbl.set_visible(true);
                            chat_stack_ref.set_visible_child_name("welcome");
                        }
                    }));
                    ss_del.set_visible_child_name("confirm_delete");
                });
            }

            state.borrow_mut().prompts.push(TrackedPrompt {
                index: prompt_idx,
                user_card,
            });

            *active_stream.borrow_mut() = Some(ActiveStreamTarget {
                turn_body: turn_body.clone(),
                full_text: full_text.clone(),
                thinking: Rc::new(RefCell::new(None)),
                progress: Rc::new(RefCell::new(None)),
                active_markdown: Rc::new(RefCell::new(None)),
                metrics_badge,
            });

            scroll_to_bottom();
            is_streaming.set(true);
            update_send_btn_state();

            let mut st = state.borrow_mut();
            let model_opt = Some(st.settings.model.clone());
            let thinking_opt = Some(st.settings.thinking_level.clone());
            let mode_opt = Some(st.current_mode.clone());
            let attached = st.attached_files.clone();
            if let Err(e) = st.harness.spawn_prompt(
                prompt.clone(),
                attached,
                model_opt,
                thinking_opt,
                mode_opt,
                stream_tx.clone(),
            ) {
                set_text_view_content(&text_view, &prompt);
                let err_str = format!("[Error] Failed to spawn agent: {e}");
                full_text.borrow_mut().push_str(&err_str);
                let err_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                turn_body.append(&err_box);
                crate::render::render_markdown_blocks_into(&err_box, &err_str);
                is_streaming.set(false);
                update_send_btn_state();
                scroll_to_bottom();
            } else {
                st.attached_files.clear();
                while let Some(child) = chips_row.first_child() {
                    chips_row.remove(&child);
                }
            }
        })
    };

    Rc::new(move || {
        if is_streaming_entry.get() {
            return;
        }
        let raw_prompt = get_text_view_content(&tv_entry);
        if raw_prompt.is_empty() {
            return;
        }

        if let Some(cmd) = commands::parse_local_command(&raw_prompt) {
            use commands::LocalCommand;
            set_text_view_content(&tv_entry, "");
            match cmd {
                LocalCommand::NewSession | LocalCommand::ClearConversation => clear_fn_entry(),
                LocalCommand::CaptureScreen
                | LocalCommand::CaptureRegion
                | LocalCommand::CaptureWindow => capture_fn_entry(),
                LocalCommand::OpenMarkup => markup_fn_entry(),
                LocalCommand::ShowHelp => {
                    set_text_view_content(
                        &tv_entry,
                        "Commands: /new /clear /screen /region /window /markup /help",
                    );
                }
            }
            return;
        }

        if let Some(prev) = active_markup_entry.borrow_mut().take() {
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
        set_text_view_content(&tv_entry, "");
        update_btn_entry();

        if let (Some(cap_mode), false) = (live_capture_mode, has_screen) {
            let state_cap = state_entry.clone();
            let deliver_after = deliver.clone();
            capture::capture_mode_hiding_window(&window_submit, cap_mode, move |res| {
                if let Ok(path) = res {
                    let path_str = path.to_string_lossy().to_string();
                    let content = std::fs::read(&path)
                        .map(|b| capture::base64_encode(&b))
                        .unwrap_or_default();
                    state_cap.borrow_mut().attached_files.push((path_str, content));
                }
                deliver_after();
            });
        } else {
            deliver();
        }
    })
}
