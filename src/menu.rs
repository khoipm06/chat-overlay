use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Where a floating menu prefers to open relative to its anchor button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuPlacement {
    BelowLeft,
    BelowRight,
    AboveLeft,
    AboveRight,
}

/// Non-grabbing in-drawer floating menu host backed by a `gtk4::Overlay`.
///
/// Using an in-panel overlay instead of Wayland `xdg_popup` (`GtkPopover`)
/// prevents compositor popup grabs on `gtk4-layer-shell` surfaces from trapping
/// pointer or keyboard input, supports clean click-twice toggling, and gives
/// deterministic width/edge alignment relative to trigger buttons.
pub struct FloatingMenuHost {
    pub overlay: gtk4::Overlay,
    drawer: gtk4::Box,
    backdrop: gtk4::Box,
    active_id: RefCell<Option<String>>,
    active_frame: RefCell<Option<gtk4::Box>>,
    active_content: RefCell<Option<gtk4::Widget>>,
    suggest_frame: RefCell<Option<gtk4::Box>>,
}

impl FloatingMenuHost {
    pub fn new(drawer: &gtk4::Box) -> Rc<Self> {
        let overlay = gtk4::Overlay::new();
        overlay.set_vexpand(true);
        overlay.set_hexpand(false);
        overlay.set_child(Some(drawer));

        let backdrop = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        backdrop.add_css_class("floating-backdrop");
        backdrop.set_hexpand(true);
        backdrop.set_vexpand(true);
        backdrop.set_visible(false);
        overlay.add_overlay(&backdrop);

        let host = Rc::new(Self {
            overlay,
            drawer: drawer.clone(),
            backdrop: backdrop.clone(),
            active_id: RefCell::new(None),
            active_frame: RefCell::new(None),
            active_content: RefCell::new(None),
            suggest_frame: RefCell::new(None),
        });

        let click = gtk4::GestureClick::new();
        let host_cb = host.clone();
        click.connect_pressed(move |_, _, _, _| {
            host_cb.close_menu();
        });
        backdrop.add_controller(click);

        host
    }

    pub fn is_menu_open(&self) -> bool {
        self.active_id.borrow().is_some()
    }

    /// Closes any currently open floating menu and restores backdrop pass-through.
    pub fn close_menu(&self) {
        if let Some(frame) = self.active_frame.borrow_mut().take() {
            if let Some(content) = self.active_content.borrow_mut().take() {
                frame.remove(&content);
            }
            self.overlay.remove_overlay(&frame);
        }
        *self.active_id.borrow_mut() = None;
        self.backdrop.set_visible(false);
    }

    /// Toggles `id`: closes if already open, otherwise replaces any open menu
    /// and positions `content` cleanly inside `self.drawer`.
    pub fn toggle_menu(
        &self,
        id: &str,
        anchor: &impl IsA<gtk4::Widget>,
        content: &impl IsA<gtk4::Widget>,
        min_width: i32,
        placement: MenuPlacement,
    ) {
        if self.active_id.borrow().as_deref() == Some(id) {
            self.close_menu();
            return;
        }
        self.open_menu(id, anchor, content, min_width, placement);
    }

    pub fn open_menu(
        &self,
        id: &str,
        anchor: &impl IsA<gtk4::Widget>,
        content: &impl IsA<gtk4::Widget>,
        min_width: i32,
        placement: MenuPlacement,
    ) {
        self.close_menu();

        let content_widget: gtk4::Widget = content.clone().upcast();
        if let Some(prev_parent) = content_widget.parent() {
            if let Ok(box_parent) = prev_parent.downcast::<gtk4::Box>() {
                box_parent.remove(&content_widget);
            }
        }

        let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        frame.add_css_class("floating-menu");
        frame.append(&content_widget);

        let drawer_w = self.drawer.width().max(420);
        let drawer_h = self.drawer.height().max(700);
        let anchor_bounds = anchor.as_ref().compute_bounds(&self.drawer);
        let (ax, ay, aw, ah) = anchor_bounds
            .map(|b| {
                (
                    b.x() as i32,
                    b.y() as i32,
                    b.width() as i32,
                    b.height() as i32,
                )
            })
            .unwrap_or((12, drawer_h - 140, 120, 26));

        let target_w = min_width.max(aw).min(drawer_w - 16);
        frame.set_width_request(target_w);
        let (_, nat_size) = frame.preferred_size();
        let menu_w = nat_size.width().max(target_w).min(drawer_w - 16);
        let menu_h = nat_size.height().clamp(36, 420);

        let right_align = matches!(
            placement,
            MenuPlacement::BelowRight | MenuPlacement::AboveRight
        );
        let above = matches!(
            placement,
            MenuPlacement::AboveLeft | MenuPlacement::AboveRight
        );

        let raw_x = if right_align { ax + aw - menu_w } else { ax };
        let x = raw_x.clamp(8, (drawer_w - menu_w - 8).max(8));

        let raw_y = if above { ay - menu_h - 6 } else { ay + ah + 6 };
        let y = raw_y.clamp(8, (drawer_h - menu_h - 8).max(8));

        frame.set_halign(gtk4::Align::Start);
        frame.set_valign(gtk4::Align::Start);
        frame.set_margin_start(x);
        frame.set_margin_top(y);

        self.backdrop.set_visible(true);
        self.overlay.add_overlay(&frame);

        *self.active_id.borrow_mut() = Some(id.to_string());
        *self.active_frame.borrow_mut() = Some(frame);
        *self.active_content.borrow_mut() = Some(content_widget);
    }

    /// Shows or hides the non-blocking `@` / `#` / `/` autocomplete panel flush
    /// above `composer_widget`. Does not activate `backdrop`, keeping editor typing live.
    pub fn show_suggest(
        &self,
        composer_widget: &impl IsA<gtk4::Widget>,
        content: &impl IsA<gtk4::Widget>,
    ) {
        let drawer_h = self.drawer.height();
        let bottom_margin = if drawer_h > 0 {
            composer_widget
                .as_ref()
                .compute_bounds(&self.drawer)
                .map(|b| (drawer_h - b.y() as i32 + 4).max(96))
                .unwrap_or_else(|| composer_widget.as_ref().height().max(96) + 16)
        } else {
            composer_widget.as_ref().height().max(96) + 16
        };

        if self.suggest_frame.borrow().is_none() {
            let content_widget: gtk4::Widget = content.clone().upcast();
            let frame = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            frame.add_css_class("floating-menu");
            frame.set_halign(gtk4::Align::Fill);
            frame.set_valign(gtk4::Align::End);
            frame.set_margin_start(12);
            frame.set_margin_end(12);
            frame.set_margin_bottom(bottom_margin);
            frame.append(&content_widget);

            self.overlay.add_overlay(&frame);
            *self.suggest_frame.borrow_mut() = Some(frame);
        } else if let Some(ref frame) = *self.suggest_frame.borrow() {
            frame.set_margin_bottom(bottom_margin);
            frame.set_visible(true);
        }
    }

    pub fn hide_suggest(&self) {
        if let Some(ref frame) = *self.suggest_frame.borrow() {
            frame.set_visible(false);
        }
    }

    pub fn is_suggest_visible(&self) -> bool {
        self.suggest_frame
            .borrow()
            .as_ref()
            .is_some_and(|f| f.is_visible())
    }
}
