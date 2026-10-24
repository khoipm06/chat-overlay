use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

pub fn init_layer_shell(window: &gtk4::ApplicationWindow) {
    if gtk4_layer_shell::is_supported() {
        window.init_layer_shell();
        window.set_namespace("pi-ephemeral-overlay");
        window.set_layer(Layer::Top);
        window.set_exclusive_zone(0);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        window.set_visible(false);
    } else {
        window.set_decorated(false);
        window.set_visible(false);
    }
}

/// Map the surface with keyboard focus on demand using persistent margins.
pub fn expand_overlay(
    window: &gtk4::ApplicationWindow,
    top_margin: i32,
    bottom_margin: i32,
    right_margin: i32,
) {
    if window.is_layer_window() {
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_margin(Edge::Right, right_margin.max(0));
        window.set_margin(Edge::Top, top_margin.max(8));
        window.set_margin(Edge::Bottom, bottom_margin.max(54));
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    }
    window.set_visible(true);
    window.present();
}

/// Update position margins dynamically when dragging or resizing
pub fn update_margins(window: &gtk4::ApplicationWindow, top: i32, bottom: i32, right: i32) {
    if window.is_layer_window() {
        window.set_margin(Edge::Right, right.max(0));
        window.set_margin(Edge::Top, top.max(8));
        window.set_margin(Edge::Bottom, bottom.max(54));
    }
}

/// Unmap the surface completely: 100% click-through until expanded again.
pub fn hide_overlay(window: &gtk4::ApplicationWindow) {
    if window.is_layer_window() {
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    }
    window.set_visible(false);
}

/// Emit an audible/visual bell notification on prompt completion
#[allow(dead_code)]
pub fn ring_completion_bell() {
    if let Some(display) = gdk::Display::default() {
        display.beep();
    }
}
