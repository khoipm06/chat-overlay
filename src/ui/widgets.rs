//! Reusable GTK4 widgets matching VS Code Chat presentation and permission cards.

use super::state::AppState;
use crate::backend;
use crate::capture;
use crate::icons;
use crate::markup;
use crate::render;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Lightweight VS Code-style collapsible disclosure with chevron icon and vertical rail.
#[derive(Clone)]
pub struct Disclosure {
    pub root: gtk4::Box,
    pub chevron: gtk4::Image,
    pub label: gtk4::Label,
    pub revealer: gtk4::Revealer,
    pub rail: gtk4::Box,
    pub expanded: Rc<Cell<bool>>,
}

impl Disclosure {
    pub fn new(title: &str, expanded: bool) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        root.add_css_class("disclosure-box");

        let btn = gtk4::Button::builder()
            .css_classes(["disclosure-btn"])
            .halign(gtk4::Align::Start)
            .build();
        let hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let chevron = icons::image(
            if expanded {
                "codicon-chevron-down-symbolic"
            } else {
                "codicon-chevron-right-symbolic"
            },
            12,
        );
        let label = gtk4::Label::builder()
            .label(title)
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        hdr.append(&chevron);
        hdr.append(&label);
        btn.set_child(Some(&hdr));

        let revealer = gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::SlideDown)
            .transition_duration(120)
            .reveal_child(expanded)
            .build();
        let rail = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        rail.add_css_class("disclosure-rail");
        revealer.set_child(Some(&rail));

        let expanded_cell = Rc::new(Cell::new(expanded));
        {
            let exp_c = expanded_cell.clone();
            let rev_c = revealer.clone();
            let chev_c = chevron.clone();
            btn.connect_clicked(move |_| {
                let next = !exp_c.get();
                exp_c.set(next);
                rev_c.set_reveal_child(next);
                chev_c.set_icon_name(Some(if next {
                    "codicon-chevron-down-symbolic"
                } else {
                    "codicon-chevron-right-symbolic"
                }));
            });
        }

        root.append(&btn);
        root.append(&revealer);

        Self {
            root,
            chevron,
            label,
            revealer,
            rail,
            expanded: expanded_cell,
        }
    }

    pub fn set_title(&self, title: &str) {
        self.label.set_text(title);
    }

    pub fn set_expanded(&self, expanded: bool) {
        self.expanded.set(expanded);
        self.revealer.set_reveal_child(expanded);
        self.chevron.set_icon_name(Some(if expanded {
            "codicon-chevron-down-symbolic"
        } else {
            "codicon-chevron-right-symbolic"
        }));
    }
}

/// Live markdown stream cache keeping completed block widgets alive between deltas.
pub struct MarkdownStream {
    pub container: gtk4::Box,
    pub text: String,
    pub blocks: Vec<render::MarkdownBlock>,
}

impl MarkdownStream {
    pub fn new(container: gtk4::Box) -> Self {
        Self {
            container,
            text: String::new(),
            blocks: Vec::new(),
        }
    }

    pub fn push(&mut self, delta: &str) {
        self.text.push_str(delta);
        let visible = markup::strip_agent_markup_tags_preserving_open(&self.text);
        render::render_markdown_streaming_into(&self.container, &mut self.blocks, &visible);
    }
}

pub fn get_text_view_content(tv: &gtk4::TextView) -> String {
    let buffer = tv.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, false).trim().to_string()
}

pub fn set_text_view_content(tv: &gtk4::TextView, text: &str) {
    tv.buffer().set_text(text);
}

/// Builds a compact flat picker button displaying `label` + Codicon chevron-down.
pub fn make_picker_button(
    initial_label: &str,
    css_class: &str,
    tooltip: &str,
) -> (gtk4::Button, gtk4::Label) {
    let btn = gtk4::Button::builder()
        .css_classes([css_class])
        .tooltip_text(tooltip)
        .valign(gtk4::Align::Center)
        .build();
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    row.set_valign(gtk4::Align::Center);
    let lbl = gtk4::Label::builder()
        .label(initial_label)
        .valign(gtk4::Align::Center)
        .yalign(0.5)
        .ellipsize(gtk4::pango::EllipsizeMode::End)
        .build();
    let chev = icons::image("codicon-chevron-down-symbolic", 11);
    chev.set_opacity(0.75);
    chev.set_valign(gtk4::Align::Center);
    row.append(&lbl);
    row.append(&chev);
    btn.set_child(Some(&row));
    (btn, lbl)
}

