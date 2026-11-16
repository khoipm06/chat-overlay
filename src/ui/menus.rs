//! Floating menus: Harness switcher, Screen Capture, Searchable Model Picker (`Ctrl+.`),
//! Agent Mode, Reasoning Depth, Session History, Settings, and New Chat confirmation.

use super::state::{
    model_display_name_with_list, mode_display_name, thinking_display_name, AppState,
    DeferredAction, AGENT_MODES, THINKING_LEVELS,
};
use super::widgets::set_text_view_content;
use crate::backend::{self, StreamEvent};
use crate::capture;
use crate::icons;
use crate::markup;
use crate::mentions;
use crate::menu::{FloatingMenuHost, MenuPlacement};
use crate::settings::{self, AppSettings, HarnessKind};
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;

pub struct MenuHandles {
    #[cfg(feature = "audit")]
    pub capture_box: gtk4::Box,
    pub session_stack: gtk4::Stack,
    pub prompts_list_box: gtk4::Box,
    pub prompt_empty_label: gtk4::Label,
    pub pending_prompt_delete: DeferredAction,
    #[cfg(feature = "audit")]
    pub more_stack: gtk4::Stack,
    pub execute_clear: Rc<dyn Fn()>,
    pub request_new_chat: Rc<dyn Fn()>,
    pub rebuild_catalogs: Rc<dyn Fn()>,
    pub open_model_picker: Rc<dyn Fn()>,
    pub capture_screen_fn: Rc<dyn Fn()>,
    pub capture_markup_fn: Rc<dyn Fn()>,
}

pub struct MenuBuildArgs {
    pub app: gtk4::Application,
    pub window: gtk4::ApplicationWindow,
    pub menu_host: Rc<FloatingMenuHost>,
    pub state: Rc<RefCell<AppState>>,
    pub scale_css_provider: gtk4::CssProvider,
    pub workspace_index: Rc<RefCell<mentions::WorkspaceIndex>>,
    pub stream_tx: mpsc::UnboundedSender<(u64, StreamEvent)>,
    pub text_view: gtk4::TextView,
    pub chips_row: gtk4::Box,
    pub messages_box: gtk4::Box,
    pub chat_stack: gtk4::Stack,
    pub active_stream: Rc<RefCell<Option<super::state::ActiveStreamTarget>>>,
    pub active_markup_overlay: Rc<RefCell<Option<markup::AgentMarkupOverlay>>>,
    pub is_streaming: Rc<std::cell::Cell<bool>>,
    pub update_send_btn_state: Rc<dyn Fn()>,
    pub btn_harness: gtk4::Button,
    pub lbl_harness: gtk4::Label,
    pub btn_attach: gtk4::Button,
    pub btn_capture: gtk4::Button,
    pub btn_mode_pill: gtk4::Button,
    pub lbl_mode_pill: gtk4::Label,
    pub btn_model_pill: gtk4::Button,
    pub lbl_model_pill: gtk4::Label,
    pub btn_think_pill: gtk4::Button,
    pub lbl_think_pill: gtk4::Label,
    pub btn_session_list: gtk4::Button,
    pub btn_more: gtk4::Button,
    pub btn_new_chat: gtk4::Button,
    pub session_title_label: gtk4::Label,
    pub initial_settings: AppSettings,
}

/// Case-insensitive substring filter matching both friendly name and raw model ID.
pub fn matches_model_query(id: &str, display_name: &str, query: &str) -> bool {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return true;
    }
    display_name.to_ascii_lowercase().contains(&q) || id.to_ascii_lowercase().contains(&q)
}

