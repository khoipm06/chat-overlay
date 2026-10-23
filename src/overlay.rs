use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

pub fn init_layer_shell(window: &gtk4::ApplicationWindow) {
    window.init_layer_shell();
    window.set_namespace("pi-ephemeral-overlay");
    window.set_layer(Layer::Overlay);

    // Overlay on top of existing windows without claiming screen space
    window.set_exclusive_zone(0);

    // Start fully hidden: unmapped surface, no input region, 100% click-through
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    window.set_visible(false);
}

/// Map the layer surface as a floating right-side panel with keyboard focus on demand.
/// Margin top/bottom controls panel height, while width request sets width.
pub fn expand_overlay(window: &gtk4::ApplicationWindow, top_margin: i32, bottom_margin: i32) {
    if window.is_layer_window() {
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_margin(Edge::Right, 12);
        window.set_margin(Edge::Top, top_margin.max(8));
        window.set_margin(Edge::Bottom, bottom_margin.max(8));
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    }
    window.set_visible(true);
    window.present();
}

/// Update vertical margins dynamically for vertical resizing
pub fn update_vertical_margins(window: &gtk4::ApplicationWindow, top: i32, bottom: i32) {
    if window.is_layer_window() {
        window.set_margin(Edge::Top, top.max(8));
        window.set_margin(Edge::Bottom, bottom.max(8));
    }
}

/// Unmap the Wayland surface completely: not rendered, no input region,
/// 100% click-through until expanded again.
pub fn hide_overlay(window: &gtk4::ApplicationWindow) {
    if window.is_layer_window() {
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    }
    window.set_visible(false);
}
