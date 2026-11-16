//! Autocomplete suggest popup for `@` / `#` mentions and `/` commands.
//!
//! Uses a non-reentrant accept sequence (releasing any `RefCell` borrow before
//! mutating the `gtk4::TextBuffer`) because GTK emits `changed` synchronously.

use crate::icons;
use crate::mentions;
use crate::menu;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum MentionItemKind {
    Mention(mentions::MentionItem),
    Command { name: String, description: String },
}

pub struct MentionPopup {
    menu_host: Rc<menu::FloatingMenuHost>,
    composer_widget: gtk4::Widget,
    container: gtk4::Box,
    header_lbl: gtk4::Label,
    list: gtk4::ListBox,
    items: Vec<MentionItemKind>,
    pub selected: usize,
    replace_range: Option<(usize, usize)>,
    text_view: gtk4::TextView,
    workspace_index: Rc<RefCell<mentions::WorkspaceIndex>>,
    commands: Rc<RefCell<Vec<mentions::SlashCommand>>>,
    suppress_refresh: Rc<Cell<bool>>,
}

impl MentionPopup {
    pub fn attach(
        menu_host: Rc<menu::FloatingMenuHost>,
        composer_box: &gtk4::Box,
        text_view: &gtk4::TextView,
        workspace_index: Rc<RefCell<mentions::WorkspaceIndex>>,
        commands: Rc<RefCell<Vec<mentions::SlashCommand>>>,
    ) -> Rc<RefCell<Self>> {
        let pop_outer = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        pop_outer.set_hexpand(true);

        let header_lbl = gtk4::Label::builder()
            .label("SUGGESTIONS")
            .css_classes(["clean-popover-title"])
            .xalign(0.0)
            .build();
        pop_outer.append(&header_lbl);

        let list = gtk4::ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::Single);
        list.add_css_class("mention-list");
        list.set_hexpand(true);

        let scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .max_content_height(240)
            .propagate_natural_height(true)
            .child(&list)
            .build();
        pop_outer.append(&scroll);

        let popup = Rc::new(RefCell::new(Self {
            menu_host,
            composer_widget: composer_box.clone().upcast(),
            container: pop_outer,
            header_lbl,
            list: list.clone(),
            items: Vec::new(),
            selected: 0,
            replace_range: None,
            text_view: text_view.clone(),
            workspace_index,
            commands,
            suppress_refresh: Rc::new(Cell::new(false)),
        }));

        {
            let popup_row = popup.clone();
            list.connect_row_activated(move |_, row| {
                popup_row.borrow_mut().selected = row.index().max(0) as usize;
                accept_mention(&popup_row);
            });
        }

        {
            let popup_buf = popup.clone();
            text_view.buffer().connect_changed(move |_| {
                if popup_buf.borrow().suppress_refresh.get() {
                    return;
                }
                popup_buf.borrow_mut().refresh();
            });
        }