pub fn add_context_chip(
    chips_box: &gtk4::Box,
    display_name: &str,
    full_path: &str,
    state: Rc<RefCell<AppState>>,
    on_change: Option<Rc<dyn Fn()>>,
) {
    let chip = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    chip.add_css_class("composer-chip");
    let is_img = capture::is_image_path(full_path);
    if is_img {
        chip.add_css_class("image-chip");
    }

    let chip_icon = icons::image(
        if is_img {
            "codicon-device-camera-symbolic"
        } else {
            "codicon-file-code-symbolic"
        },
        11,
    );
    chip_icon.add_css_class("composer-chip-icon");

    let chip_text = gtk4::Label::builder()
        .label(display_name)
        .css_classes(["composer-chip-text"])
        .ellipsize(gtk4::pango::EllipsizeMode::Middle)
        .max_width_chars(18)
        .build();

    let chip_del = icons::icon_button(
        "codicon-close-symbolic",
        10,
        "Remove context chip",
        &["composer-chip-del"],
    );

    chip.append(&chip_del);
    chip.append(&chip_icon);
    chip.append(&chip_text);

    let chips_box_clone = chips_box.clone();
    let chip_clone = chip.clone();
    let on_change_clone = on_change.clone();
    let path_clone = full_path.to_string();
    chip_del.connect_clicked(move |_| {
        chips_box_clone.remove(&chip_clone);
        state
            .borrow_mut()
            .attached_files
            .retain(|(p, _)| p != &path_clone);
        if let Some(ref cb) = on_change_clone {
            cb();
        }
    });

    chips_box.append(&chip);
    if let Some(ref cb) = on_change {
        cb();
    }
}

/// Inline approval card shown when an ACP agent requests tool confirmation.
pub fn build_permission_card(
    req: &backend::PermissionRequest,
    state: &Rc<RefCell<AppState>>,
) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    card.add_css_class("chat-permission-card");

    let hdr = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let icon = icons::image("codicon-shield-symbolic", 14);
    icon.add_css_class("permission-icon");
    hdr.append(&icon);

    let title = gtk4::Label::builder()
        .label(format!("Allow {}?", req.title))
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk4::pango::WrapMode::WordChar)
        .hexpand(true)
        .css_classes(["permission-title"])
        .build();
    hdr.append(&title);
    card.append(&hdr);

    if !req.detail.trim().is_empty() {
        let detail = gtk4::Label::builder()
            .label(&req.detail)
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::Char)
            .selectable(true)
            .hexpand(true)
            .css_classes(["permission-detail"])
            .build();
        card.append(&detail);
    }

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    actions.add_css_class("permission-actions");

    let status_lbl = gtk4::Label::builder()
        .css_classes(["permission-detail"])
        .xalign(0.0)
        .visible(false)
        .build();
    card.append(&actions);
    card.append(&status_lbl);

    let request_id = req.request_id;
    for (option_id, label) in &req.options {
        let btn = gtk4::Button::builder()
            .label(label)
            .css_classes(["permission-btn"])
            .build();
        if option_id == "allow_once" || option_id == "allow" {
            btn.add_css_class("permission-btn-primary");
        }

        let option = option_id.clone();
        let label_text = label.clone();
        let actions_ref = actions.clone();
        let status_ref = status_lbl.clone();
        let state_clone = state.clone();
        btn.connect_clicked(move |_| {
            {
                let mut st = state_clone.borrow_mut();
                st.harness.respond_permission(request_id, &option);
                if option == "allow_always" {
                    st.harness.set_auto_approve(true);
                }
            }
            actions_ref.set_visible(false);
            status_ref.set_text(&format!("Selected: {label_text}"));
            status_ref.set_visible(true);
        });
        actions.append(&btn);
    }

    card
}
