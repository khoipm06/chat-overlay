use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

pub fn init_layer_shell(window: &gtk4::ApplicationWindow) {
    window.init_layer_shell();
    window.set_namespace("pi-ephemeral-overlay");
    window.set_layer(Layer::Overlay);

    // Neo vào cạnh phải màn hình, kéo dài hết chiều cao
    window.set_anchor(Edge::Right, true);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Bottom, true);
    window.set_anchor(Edge::Left, false);

    // Không chiếm không gian của ứng dụng khác (exclusive zone = 0)
    window.set_exclusive_zone(0);

    // Bật sẵn tương tác bàn phím OnDemand và hiển thị
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