        popup
    }

    pub fn is_visible(&self) -> bool {
        self.menu_host.is_suggest_visible()
    }

    pub fn close(&mut self) {
        self.selected = 0;
        self.replace_range = None;
        self.menu_host.hide_suggest();
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as i32;
        self.selected = ((self.selected as i32 + delta).rem_euclid(len)) as usize;
        self.list
            .select_row(self.list.row_at_index(self.selected as i32).as_ref());
    }

    /// Extracts the replacement payload and closes the popup *without* mutating the buffer.
    fn take_pending_accept(&mut self) -> Option<PendingAccept> {
        let Some((start, end)) = self.replace_range else {
            self.close();
            return None;
        };
        let Some(item) = self.items.get(self.selected).cloned() else {
            self.close();
            return None;
        };
        let insert = match &item {
            MentionItemKind::Mention(m) => format!("{} ", m.insert),
            MentionItemKind::Command { name, .. } => format!("/{name} "),
        };
        let tv = self.text_view.clone();
        let suppress = self.suppress_refresh.clone();
        self.close();
        Some(PendingAccept {
            text_view: tv,
            start,
            end,
            insert,
            suppress,
        })
    }

    pub fn refresh(&mut self) {
        let buffer = self.text_view.buffer();
        let insert_mark = buffer.get_insert();
        let offset = buffer.iter_at_mark(&insert_mark).offset() as usize;
        let text_before = {
            let line_start = buffer
                .iter_at_offset(insert_mark_offset_line_start(&buffer, offset))
                .offset() as usize;
            buffer
                .text(
                    &buffer.iter_at_offset(line_start as i32),
                    &buffer.iter_at_offset(offset as i32),
                    false,
                )
                .to_string()
        };

        let Some(trigger) = mentions::extract_active_trigger(&text_before) else {
            self.close();
            return;
        };

        let query_chars = trigger.query.chars().count();
        let start = offset.saturating_sub(query_chars + 1);
        let end = offset;

        match trigger.kind {
            mentions::TriggerKind::Slash => {
                self.header_lbl.set_label("SLASH COMMANDS");
                let commands = self.commands.borrow();
                let matches = mentions::filter_commands(&commands, &trigger.query);
                self.items = matches
                    .into_iter()
                    .map(|c| MentionItemKind::Command {
                        name: c.name,
                        description: c.description,
                    })
                    .collect();
            }
            mentions::TriggerKind::At | mentions::TriggerKind::Hash => {
                self.header_lbl.set_label("CONTEXT & FILES");
                let index = self.workspace_index.borrow();
                let mut items = index.suggest(&trigger.query, 12);
                if trigger.kind == mentions::TriggerKind::Hash {
                    items.retain(|i| i.is_context);
                }
                self.items = items.into_iter().map(MentionItemKind::Mention).collect();
            }
        }

        if self.items.is_empty() {
            self.close();
            return;
        }

        self.replace_range = Some((start, end));
        self.selected = 0;
        self.render_rows();
        self.menu_host.show_suggest(&self.composer_widget, &self.container);
    }

    fn render_rows(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        for item in &self.items {
            let row = gtk4::ListBoxRow::new();
            row.add_css_class("mention-row");

            let outer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            outer.set_margin_top(4);
            outer.set_margin_bottom(4);
            outer.set_margin_start(8);
            outer.set_margin_end(8);

            let (icon_name, label, detail) = match item {
                MentionItemKind::Mention(m) => {
                    let ic = match m.insert.as_str() {
                        "@screen" => "codicon-device-camera-symbolic",
                        "@markup" => "codicon-edit-symbolic",
                        _ if m.is_context => "codicon-sparkle-symbolic",
                        _ => "codicon-file-code-symbolic",
                    };
                    (ic, m.label.clone(), m.detail.clone())
                }
                MentionItemKind::Command { name, description } => (
                    "codicon-symbol-namespace-symbolic",
                    format!("/{name}"),
                    description.clone(),
                ),
            };

            let ic_img = icons::image(icon_name, 14);
            ic_img.add_css_class("mention-row-icon");
            outer.append(&ic_img);

            outer.append(
                &gtk4::Label::builder()
                    .label(&label)
                    .xalign(0.0)
                    .css_classes(["mention-label"])
                    .build(),
            );

            if !detail.is_empty() {
                outer.append(
                    &gtk4::Label::builder()
                        .label(&detail)
                        .xalign(1.0)
                        .hexpand(true)
                        .ellipsize(gtk4::pango::EllipsizeMode::End)
                        .css_classes(["mention-detail"])
                        .build(),
                );
            }
            row.set_child(Some(&outer));
            self.list.append(&row);
        }

        self.list.select_row(self.list.row_at_index(0).as_ref());
    }
}

struct PendingAccept {
    text_view: gtk4::TextView,
    start: usize,
    end: usize,
    insert: String,
    suppress: Rc<Cell<bool>>,
}

/// Inserts the highlighted suggestion after releasing all `RefCell` borrows on `popup`.
pub fn accept_mention(popup: &Rc<RefCell<MentionPopup>>) -> bool {
    let Some(pending) = popup.borrow_mut().take_pending_accept() else {
        return false;
    };

    pending.suppress.set(true);
    let buffer = pending.text_view.buffer();
    let mut iter_start = buffer.iter_at_offset(pending.start as i32);
    let mut iter_end = buffer.iter_at_offset(pending.end as i32);
    buffer.delete(&mut iter_start, &mut iter_end);
    buffer.insert(&mut buffer.iter_at_offset(pending.start as i32), &pending.insert);
    pending.suppress.set(false);
    true
}

fn insert_mark_offset_line_start(buffer: &gtk4::TextBuffer, offset: usize) -> i32 {
    let mut iter = buffer.iter_at_offset(offset as i32);
    iter.set_line_offset(0);
    iter.offset()
}

/// Pure replacement helper tested independently of GTK widget trees.
#[cfg(test)]
pub fn apply_text_replacement(source: &str, start_char: usize, end_char: usize, insert: &str) -> String {
    let before: String = source.chars().take(start_char).collect();
    let after: String = source.chars().skip(end_char).collect();
    format!("{before}{insert}{after}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_text_replacement_handles_unicode_and_at_mentions() {
        assert_eq!(
            apply_text_replacement("Xin chào @scr", 9, 13, "@screen "),
            "Xin chào @screen "
        );
        assert_eq!(
            apply_text_replacement("Check @ma next", 6, 9, "@src/main.rs "),
            "Check @src/main.rs  next"
        );
    }
}
