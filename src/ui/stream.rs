//! Async `StreamEvent` consumer driving assistant turns, live markup, and catalogs.

use super::state::{
    apply_session_config, collapse_and_finalize_progress, collapse_and_finalize_thinking,
    mode_display_name, model_display_name_with_list, ActiveStreamTarget, AppState,
    ProgressDisclosureState, StepItemEntry, ThinkingDisclosureState,
};
use super::widgets::{build_permission_card, MarkdownStream};
use crate::backend::StreamEvent;
use crate::format;
use crate::icons;
use crate::markup;
use crate::overlay;
use crate::render;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::mpsc;

pub struct StreamConsumerArgs {
    pub app: gtk4::Application,
    pub stream_rx: mpsc::UnboundedReceiver<(u64, StreamEvent)>,
    pub active_stream: Rc<RefCell<Option<ActiveStreamTarget>>>,
    pub active_markup_overlay: Rc<RefCell<Option<markup::AgentMarkupOverlay>>>,
    pub state: Rc<RefCell<AppState>>,
    pub scroll_to_bottom: Rc<dyn Fn()>,
    pub update_send_btn_state: Rc<dyn Fn()>,
    pub is_streaming: Rc<Cell<bool>>,
    pub session_title_label: gtk4::Label,
    pub lbl_model_pill: gtk4::Label,
    pub lbl_mode_pill: gtk4::Label,
    pub rebuild_catalogs: Rc<dyn Fn()>,
}

