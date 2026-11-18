use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

/// Anchors the chat panel to `edge` (`left` or `right`).
pub fn init_layer_shell(window: &gtk4::ApplicationWindow, use_layer_shell: bool, edge: &str) {
    if use_layer_shell && gtk4_layer_shell::is_supported() {
        window.init_layer_shell();
        window.set_namespace("pi-overlay-chat");
        window.set_layer(Layer::Overlay);
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

        let anchor_left = matches!(edge, "left");
        window.set_anchor(Edge::Left, anchor_left);
        window.set_anchor(Edge::Right, !anchor_left);
        window.set_visible(false);
    } else {
        window.set_decorated(false);
        window.set_visible(false);
    }
}

/// Fullscreen transparent surface for the annotation overlays.
///
/// No keyboard focus and no exclusive zone: the markup layer must never steal
/// input from the applications underneath it.
pub fn init_markup_overlay(window: &gtk4::ApplicationWindow) {
    if gtk4_layer_shell::is_supported() {
        window.init_layer_shell();
        window.set_namespace("pi-overlay-markup");
        window.set_layer(Layer::Overlay);
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
    } else {
        window.set_decorated(false);
    }
}

/// Maps the surface with keyboard focus on demand using persistent margins.
pub fn expand_overlay(
    window: &gtk4::ApplicationWindow,
    top_margin: i32,
    bottom_margin: i32,
    right_margin: i32,
    edge: &str,
) {
    if window.is_layer_window() {
        let anchor_left = matches!(edge, "left");
        window.set_anchor(Edge::Left, anchor_left);
        window.set_anchor(Edge::Right, !anchor_left);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_margin(Edge::Right, right_margin.max(0));
        window.set_margin(Edge::Top, top_margin.max(0));
        window.set_margin(Edge::Bottom, bottom_margin.max(0));
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    }
    window.set_visible(true);
    window.present();
}

/// Unmap the surface completely: 100% click-through until expanded again.
pub fn hide_overlay(window: &gtk4::ApplicationWindow) {
    if window.is_layer_window() {
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    }
    window.set_visible(false);
}

/// Emit an audible/visual bell notification on prompt completion.
pub fn ring_completion_bell() {
    if let Some(display) = gdk::Display::default() {
        display.beep();
    }
}
