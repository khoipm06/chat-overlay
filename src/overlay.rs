use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

pub fn init_layer_shell(window: &gtk4::ApplicationWindow) {
    window.init_layer_shell();
    window.set_namespace("pi-ephemeral-overlay");
    window.set_layer(Layer::Overlay);

    // Anchor to the right edge of the screen, spanning full screen height
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, false);

    // Overlay on top of existing windows without claiming screen space
    window.set_exclusive_zone(0);

    // Enable on-demand keyboard interactivity and make visible
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    window.set_visible(true);
}

pub fn show_overlay(window: &gtk4::ApplicationWindow) {
    window.set_visible(true);
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::OnDemand);
    window.present();
}

pub fn hide_overlay(window: &gtk4::ApplicationWindow) {
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    window.set_visible(false);
}