pub fn spawn_stream_consumer(args: StreamConsumerArgs) {
    let StreamConsumerArgs {
        app,
        mut stream_rx,
        active_stream,
        active_markup_overlay,
        state,
        scroll_to_bottom,
        update_send_btn_state,
        is_streaming,
        session_title_label,
        lbl_model_pill,
        lbl_mode_pill,
        rebuild_catalogs,
    } = args;

    glib::spawn_future_local(async move {
        while let Some((gen, event)) = stream_rx.recv().await {
            if gen != state.borrow().harness.generation() {
                continue;
            }

            match event {
                StreamEvent::SessionConfig(cfg) => {
                    apply_session_config(&state, &cfg);
                    {
                        let st = state.borrow();
                        if !st.settings.model.is_empty() {
                            lbl_model_pill.set_label(&model_display_name_with_list(
                                &st.settings.model,
                                &st.available_models,
                            ));
                        } else if st.settings.harness.is_acp() {
                            lbl_model_pill.set_label("Auto");
                        }
                        if !st.current_mode.is_empty() {
                            lbl_mode_pill.set_label(&mode_display_name(&st.current_mode));
                        }
                    }
                    rebuild_catalogs();
                }
                StreamEvent::PermissionRequest(req) => {
                    if let Some(ref target) = *active_stream.borrow() {
                        let card = build_permission_card(&req, &state);
                        target.turn_body.append(&card);
                    }
                    scroll_to_bottom();
                }
                StreamEvent::ThinkingDelta(delta) => {
                    if let Some(ref target) = *active_stream.borrow() {
                        collapse_and_finalize_progress(target);
                        *target.active_markdown.borrow_mut() = None;

                        let mut th_opt = target.thinking.borrow_mut();
                        if th_opt.is_none() {
                            let disclosure = super::widgets::Disclosure::new("Thinking…", true);
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
                    scroll_to_bottom();
                }
                StreamEvent::ToolStart { id, name, args } => {
                    if let Some(ref target) = *active_stream.borrow() {
                        collapse_and_finalize_thinking(target);
                        *target.active_markdown.borrow_mut() = None;

                        let mut p_opt = target.progress.borrow_mut();
                        if p_opt.is_none() {
                            let disclosure =
                                super::widgets::Disclosure::new("Running steps…", true);
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
                    scroll_to_bottom();
                }
                StreamEvent::ToolEnd {
                    id,
                    name,
                    result,
                    is_error,
                    duration_ms,
                } => {
                    if let Some(ref target) = *active_stream.borrow() {
                        collapse_and_finalize_thinking(target);

                        let mut p_opt = target.progress.borrow_mut();
                        if p_opt.is_none() {
                            let disclosure =
                                super::widgets::Disclosure::new("Running steps…", true);
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

                            if let Some(item) = p.items.get(&id) {
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
                                    let out_disc = super::widgets::Disclosure::new("Output", false);
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
                                format!(
                                    "Completed {} steps with errors in {}",
                                    p.steps_count, dur_str
                                )
                            } else {
                                format!("Completed {} steps in {}", p.steps_count, dur_str)
                            };
                            p.disclosure.set_title(&lbl);
                        }
                        *target.active_markdown.borrow_mut() = None;
                    }
                    scroll_to_bottom();
                }
                StreamEvent::ToolCall { name, args } => {
                    if let Some(ref target) = *active_stream.borrow() {
                        collapse_and_finalize_thinking(target);

                        let mut p_opt = target.progress.borrow_mut();
                        if p_opt.is_none() {
                            let disclosure = super::widgets::Disclosure::new("Steps", true);
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
                    scroll_to_bottom();
                }
                StreamEvent::TextDelta(delta) => {
                    if let Some(ref target) = *active_stream.borrow() {
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
                            sync_live_markup_overlay(&app, &active_markup_overlay, &full_snapshot);
                        }
                    }
                    scroll_to_bottom();
                }
                StreamEvent::Content(line) => {
                    if let Some(ref target) = *active_stream.borrow() {
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
                            sync_live_markup_overlay(&app, &active_markup_overlay, &full_snapshot);
                        }
                    }
                    scroll_to_bottom();
                }
                StreamEvent::Metrics(m) => {
                    if let Some(ref target) = *active_stream.borrow() {
                        target.metrics_badge.set_visible(true);
                        let sec = (m.duration_ms as f64) / 1000.0;
                        target
                            .metrics_badge
                            .set_text(&format!("{} tok • {:.1}s", m.total_tokens, sec));
                    }
                }
                StreamEvent::Error(err_line) => {
                    let clean_err = format::strip_ansi_escapes(&err_line);
                    if let Some(ref target) = *active_stream.borrow() {
                        collapse_and_finalize_thinking(target);
                        collapse_and_finalize_progress(target);
                        let err_msg = format!("[Error] {clean_err}");

                        let mut md_opt = target.active_markdown.borrow_mut();
                        if md_opt.is_none() {
                            let md_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
                            md_box.add_css_class("markdown-stream");
                            target.turn_body.append(&md_box);
                            *md_opt = Some(MarkdownStream::new(md_box));
                        }
                        if let Some(stream) = md_opt.as_mut() {
                            stream.push(&format!("\n\n{err_msg}\n"));
                        }
                    } else if state.borrow().settings.harness.is_acp() {
                        // Warmup failed (e.g. binary missing or unauthenticated).
                        lbl_model_pill.set_label("Offline");
                        lbl_model_pill.set_tooltip_text(Some(&clean_err));
                    }
                    is_streaming.set(false);
                    update_send_btn_state();
                    scroll_to_bottom();
                }
                StreamEvent::SessionTitle(title) => {
                    session_title_label.set_label(&title);
                }
                StreamEvent::Done => {
                    let mut agent_shapes = Vec::new();
                    if let Some(ref target) = *active_stream.borrow() {
                        collapse_and_finalize_thinking(target);
                        collapse_and_finalize_progress(target);
                        let full = target.full_text.borrow().clone();
                        if let Some(stream) = target.active_markdown.borrow().as_ref() {
                            render::render_markdown_streaming_into(
                                &stream.container,
                                &mut stream.blocks.clone(),
                                &stream.text,
                            );
                        }
                        agent_shapes =
                            sync_live_markup_overlay(&app, &active_markup_overlay, &full);
                    }
                    if !agent_shapes.is_empty() {
                        let count = agent_shapes.len();
                        let app_overlay = app.clone();
                        let slot_overlay = active_markup_overlay.clone();
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
                        if let Some(ref target) = *active_stream.borrow() {
                            target.turn_body.append(&pill);
                        }
                    }
                    if state.borrow().settings.sound_bell_on_done {
                        overlay::ring_completion_bell();
                    }
                    is_streaming.set(false);
                    update_send_btn_state();
                    scroll_to_bottom();
                }
            }
        }
    });
}

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