pub fn build_menus(args: MenuBuildArgs) -> MenuHandles {
    let MenuBuildArgs {
        app,
        window,
        menu_host,
        state,
        scale_css_provider,
        workspace_index,
        stream_tx,
        text_view,
        chips_row,
        messages_box,
        chat_stack,
        active_stream,
        active_markup_overlay,
        is_streaming,
        update_send_btn_state,
        btn_harness,
        lbl_harness,
        btn_attach,
        btn_capture,
        btn_mode_pill,
        lbl_mode_pill,
        btn_model_pill,
        lbl_model_pill,
        btn_think_pill,
        lbl_think_pill,
        btn_session_list,
        btn_more,
        btn_new_chat,
        session_title_label,
        initial_settings,
    } = args;

    // ── Model Picker (Searchable, `Ctrl+.` + Button) ────────────────────────
    let model_picker_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    model_picker_box.set_hexpand(true);

    model_picker_box.append(
        &gtk4::Label::builder()
            .label("SELECT MODEL (CTRL+.)")
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build(),
    );

    let model_search = gtk4::SearchEntry::builder()
        .placeholder_text("Search models…")
        .css_classes(["floating-search-entry"])
        .hexpand(true)
        .build();
    model_picker_box.append(&model_search);

    let model_scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .max_content_height(260)
        .propagate_natural_height(true)
        .build();
    let model_list = gtk4::ListBox::new();
    model_list.set_selection_mode(gtk4::SelectionMode::Single);
    model_list.add_css_class("model-picker-list");
    model_scroll.set_child(Some(&model_list));
    model_picker_box.append(&model_scroll);

    let filtered_models: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));

    let populate_model_rows: Rc<dyn Fn(&str)> = {
        let state_m = state.clone();
        let list_m = model_list.clone();
        let filt_m = filtered_models.clone();
        Rc::new(move |query: &str| {
            while let Some(child) = list_m.first_child() {
                list_m.remove(&child);
            }
            let (all_models, cur_model) = {
                let st = state_m.borrow();
                (st.available_models.clone(), st.settings.model.clone())
            };

            let mut matched = Vec::new();
            let mut active_idx: Option<i32> = None;

            for (id, name) in all_models {
                if !matches_model_query(&id, &name, query) {
                    continue;
                }
                let is_cur = id == cur_model;
                if is_cur {
                    active_idx = Some(matched.len() as i32);
                }
                let row = gtk4::ListBoxRow::new();
                row.add_css_class("model-picker-row");
                let rbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                rbox.set_margin_top(4);
                rbox.set_margin_bottom(4);
                rbox.set_margin_start(8);
                rbox.set_margin_end(8);
                let lbl = gtk4::Label::builder()
                    .label(&name)
                    .xalign(0.0)
                    .hexpand(true)
                    .ellipsize(gtk4::pango::EllipsizeMode::End)
                    .css_classes(["model-picker-name"])
                    .build();
                rbox.append(&lbl);
                if is_cur {
                    let chk = icons::image("codicon-check-symbolic", 12);
                    chk.add_css_class("mention-row-icon");
                    rbox.append(&chk);
                }
                row.set_child(Some(&rbox));
                list_m.append(&row);
                matched.push((id, name));
            }

            if matched.is_empty() {
                let empty_row = gtk4::ListBoxRow::new();
                empty_row.set_selectable(false);
                empty_row.set_activatable(false);
                empty_row.set_child(Some(
                    &gtk4::Label::builder()
                        .label("No matching models")
                        .css_classes(["clean-popover-item-desc"])
                        .margin_top(8)
                        .margin_bottom(8)
                        .xalign(0.5)
                        .build(),
                ));
                list_m.append(&empty_row);
            } else {
                let select_idx = active_idx.unwrap_or(0);
                list_m.select_row(list_m.row_at_index(select_idx).as_ref());
            }

            *filt_m.borrow_mut() = matched;
        })
    };

    let apply_selected_model_idx: Rc<dyn Fn(usize)> = {
        let mh_m = menu_host.clone();
        let lbl_mdl = lbl_model_pill.clone();
        let state_mdl = state.clone();
        let filt_m = filtered_models.clone();
        let tv_focus = text_view.clone();
        Rc::new(move |idx: usize| {
            let entry = filt_m.borrow().get(idx).cloned();
            let Some((model_key, _)) = entry else {
                return;
            };
            mh_m.close_menu();
            {
                let mut st = state_mdl.borrow_mut();
                let short_name = model_display_name_with_list(&model_key, &st.available_models);
                lbl_mdl.set_label(&short_name);
                let kind = st.settings.harness;
                st.settings.set_model_for_harness(kind, &model_key);
                st.harness.set_config_option("model", &model_key);
                let _ = st.settings.save();
            }
            tv_focus.grab_focus();
        })
    };

    {
        let pop_rows = populate_model_rows.clone();
        model_search.connect_search_changed(move |entry| {
            pop_rows(&entry.text());
        });
    }

    {
        let apply_fn = apply_selected_model_idx.clone();
        model_list.connect_row_activated(move |_, row| {
            let idx = row.index();
            if idx >= 0 {
                apply_fn(idx as usize);
            }
        });
    }

    {
        let list_nav = model_list.clone();
        let filt_nav = filtered_models.clone();
        let apply_nav = apply_selected_model_idx.clone();
        let mh_nav = menu_host.clone();
        let tv_nav = text_view.clone();
        let key_ctrl = gtk4::EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            match key {
                gdk::Key::Escape => {
                    mh_nav.close_menu();
                    tv_nav.grab_focus();
                    glib::Propagation::Stop
                }
                gdk::Key::Down | gdk::Key::Up => {
                    let count = filt_nav.borrow().len() as i32;
                    if count == 0 {
                        return glib::Propagation::Stop;
                    }
                    let cur = list_nav.selected_row().map(|r| r.index()).unwrap_or(0);
                    let delta = if key == gdk::Key::Down { 1 } else { -1 };
                    let next = (cur + delta).rem_euclid(count);
                    if let Some(row) = list_nav.row_at_index(next) {
                        list_nav.select_row(Some(&row));
                        row.grab_focus();
                    }
                    glib::Propagation::Stop
                }
                gdk::Key::Return | gdk::Key::KP_Enter => {
                    let idx = list_nav
                        .selected_row()
                        .map(|r| r.index().max(0) as usize)
                        .unwrap_or(0);
                    apply_nav(idx);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        model_search.add_controller(key_ctrl);
    }

    let open_model_picker: Rc<dyn Fn()> = {
        let mh_ref = menu_host.clone();
        let anchor_ref = btn_model_pill.clone();
        let box_ref = model_picker_box.clone();
        let search_ref = model_search.clone();
        let pop_rows = populate_model_rows.clone();
        Rc::new(move || {
            search_ref.set_text("");
            pop_rows("");
            mh_ref.toggle_menu("model", &anchor_ref, &box_ref, 280, MenuPlacement::AboveLeft);
            if mh_ref.is_menu_open() {
                search_ref.grab_focus();
                let s_idle = search_ref.clone();
                glib::idle_add_local_once(move || {
                    s_idle.grab_focus();
                });
            }
        })
    };

    {
        let open_fn = open_model_picker.clone();
        btn_model_pill.connect_clicked(move |_| open_fn());
    }

    // ── Mode Picker (dynamic across Pi / Antigravity / OpenCode) ────────────
    let mode_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    mode_box.set_hexpand(true);

    let rebuild_modes: Rc<dyn Fn()> = {
        let mode_box_r = mode_box.clone();
        let state_r = state.clone();
        let mh_r = menu_host.clone();
        let lbl_mode_r = lbl_mode_pill.clone();
        Rc::new(move || {
            while let Some(child) = mode_box_r.first_child() {
                mode_box_r.remove(&child);
            }
            mode_box_r.append(
                &gtk4::Label::builder()
                    .label("SELECT AGENT MODE")
                    .css_classes(["clean-popover-title"])
                    .xalign(0.0)
                    .build(),
            );

            let reported = state_r.borrow().available_modes.clone();
            let entries: Vec<(String, String, String)> = if !reported.is_empty() {
                reported
                    .into_iter()
                    .map(|(id, name)| {
                        let desc = AGENT_MODES
                            .iter()
                            .find(|(aid, _, _)| *aid == id)
                            .map(|(_, _, d)| (*d).to_string())
                            .unwrap_or_else(|| match id.as_str() {
                                "build" => "Default editing and tool execution mode".to_string(),
                                "plan" => "Read-only exploration & planning".to_string(),
                                _ => String::new(),
                            });
                        (id, name, desc)
                    })
                    .collect()
            } else {
                AGENT_MODES
                    .iter()
                    .map(|(id, name, desc)| ((*id).to_string(), (*name).to_string(), (*desc).to_string()))
                    .collect()
            };

            for (m_id, m_name, m_desc) in entries {
                let m_btn = gtk4::Button::builder()
                    .css_classes(["clean-popover-item"])
                    .halign(gtk4::Align::Fill)
                    .build();
                let inner_m = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
                inner_m.append(&gtk4::Label::builder().label(&m_name).xalign(0.0).build());
                if !m_desc.is_empty() {
                    inner_m.append(
                        &gtk4::Label::builder()
                            .label(&m_desc)
                            .css_classes(["clean-popover-item-desc"])
                            .xalign(0.0)
                            .build(),
                    );
                }
                m_btn.set_child(Some(&inner_m));

                let mh_close = mh_r.clone();
                let lbl_mode = lbl_mode_r.clone();
                let state_m = state_r.clone();
                m_btn.connect_clicked(move |_| {
                    mh_close.close_menu();
                    lbl_mode.set_label(&m_name);
                    let mut st = state_m.borrow_mut();
                    st.current_mode = m_id.clone();
                    st.settings.acp_mode = m_id.clone();
                    let auto = st.settings.auto_approve_permissions || m_id == "auto";
                    st.harness.set_auto_approve(auto);
                    st.harness.set_mode(&m_id);
                    let _ = st.settings.save();
                });
                mode_box_r.append(&m_btn);
            }
        })
    };
    rebuild_modes();

    {
        let mh_ref = menu_host.clone();
        let m_box_ref = mode_box.clone();
        let rebuild_fn = rebuild_modes.clone();
        btn_mode_pill.connect_clicked(move |btn| {
            rebuild_fn();
            mh_ref.toggle_menu("mode", btn, &m_box_ref, 240, MenuPlacement::AboveLeft);
        });
    }

    // ── Thinking Depth Picker ───────────────────────────────────────────────
    let think_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    think_box.set_hexpand(true);

    let rebuild_thinking: Rc<dyn Fn()> = {
        let think_box_r = think_box.clone();
        let state_r = state.clone();
        let mh_r = menu_host.clone();
        let lbl_th_r = lbl_think_pill.clone();
        let btn_th_r = btn_think_pill.clone();
        Rc::new(move || {
            while let Some(child) = think_box_r.first_child() {
                think_box_r.remove(&child);
            }
            think_box_r.append(
                &gtk4::Label::builder()
                    .label("REASONING DEPTH")
                    .css_classes(["clean-popover-title"])
                    .xalign(0.0)
                    .build(),
            );
            let (is_acp, levels) = {
                let st = state_r.borrow();
                (st.settings.harness.is_acp(), st.available_thinking_levels.clone())
            };
            // OpenCode ACP exposes no thinking_levels; Antigravity & Pi do.
            let items: Vec<(String, String)> = if !levels.is_empty() {
                levels
                    .into_iter()
                    .map(|id| {
                        let name = thinking_display_name(&id);
                        (id, name)
                    })
                    .collect()
            } else if is_acp {
                Vec::new()
            } else {
                THINKING_LEVELS
                    .iter()
                    .map(|(id, name)| ((*id).to_string(), (*name).to_string()))
                    .collect()
            };
            btn_th_r.set_visible(!items.is_empty());

            for (th_id, th_name) in items {
                let th_btn = gtk4::Button::builder()
                    .css_classes(["clean-popover-item"])
                    .halign(gtk4::Align::Fill)
                    .build();
                th_btn.set_child(Some(&gtk4::Label::builder().label(&th_name).xalign(0.0).build()));

                let mh_close = mh_r.clone();
                let lbl_th = lbl_th_r.clone();
                let state_th = state_r.clone();
                th_btn.connect_clicked(move |_| {
                    mh_close.close_menu();
                    lbl_th.set_label(&th_name);
                    let mut st = state_th.borrow_mut();
                    st.settings.thinking_level = th_id.clone();
                    st.harness.set_config_option("thought_level", &th_id);
                    let _ = st.settings.save();
                });
                think_box_r.append(&th_btn);
            }
        })
    };
    rebuild_thinking();

    {
        let mh_ref = menu_host.clone();
        let th_box_ref = think_box.clone();
        btn_think_pill.connect_clicked(move |btn| {
            mh_ref.toggle_menu("think", btn, &th_box_ref, 240, MenuPlacement::AboveLeft);
        });
    }

    let rebuild_catalogs: Rc<dyn Fn()> = {
        let rm = rebuild_modes.clone();
        let rt = rebuild_thinking.clone();
        let rmod = populate_model_rows.clone();
        let ms = model_search.clone();
        Rc::new(move || {
            rm();
            rt();
            rmod(&ms.text());
        })
    };

    // ── Harness Switcher (Pi JSON / Antigravity ACP / OpenCode ACP) ──────────
    {
        let harness_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        harness_box.set_hexpand(true);
        harness_box.append(
            &gtk4::Label::builder()
                .label("AGENT HARNESS")
                .css_classes(["clean-popover-title"])
                .xalign(0.0)
                .build(),
        );

        for kind in HarnessKind::all() {
            let (label, desc) = match kind {
                HarnessKind::PiJson => ("Pi (JSON)", "pi -p --mode json, local session"),
                HarnessKind::AgyAcp => ("Antigravity (ACP)", "agy acp over stdio, tool permissions"),
                HarnessKind::OpenCodeAcp => ("OpenCode (ACP)", "opencode acp over stdio, multi-provider"),
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
            let lbl_mdl_h = lbl_model_pill.clone();
            let lbl_mode_h = lbl_mode_pill.clone();
            let state_h = state.clone();
            let tx_h = stream_tx.clone();
            let rb_h = rebuild_catalogs.clone();
            item.connect_clicked(move |_| {
                mh_close.close_menu();
                lbl_h.set_label(kind.label());

                {
                    let mut st = state_h.borrow_mut();
                    if st.settings.harness == kind {
                        return;
                    }
                    st.settings.harness = kind;
                    let remembered = st.settings.model_for_harness(kind);
                    st.settings.model = if kind == HarnessKind::PiJson {
                        remembered.unwrap_or_else(|| "default".to_string())
                    } else {
                        remembered.unwrap_or_default()
                    };
                    st.available_modes.clear();
                    st.available_thinking_levels.clear();
                    let _ = st.settings.save();
                    let settings_snapshot = st.settings.clone();
                    st.harness.set_kind(kind, &settings_snapshot);

                    if kind == HarnessKind::PiJson {
                        drop(st);
                        let models = backend::query_pi_models_sync();
                        let (cur_model, cur_mode) = {
                            let mut st = state_h.borrow_mut();
                            st.available_models = models.clone();
                            (st.settings.model.clone(), st.current_mode.clone())
                        };
                        lbl_mdl_h.set_label(&model_display_name_with_list(&cur_model, &models));
                        lbl_mode_h.set_label(&mode_display_name(&cur_mode));
                        rb_h();
                    } else {
                        st.available_models.clear();
                        lbl_mdl_h.set_label("Connecting…");
                        st.harness.warmup(tx_h.clone());
                        drop(st);
                        rb_h();
                    }
                }
            });
            harness_box.append(&item);
        }

        let mh_ref = menu_host.clone();
        btn_harness.connect_clicked(move |btn| {
            mh_ref.toggle_menu("harness", btn, &harness_box, 240, MenuPlacement::BelowLeft);
        });
    }

    // ── Attach File Button ──────────────────────────────────────────────────
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
            let st_cb = state_attach.clone();
            let chips_cb = chips_ref.clone();
            let upd_cb = update_btn_attach.clone();
            chooser.connect_response(move |dialog, resp| {
                if resp != gtk4::ResponseType::Accept {
                    return;
                }
                let Some(file) = dialog.file() else { return };
                let Some(path) = file.path() else { return };
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
                st_cb.borrow_mut().attached_files.push((path_str.clone(), content));
                super::widgets::add_context_chip(
                    &chips_cb,
                    &display_name,
                    &path_str,
                    st_cb.clone(),
                    Some(upd_cb.clone()),
                );
            });
            chooser.show();
        });
    }

    // ── Screen Capture Menu & Helpers ───────────────────────────────────────
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
            super::widgets::add_context_chip(
                &chips_cap,
                &display_name,
                &path_str,
                state_cap.clone(),
                Some(upd_cap.clone()),
            );
        })
    };

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

    let capture_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    capture_box.set_hexpand(true);
    capture_box.append(
        &gtk4::Label::builder()
            .label("SCREEN CONTEXT")
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build(),
    );

    for (mode, m_label, m_desc) in [
        (capture::CaptureMode::FullScreen, "Full screen", "Capture the whole desktop"),
        (capture::CaptureMode::Region, "Select region", "Click & drag the area to attach"),
        (capture::CaptureMode::ActiveWindow, "Active window", "Capture the focused application"),
    ] {
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

        let mh_close = menu_host.clone();
        let attached = attach_capture.clone();
        let screen_fn = capture_screen_fn.clone();
        let win_cap = window.clone();
        cap_btn.connect_clicked(move |_| {
            mh_close.close_menu();
            match mode {
                capture::CaptureMode::FullScreen => screen_fn(),
                _ => {
                    let attached = attached.clone();
                    capture::capture_mode_hiding_window(&win_cap, mode, move |res| match res {
                        Ok(path) => attached(&path),
                        Err(err) => eprintln!("Capture failed: {err}"),
                    });
                }
            }
        });
        capture_box.append(&cap_btn);
    }

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
            mh_ref.toggle_menu("capture", btn, &cap_box_ref, 240, MenuPlacement::AboveRight);
        });
    }

    // ── Session History Stack ───────────────────────────────────────────────
    let session_stack = gtk4::Stack::new();
    session_stack.set_vhomogeneous(false);
    session_stack.set_hexpand(true);

    let session_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    session_box.set_hexpand(true);
    session_box.append(
        &gtk4::Label::builder()
            .label("SESSION HISTORY")
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build(),
    );

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

    let pending_prompt_delete: DeferredAction = Rc::new(RefCell::new(None));
    {
        let del_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        del_box.append(
            &gtk4::Label::builder()
                .label("Delete this message?")
                .css_classes(["confirm-dialog-title"])
                .xalign(0.0)
                .build(),
        );
        del_box.append(
            &gtk4::Label::builder()
                .label("Remove this turn and its response from history.")
                .css_classes(["confirm-dialog-desc"])
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .xalign(0.0)
                .build(),
        );
        let btn_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        btn_row.set_halign(gtk4::Align::End);
        let btn_cancel_item = gtk4::Button::builder()
            .label("Cancel")
            .css_classes(["btn-dialog-cancel"])
            .build();
        let btn_confirm_item = gtk4::Button::builder()
            .label("Delete")
            .css_classes(["btn-dialog-destructive"])
            .build();
        btn_row.append(&btn_cancel_item);
        btn_row.append(&btn_confirm_item);
        del_box.append(&btn_row);

        let ss_c = session_stack.clone();
        let p_c = pending_prompt_delete.clone();
        btn_cancel_item.connect_clicked(move |_| {
            *p_c.borrow_mut() = None;
            ss_c.set_visible_child_name("list");
        });

        let ss_ok = session_stack.clone();
        let p_ok = pending_prompt_delete.clone();
        btn_confirm_item.connect_clicked(move |_| {
            if let Some(action) = p_ok.borrow_mut().take() {
                action();
            }
            ss_ok.set_visible_child_name("list");
        });
        session_stack.add_named(&del_box, Some("confirm_delete"));
    }

    {
        let mh_ref = menu_host.clone();
        let ss_ref = session_stack.clone();
        btn_session_list.connect_clicked(move |btn| {
            ss_ref.set_visible_child_name("list");
            mh_ref.toggle_menu("session", btn, &ss_ref, 260, MenuPlacement::BelowRight);
        });
    }

    // ── Settings Menu Stack ─────────────────────────────────────────────────
    let more_stack = gtk4::Stack::new();
    more_stack.set_vhomogeneous(false);
    more_stack.set_hexpand(true);

    let more_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    more_box.set_hexpand(true);
    more_box.append(
        &gtk4::Label::builder()
            .label("SESSION SETTINGS")
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build(),
    );

    let btn_row_scale = gtk4::Button::builder()
        .css_classes(["clean-settings-row"])
        .halign(gtk4::Align::Fill)
        .build();
    let box_rs = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    box_rs.append(
        &gtk4::Label::builder()
            .label("UI Scale")
            .css_classes(["clean-popover-item-desc"])
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );
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
    box_rs.append(&lbl_rs_val);
    btn_row_scale.set_child(Some(&box_rs));
    more_box.append(&btn_row_scale);

    let prs_box = build_subpage_box(&more_stack, "UI SCALE PRESET");
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
        btn_row_scale.connect_clicked(move |_| ms_go.set_visible_child_name("scale"));
    }

    let btn_row_font = gtk4::Button::builder()
        .css_classes(["clean-settings-row"])
        .halign(gtk4::Align::Fill)
        .build();
    let box_rf = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    box_rf.append(
        &gtk4::Label::builder()
            .label("Font Size")
            .css_classes(["clean-popover-item-desc"])
            .xalign(0.0)
            .hexpand(true)
            .build(),
    );
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
    box_rf.append(&lbl_rf_val);
    btn_row_font.set_child(Some(&box_rf));
    more_box.append(&btn_row_font);

    let prf_box = build_subpage_box(&more_stack, "BASE FONT SIZE");
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
        btn_row_font.connect_clicked(move |_| ms_go.set_visible_child_name("font"));
    }

    let pin_check = gtk4::CheckButton::builder()
        .label("Pin sidebar on blur")
        .active(!initial_settings.auto_hide_on_blur)
        .css_classes(["clean-popover-check"])
        .build();
    more_box.append(&pin_check);
    {
        let state_pin = state.clone();
        pin_check.connect_toggled(move |cb| {
            let mut st = state_pin.borrow_mut();
            st.settings.auto_hide_on_blur = !cb.is_active();
            let _ = st.settings.save();
        });
    }

    let ephemeral_check = gtk4::CheckButton::builder()
        .label("Ephemeral (Clear on hide)")
        .active(initial_settings.ephemeral)
        .css_classes(["clean-popover-check"])
        .build();
    more_box.append(&ephemeral_check);
    {
        let state_eph = state.clone();
        ephemeral_check.connect_toggled(move |cb| {
            let mut st = state_eph.borrow_mut();
            st.settings.ephemeral = cb.is_active();
            let _ = st.settings.save();
        });
    }

    more_box.append(&gtk4::Separator::builder().css_classes(["menu-sep"]).build());

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
            mh_ref.toggle_menu("more", btn, &ms_ref, 260, MenuPlacement::BelowRight);
        });
    }

    // ── Execute Clear + Clear Confirmation + New Chat Discard Confirm ───────
    let execute_clear: Rc<dyn Fn()> = {
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
            while let Some(child) = prompts_list_box.first_child() {
                if child != prompt_empty_label {
                    prompts_list_box.remove(&child);
                } else {
                    break;
                }
            }
            let mut cur = prompt_empty_label.next_sibling();
            while let Some(sib) = cur {
                cur = sib.next_sibling();
                prompts_list_box.remove(&sib);
            }
            prompt_empty_label.set_visible(true);
            state_clear.borrow_mut().prompts.clear();
            session_title_clear.set_label("");
            chat_stack.set_visible_child_name("welcome");
        })
    };

    {
        let confirm_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        confirm_box.set_hexpand(true);
        confirm_box.append(
            &gtk4::Label::builder()
                .label("Clear Conversation?")
                .css_classes(["confirm-dialog-title"])
                .xalign(0.0)
                .build(),
        );
        confirm_box.append(
            &gtk4::Label::builder()
                .label("This will erase all active messages and session history.")
                .css_classes(["confirm-dialog-desc"])
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .xalign(0.0)
                .build(),
        );
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
        btn_cancel.connect_clicked(move |_| ms_cancel.set_visible_child_name("main"));

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

    let new_chat_box = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    new_chat_box.set_hexpand(true);
    new_chat_box.append(
        &gtk4::Label::builder()
            .label("Discard Unsent Draft?")
            .css_classes(["confirm-dialog-title"])
            .xalign(0.0)
            .build(),
    );
    new_chat_box.append(
        &gtk4::Label::builder()
            .label("You have unsent text in the composer. Start a new session and discard it?")
            .css_classes(["confirm-dialog-desc"])
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::WordChar)
            .xalign(0.0)
            .build(),
    );
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
        btn_nc_cancel.connect_clicked(move |_| mh_close.close_menu());
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

    let request_new_chat: Rc<dyn Fn()> = {
        let mh_ref = menu_host.clone();
        let nc_anchor = btn_new_chat.clone();
        let nc_box_ref = new_chat_box.clone();
        let do_clear = execute_clear.clone();
        let tv_ref = text_view.clone();
        Rc::new(move || {
            let draft = super::widgets::get_text_view_content(&tv_ref);
            if !draft.is_empty() {
                mh_ref.open_menu("new_chat", &nc_anchor, &nc_box_ref, 260, MenuPlacement::BelowRight);
            } else {
                do_clear();
                tv_ref.grab_focus();
            }
        })
    };

    {
        let req_nc = request_new_chat.clone();
        btn_new_chat.connect_clicked(move |_| req_nc());
    }

    // Also keep workspace index updated inside menus owner
    let _ = workspace_index;

    MenuHandles {
        #[cfg(feature = "audit")]
        capture_box,
        session_stack,
        prompts_list_box,
        prompt_empty_label,
        pending_prompt_delete,
        #[cfg(feature = "audit")]
        more_stack,
        execute_clear,
        request_new_chat,
        rebuild_catalogs,
        open_model_picker,
        capture_screen_fn,
        capture_markup_fn,
    }
}

fn build_subpage_box(stack: &gtk4::Stack, title: &str) -> gtk4::Box {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    page.set_hexpand(true);
    let back_btn = gtk4::Button::builder()
        .label("← Session Settings")
        .css_classes(["floating-menu-back"])
        .halign(gtk4::Align::Start)
        .build();
    let stack_ref = stack.clone();
    back_btn.connect_clicked(move |_| stack_ref.set_visible_child_name("main"));
    page.append(&back_btn);
    page.append(
        &gtk4::Label::builder()
            .label(title)
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build(),
    );
    page
}

#[cfg(test)]
mod tests {
    use super::matches_model_query;

    #[test]
    fn test_matches_model_query_case_insensitive_on_id_and_name() {
        assert!(matches_model_query("opencode/deepseek-v4.1-flash-free", "DeepSeek V4.1", ""));
        assert!(matches_model_query("opencode/deepseek-v4.1-flash-free", "DeepSeek V4.1", "DEEP"));
        assert!(matches_model_query("gemini-4-argon-high", "Gemini 4 Argon (High)", "argon"));
        assert!(!matches_model_query("gemini-4-argon-high", "Gemini 4 Argon (High)", "claude"));
    }
}
