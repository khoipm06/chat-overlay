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

/// Map the layer surface as a floating right-side panel (Raycast style) with
/// keyboard focus on demand. Margins keep the panel inside the surface bounds
/// so the 1px border and rounded corners render on all four corners instead
/// of being clipped at the screen edge.
pub fn expand_overlay(window: &gtk4::ApplicationWindow) {
    if window.is_layer_window() {
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_margin(Edge::Right, 12);
        window.set_margin(Edge::Top, 12);
        window.set_margin(Edge::Bottom, 12);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    }
    window.set_visible(true);
    window.present();
}

/// Unmap the Wayland surface completely: not rendered, no input region,
/// 100% click-through until expanded again.
pub fn hide_overlay(window: &gtk4::ApplicationWindow) {
    if window.is_layer_window() {
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    }
    window.set_visible(false);
}
