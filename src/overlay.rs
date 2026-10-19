use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

pub fn init_layer_shell(window: &gtk4::ApplicationWindow) {
    window.init_layer_shell();
    window.set_namespace("pi-ephemeral-overlay");
    window.set_layer(Layer::Overlay);

    // Overlay on top of existing windows without claiming screen space
    window.set_exclusive_zone(0);

    // Initialize in expanded drawer mode
    expand_overlay(window);
}

/// Transition layer surface to Expanded Drawer mode:
/// Anchored to Right, Top, Bottom (full monitor height), keyboard OnDemand, width 500px
pub fn expand_overlay(window: &gtk4::ApplicationWindow) {
    window.set_visible(true);
    if window.is_layer_window() {
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_margin(Edge::Right, 0);
        window.set_margin(Edge::Top, 0);
        window.set_margin(Edge::Bottom, 0);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    }
    window.set_default_size(500, -1);
    window.present();
}

/// Transition layer surface to Collapsed Floating Bubble mode:
/// Anchored only to Right edge (vertically centered), keyboard None, compact ~56x56px
pub fn collapse_to_bubble(window: &gtk4::ApplicationWindow) {
    window.set_visible(true);
    if window.is_layer_window() {
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        window.set_anchor(Edge::Top, false);
        window.set_anchor(Edge::Bottom, false);
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, true);
        window.set_margin(Edge::Right, 14);
        window.set_margin(Edge::Top, 0);
        window.set_margin(Edge::Bottom, 0);
    }
    window.set_default_size(56, 56);
    window.present();
}

#[allow(dead_code)]
pub fn show_overlay(window: &gtk4::ApplicationWindow) {
    expand_overlay(window);
}

#[allow(dead_code)]
pub fn hide_overlay(window: &gtk4::ApplicationWindow) {
    if window.is_layer_window() {
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    }
    window.set_visible(false);
}
