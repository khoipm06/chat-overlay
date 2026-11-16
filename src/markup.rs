//! Screen markup: agent-authored highlights and the user's own annotation.
//!
//! The agent points at things by emitting inline tags in its reply:
//!
//! ```text
//! [[markup: box x=12 y=30 w=25 h=10 label="Retry button"]]
//! [[markup: arrow x1=50 y1=50 x2=25 y2=30 label="here"]]
//! ```
//!
//! Coordinates are percentages of the screen (0–100) with the origin at the
//! top-left, so they survive resolution and scale-factor changes.

use std::path::PathBuf;

use gtk4::prelude::*;

/// Accent colours, in the order the palette exposes them.
pub const PALETTE: [(&str, (f64, f64, f64)); 4] = [
    ("Red", (0.98, 0.27, 0.28)),
    ("Blue", (0.23, 0.58, 0.99)),
    ("Green", (0.20, 0.78, 0.45)),
    ("Yellow", (0.98, 0.75, 0.18)),
];

/// One annotation.
#[derive(Debug, Clone, PartialEq)]
pub enum MarkupShape {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        label: String,
        color: (f64, f64, f64),
    },
    Arrow {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        label: String,
        color: (f64, f64, f64),
    },
    /// Freehand stroke, in screen percentages.
    Pen {
        points: Vec<(f64, f64)>,
        color: (f64, f64, f64),
    },
    Text {
        x: f64,
        y: f64,
        text: String,
        color: (f64, f64, f64),
    },
}

impl MarkupShape {
    pub fn color(&self) -> (f64, f64, f64) {
        match self {
            MarkupShape::Rect { color, .. }
            | MarkupShape::Arrow { color, .. }
            | MarkupShape::Pen { color, .. }
            | MarkupShape::Text { color, .. } => *color,
        }
    }
}

fn default_color() -> (f64, f64, f64) {
    PALETTE[0].1
}

/// Parses a `color=` attribute value (`red`, `blue`, or `#RRGGBB`).
fn parse_color(value: &str) -> Option<(f64, f64, f64)> {
    let lower = value.trim().to_ascii_lowercase();
    for (name, rgb) in PALETTE {
        if lower == name.to_ascii_lowercase() {
            return Some(rgb);
        }
    }
    let hex = lower.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let channel = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|v| v as f64 / 255.0)
    };
    Some((channel(0)?, channel(2)?, channel(4)?))
}

/// Extracts `key=value` pairs; quoted values may contain spaces.
fn parse_attrs(body: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut chars = body.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        if chars.peek() != Some(&'=') {
            continue;
        }
        chars.next(); // consume '='

        let mut value = String::new();
        if chars.peek() == Some(&'"') {
            chars.next();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                value.push(c);
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                value.push(c);
                chars.next();
            }
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }

    attrs
}

fn attr_f64(attrs: &[(String, String)], key: &str) -> Option<f64> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| v.parse::<f64>().ok())
}

fn attr_str(attrs: &[(String, String)], key: &str) -> String {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn coord_factor(explicit_scale: Option<f64>, coords: &[f64]) -> f64 {
    let scale = explicit_scale.unwrap_or(if coords.iter().any(|&v| v > 100.0) {
        1000.0
    } else {
        100.0
    });
    100.0 / scale
}

/// Parses all `[[markup: ...]]` tags in `text`.
///
/// Malformed tags are skipped rather than aborting the scan, so one bad tag
/// cannot suppress the rest of the annotations.
pub fn parse_agent_markup(text: &str) -> Vec<MarkupShape> {
    let mut shapes = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find("[[markup:") {
        let after = &rest[start + "[[markup:".len()..];
        let Some(end) = after.find("]]") else { break };
        let body = after[..end].trim();
        rest = &after[end + 2..];

        let mut parts = body.splitn(2, char::is_whitespace);
        let kind = parts.next().unwrap_or("").to_ascii_lowercase();
        let attr_body = parts.next().unwrap_or("");
        let attrs = parse_attrs(attr_body);
        let color = attr_str(&attrs, "color")
            .parse::<String>()
            .ok()
            .and_then(|c| parse_color(&c))
            .unwrap_or_else(default_color);
        let label = attr_str(&attrs, "label");
        let explicit_scale = attr_f64(&attrs, "scale").filter(|s| *s > 0.0);

        match kind.as_str() {
            "box" | "rect" => {
                let (Some(x), Some(y), Some(w), Some(h)) = (
                    attr_f64(&attrs, "x"),
                    attr_f64(&attrs, "y"),
                    attr_f64(&attrs, "w"),
                    attr_f64(&attrs, "h"),
                ) else {
                    continue;
                };
                let factor = coord_factor(explicit_scale, &[x, y, w, h]);
                shapes.push(MarkupShape::Rect {
                    x: x * factor,
                    y: y * factor,
                    w: w * factor,
                    h: h * factor,
                    label,
                    color,
                });
            }
            "arrow" => {
                let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                    attr_f64(&attrs, "x1"),
                    attr_f64(&attrs, "y1"),
                    attr_f64(&attrs, "x2"),
                    attr_f64(&attrs, "y2"),
                ) else {
                    continue;
                };
                let factor = coord_factor(explicit_scale, &[x1, y1, x2, y2]);
                shapes.push(MarkupShape::Arrow {
                    x1: x1 * factor,
                    y1: y1 * factor,
                    x2: x2 * factor,
                    y2: y2 * factor,
                    label,
                    color,
                });
            }
            "text" => {
                let (Some(x), Some(y)) = (attr_f64(&attrs, "x"), attr_f64(&attrs, "y")) else {
                    continue;
                };
                if label.trim().is_empty() {
                    continue;
                }
                let factor = coord_factor(explicit_scale, &[x, y]);
                shapes.push(MarkupShape::Text {
                    x: x * factor,
                    y: y * factor,
                    text: label,
                    color,
                });
            }
            _ => {}
        }
    }

    shapes
}

/// Removes `[[markup: ...]]` tags so the reply reads as prose.
pub fn strip_agent_markup_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("[[markup:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "[[markup:".len()..];
        match after.find("]]") {
            Some(end) => rest = &after[end + 2..],
            None => {
                // Unterminated tag: drop it from here on.
                return out.trim_end().to_string();
            }
        }
    }
    out.push_str(rest);
    out.trim_end().to_string()
}

/// Like [`strip_agent_markup_tags`], but keeps an unterminated trailing tag
/// invisible instead of truncating the text.
///
/// During streaming a tag arrives across several deltas; this lets the visible
/// text stay correct without discarding the partial tag from the buffer.
pub fn strip_agent_markup_tags_preserving_open(text: &str) -> String {
    let Some(start) = text.rfind("[[markup:") else {
        return text.to_string();
    };
    // A tag that already closed is complete; only the tail can be open.
    let after = &text[start + "[[markup:".len()..];
    if after.contains("]]") {
        return strip_agent_markup_tags(text);
    }
    strip_agent_markup_tags(&text[..start])
}

/// Converts a screen percentage into a pixel coordinate on the primary monitor.
fn pct_to_px(pct: f64, extent: f64) -> f64 {
    (pct.clamp(0.0, 100.0) / 100.0) * extent
}

/// Draws `shapes` onto `cr`, scaled to `width` x `height` pixels.
pub fn draw_shapes(
    cr: &gtk4::cairo::Context,
    shapes: &[MarkupShape],
    width: f64,
    height: f64,
    with_labels: bool,
) {
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_line_join(gtk4::cairo::LineJoin::Round);

    for shape in shapes {
        let (r, g, b) = shape.color();

        match shape {
            MarkupShape::Rect { x, y, w, h, label, .. } => {
                let px = pct_to_px(*x, width);
                let py = pct_to_px(*y, height);
                let pw = pct_to_px(*w, width);
                let ph = pct_to_px(*h, height);

                // Soft glow: a wide translucent stroke under the crisp one.
                cr.set_source_rgba(r, g, b, 0.25);
                cr.set_line_width(7.0);
                cr.rectangle(px, py, pw, ph);
                let _ = cr.stroke();

                cr.set_source_rgba(r, g, b, 0.95);
                cr.set_line_width(2.5);
                cr.rectangle(px, py, pw, ph);
                let _ = cr.stroke();

                if with_labels && !label.is_empty() {
                    draw_label(cr, label, px, py, width, (r, g, b));
                }
            }
            MarkupShape::Arrow { x1, y1, x2, y2, label, .. } => {
                let (ax1, ay1) = (pct_to_px(*x1, width), pct_to_px(*y1, height));
                let (ax2, ay2) = (pct_to_px(*x2, width), pct_to_px(*y2, height));

                cr.set_source_rgba(r, g, b, 0.25);
                cr.set_line_width(8.0);
                cr.move_to(ax1, ay1);
                cr.line_to(ax2, ay2);
                let _ = cr.stroke();

                cr.set_source_rgba(r, g, b, 0.95);
                cr.set_line_width(3.0);
                cr.move_to(ax1, ay1);
                cr.line_to(ax2, ay2);
                let _ = cr.stroke();

                // Arrowhead
                let angle = (ay2 - ay1).atan2(ax2 - ax1);
                let head = 16.0;
                cr.move_to(ax2, ay2);
                cr.line_to(
                    ax2 - head * (angle - 0.38).cos(),
                    ay2 - head * (angle - 0.38).sin(),
                );
                cr.line_to(
                    ax2 - head * (angle + 0.38).cos(),
                    ay2 - head * (angle + 0.38).sin(),
                );
                cr.close_path();
                let _ = cr.fill();

                if with_labels && !label.is_empty() {
                    draw_label(cr, label, ax2, ay2, width, (r, g, b));
                }
            }
            MarkupShape::Pen { points, .. } => {
                if points.len() < 2 {
                    continue;
                }
                cr.set_source_rgba(r, g, b, 0.95);
                cr.set_line_width(3.0);
                let (sx, sy) = points[0];
                cr.move_to(pct_to_px(sx, width), pct_to_px(sy, height));
                for (x, y) in &points[1..] {
                    cr.line_to(pct_to_px(*x, width), pct_to_px(*y, height));
                }
                let _ = cr.stroke();
            }
            MarkupShape::Text { x, y, text, .. } => {
                draw_label(
                    cr,
                    text,
                    pct_to_px(*x, width),
                    pct_to_px(*y, height),
                    width,
                    (r, g, b),
                );
            }
        }
    }
}

/// Draws a pill-shaped label anchored just above `(x, y)`.
fn draw_label(
    cr: &gtk4::cairo::Context,
    text: &str,
    x: f64,
    y: f64,
    screen_w: f64,
    color: (f64, f64, f64),
) {
    let (r, g, b) = color;

    cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
    cr.set_font_size(14.0);

    let extents = match cr.text_extents(text) {
        Ok(e) => e,
        Err(_) => return,
    };
    let pad_x = 8.0;
    let pad_y = 5.0;
    let box_w = extents.width() + pad_x * 2.0;
    let box_h = extents.height() + pad_y * 2.0;
    // Keep the pill on screen when the anchor is near an edge.
    let box_x = x.clamp(4.0, (screen_w - box_w - 4.0).max(4.0));
    let box_y = (y - box_h - 6.0).max(4.0);

    cr.set_source_rgba(0.07, 0.07, 0.07, 0.92);
    rounded_rect(cr, box_x, box_y, box_w, box_h, 6.0);
    let _ = cr.fill();

    cr.set_source_rgba(r, g, b, 0.95);
    cr.set_line_width(1.5);
    rounded_rect(cr, box_x, box_y, box_w, box_h, 6.0);
    let _ = cr.stroke();

    cr.set_source_rgba(0.97, 0.97, 0.97, 1.0);
    // Baseline sits at the bottom inset by the font descent.
    let baseline = box_y + pad_y + extents.height() - extents.height() * 0.22;
    cr.move_to(box_x + pad_x, baseline);
    let _ = cr.show_text(text);
}

/// Cairo has no rounded-rectangle primitive; this builds the path.
fn rounded_rect(cr: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    let r = radius.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
    cr.close_path();
}

/// Fullscreen transparent overlay that draws the agent's annotations.
///
/// The whole surface is click-through except a small pill at the top centre, so
/// the user keeps interacting with whatever is underneath.
#[derive(Clone)]
pub struct AgentMarkupOverlay {
    window: gtk4::ApplicationWindow,
    shapes: std::rc::Rc<std::cell::RefCell<Vec<MarkupShape>>>,
    area: gtk4::DrawingArea,
    click_area: gtk4::Box,
}

/// Height of the "clear" pill, which is also its clickable band.
const PILL_HEIGHT: i32 = 44;

/// Restricts pointer input on `window`'s Wayland surface strictly to the
/// floating control pill so the rest of the desktop stays click-through.
fn update_pill_input_region(window: &gtk4::ApplicationWindow, click_area: &gtk4::Box) {
    let Some(surface) = window.surface() else {
        return;
    };
    let rect = if let Some(bounds) = click_area.compute_bounds(window) {
        gtk4::cairo::RectangleInt::new(
            bounds.x().floor() as i32,
            bounds.y().floor() as i32,
            (bounds.width().ceil() as i32).max(160),
            (bounds.height().ceil() as i32).max(PILL_HEIGHT),
        )
    } else {
        let win_w = window.width().max(800);
        gtk4::cairo::RectangleInt::new((win_w / 2 - 110).max(0), 0, 220, PILL_HEIGHT)
    };
    let region = gtk4::cairo::Region::create_rectangle(&rect);
    surface.set_input_region(&region);
}

impl AgentMarkupOverlay {
    pub fn new(app: &gtk4::Application, shapes: Vec<MarkupShape>) -> Self {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("Screen Markup")
            .decorated(false)
            .build();
        window.add_css_class("markup-overlay-window");

        crate::overlay::init_markup_overlay(&window);

        let shapes = std::rc::Rc::new(std::cell::RefCell::new(shapes));
        let area = gtk4::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);

        {
            let shapes = shapes.clone();
            area.set_draw_func(move |_, cr, width, height| {
                let shapes = shapes.borrow();
                draw_shapes(cr, &shapes, width as f64, height as f64, true);
            });
        }

        // Only the pill accepts input; everything else stays click-through.
        let click_area = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        click_area.set_halign(gtk4::Align::Center);
        click_area.set_valign(gtk4::Align::Start);
        click_area.set_size_request(-1, PILL_HEIGHT);

        let clear_btn = gtk4::Button::builder()
            .label("Clear Screen Markup")
            .css_classes(["markup-clear-btn"])
            .build();
        {
            let window = window.clone();
            clear_btn.connect_clicked(move |_| window.close());
        }
        click_area.append(&clear_btn);

        let root = gtk4::Overlay::new();
        root.set_child(Some(&area));
        root.add_overlay(&click_area);
        window.set_child(Some(&root));

        {
            let click_ref = click_area.clone();
            window.connect_realize(move |win| {
                update_pill_input_region(win, &click_ref);
            });
        }
        {
            let win_ref = window.clone();
            let click_ref = click_area.clone();
            area.connect_resize(move |_, _, _| {
                update_pill_input_region(&win_ref, &click_ref);
            });
        }

        Self {
            window,
            shapes,
            area,
            click_area,
        }
    }

    pub fn present(&self) {
        self.window.present();
        let win = self.window.clone();
        let click_area = self.click_area.clone();
        gtk4::glib::idle_add_local_once(move || {
            update_pill_input_region(&win, &click_area);
        });
    }

    pub fn update_shapes(&self, new_shapes: Vec<MarkupShape>) {
        *self.shapes.borrow_mut() = new_shapes;
        self.area.queue_draw();
        update_pill_input_region(&self.window, &self.click_area);
    }

    pub fn is_open(&self) -> bool {
        self.window.is_visible()
    }

    pub fn close(&self) {
        self.window.close();
    }
}

/// Interactive fullscreen annotation editor over a screenshot.
///
/// `on_complete` receives the flattened PNG path on attach, or `None` on cancel.
pub fn open_user_markup_editor<F>(app: &gtk4::Application, screenshot: PathBuf, on_complete: F)
where
    F: FnOnce(Option<PathBuf>) + 'static,
{
    let editor = UserMarkupEditor::new(app, screenshot, on_complete);
    editor.window.present();
}

/// Tools the user can draw with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkupTool {
    Box,
    Arrow,
    Pen,
    Text,
}

impl MarkupTool {
    fn label(self) -> &'static str {
        match self {
            MarkupTool::Box => "Box",
            MarkupTool::Arrow => "Arrow",
            MarkupTool::Pen => "Pen",
            MarkupTool::Text => "Text",
        }
    }

    fn all() -> [MarkupTool; 4] {
        [MarkupTool::Box, MarkupTool::Arrow, MarkupTool::Pen, MarkupTool::Text]
    }
}

/// Mutable editor session state.
struct EditorState {
    tool: MarkupTool,
    color: (f64, f64, f64),
    shapes: Vec<MarkupShape>,
    /// In-progress drag, in screen percentages.
    dragging: Option<(f64, f64, f64, f64)>,
    pen_points: Vec<(f64, f64)>,
}

struct UserMarkupEditor {
    window: gtk4::ApplicationWindow,
    state: std::rc::Rc<std::cell::RefCell<EditorState>>,
    area: gtk4::DrawingArea,
    screenshot: PathBuf,
}

/// Shared, single-shot completion callback.
///
/// Both "Attach" and "Cancel" take it, so the caller can never be invoked twice.
type CompleteFn = std::rc::Rc<std::cell::RefCell<Option<Box<dyn FnOnce(Option<PathBuf>)>>>>;

fn take_complete(cb: &CompleteFn, value: Option<PathBuf>) {
    if let Some(f) = cb.borrow_mut().take() {
        f(value);
    }
}

impl UserMarkupEditor {
    fn new<F>(app: &gtk4::Application, screenshot: PathBuf, on_complete: F) -> Self
    where
        F: FnOnce(Option<PathBuf>) + 'static,
    {
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("Annotate Screen")
            .decorated(false)
            .build();
        window.add_css_class("markup-editor-window");

        crate::overlay::init_markup_overlay(&window);

        let on_complete: CompleteFn =
            std::rc::Rc::new(std::cell::RefCell::new(Some(Box::new(on_complete))));

        let state = std::rc::Rc::new(std::cell::RefCell::new(EditorState {
            tool: MarkupTool::Box,
            color: PALETTE[0].1,
            shapes: Vec::new(),
            dragging: None,
            pen_points: Vec::new(),
        }));

        let area = gtk4::DrawingArea::new();
        area.set_hexpand(true);
        area.set_vexpand(true);
        area.set_focusable(true);

        // Background: the screenshot; the drawing area sits on top of it.
        let picture = gtk4::Picture::for_filename(&screenshot);
        picture.set_can_shrink(true);

        {
            let state = state.clone();
            area.set_draw_func(move |_, cr, width, height| {
                let state = state.borrow();
                if let Some((x1, y1, x2, y2)) = state.dragging {
                    let preview = match state.tool {
                        MarkupTool::Box => MarkupShape::Rect {
                            x: x1.min(x2),
                            y: y1.min(y2),
                            w: (x2 - x1).abs(),
                            h: (y2 - y1).abs(),
                            label: String::new(),
                            color: state.color,
                        },
                        _ => MarkupShape::Arrow {
                            x1,
                            y1,
                            x2,
                            y2,
                            label: String::new(),
                            color: state.color,
                        },
                    };
                    draw_shapes(cr, &[preview], width as f64, height as f64, false);
                }
                if state.pen_points.len() >= 2 {
                    draw_shapes(
                        cr,
                        &[MarkupShape::Pen {
                            points: state.pen_points.clone(),
                            color: state.color,
                        }],
                        width as f64,
                        height as f64,
                        false,
                    );
                }
                draw_shapes(cr, &state.shapes, width as f64, height as f64, true);
            });
        }

        let root = gtk4::Overlay::new();
        root.set_child(Some(&picture));
        root.add_overlay(&area);

        let (toolbar, attach_btn, cancel_btn) = build_toolbar(&state, &area);
        root.add_overlay(&toolbar);
        window.set_child(Some(&root));

        let editor = Self {
            window,
            state,
            area,
            screenshot,
        };

        editor.connect_canvas();
        editor.connect_attach(&attach_btn, &on_complete);
        editor.connect_cancel(&cancel_btn, &on_complete);
        editor
    }

    /// Drag gestures: rubber-band for Box/Arrow, freehand for Pen.
    fn connect_canvas(&self) {
        let drag = gtk4::GestureDrag::new();
        drag.set_button(1);

        {
            let state = self.state.clone();
            let area = self.area.clone();
            drag.connect_drag_begin(move |_, x, y| {
                let (w, h) = (area.width() as f64, area.height() as f64);
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                let (px, py) = (x / w * 100.0, y / h * 100.0);
                let mut st = state.borrow_mut();
                match st.tool {
                    MarkupTool::Pen => st.pen_points = vec![(px, py)],
                    _ => st.dragging = Some((px, py, px, py)),
                }
            });
        }
        {
            let state = self.state.clone();
            let area = self.area.clone();
            drag.connect_drag_update(move |_, dx, dy| {
                let (w, h) = (area.width() as f64, area.height() as f64);
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                let mut st = state.borrow_mut();
                if st.tool == MarkupTool::Pen {
                    // Drag offsets are relative to the press point.
                    let (cx, cy) = (dx / w * 100.0, dy / h * 100.0);
                    if let Some((sx, sy)) = st.pen_points.first().copied() {
                        st.pen_points.push((sx + cx, sy + cy));
                    }
                } else if let Some((x1, y1, _, _)) = st.dragging {
                    st.dragging = Some((x1, y1, x1 + dx / w * 100.0, y1 + dy / h * 100.0));
                }
                drop(st);
                area.queue_draw();
            });
        }
        {
            let state = self.state.clone();
            let area = self.area.clone();
            drag.connect_drag_end(move |_, _, _| {
                let mut st = state.borrow_mut();
                if st.tool == MarkupTool::Pen {
                    let points = std::mem::take(&mut st.pen_points);
                    if points.len() >= 2 {
                        let color = st.color;
                        st.shapes.push(MarkupShape::Pen { points, color });
                    }
                } else if let Some((x1, y1, x2, y2)) = st.dragging.take() {
                    let color = st.color;
                    let tool = st.tool;
                    // Ignore accidental clicks that produced no extent.
                    if (x2 - x1).abs() > 0.5 || (y2 - y1).abs() > 0.5 {
                        st.shapes.push(match tool {
                            MarkupTool::Box => MarkupShape::Rect {
                                x: x1.min(x2),
                                y: y1.min(y2),
                                w: (x2 - x1).abs(),
                                h: (y2 - y1).abs(),
                                label: String::new(),
                                color,
                            },
                            _ => MarkupShape::Arrow { x1, y1, x2, y2, label: String::new(), color },
                        });
                    }
                }
                drop(st);
                area.queue_draw();
            });
        }

        self.area.add_controller(drag);
        self.area.queue_draw();
    }

    /// "Attach to Chat": flatten the screenshot plus shapes, then report the path.
    fn connect_attach(&self, attach_btn: &gtk4::Button, on_complete: &CompleteFn) {
        let state = self.state.clone();
        let window = self.window.clone();
        let screenshot = self.screenshot.clone();
        let area = self.area.clone();
        let on_complete = on_complete.clone();
        attach_btn.connect_clicked(move |_| {
            let shapes = state.borrow().shapes.clone();
            // Export at the screenshot's own resolution, not the widget's, so
            // annotations line up regardless of window scaling.
            let result = export_markup_png(&screenshot, &shapes, 0.0, 0.0)
                .map_err(|e| {
                    eprintln!("markup export failed: {e}");
                    e
                })
                .ok();
            take_complete(&on_complete, result);
            let _ = area;
            window.close();
        });
    }

    fn connect_cancel(&self, cancel_btn: &gtk4::Button, on_complete: &CompleteFn) {
        let window = self.window.clone();
        let on_complete = on_complete.clone();
        cancel_btn.connect_clicked(move |_| {
            take_complete(&on_complete, None);
            window.close();
        });
    }
}

/// Builds the editor toolbar and returns it with its action buttons.
fn build_toolbar(
    state: &std::rc::Rc<std::cell::RefCell<EditorState>>,
    area: &gtk4::DrawingArea,
) -> (gtk4::Box, gtk4::Button, gtk4::Button) {
    let toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    toolbar.add_css_class("markup-toolbar");
    toolbar.set_halign(gtk4::Align::Center);
    toolbar.set_valign(gtk4::Align::Start);

    // Tool toggles behave as a radio group.
    let tool_buttons: std::rc::Rc<std::cell::RefCell<Vec<(MarkupTool, gtk4::ToggleButton)>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));

    for tool in MarkupTool::all() {
        let btn = gtk4::ToggleButton::builder()
            .label(tool.label())
            .css_classes(["markup-tool-btn"])
            .active(tool == MarkupTool::Box)
            .build();
        {
            let state = state.clone();
            let all = tool_buttons.clone();
            btn.connect_toggled(move |b| {
                if !b.is_active() {
                    return;
                }
                state.borrow_mut().tool = tool;
                for (other_tool, other_btn) in all.borrow().iter() {
                    if *other_tool != tool {
                        other_btn.set_active(false);
                    }
                }
            });
        }
        tool_buttons.borrow_mut().push((tool, btn.clone()));
        toolbar.append(&btn);
    }

    toolbar.append(&gtk4::Separator::new(gtk4::Orientation::Vertical));

    for (name, rgb) in PALETTE {
        let swatch = gtk4::Button::builder()
            .css_classes(["markup-swatch"])
            .tooltip_text(name)
            .build();
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(&format!(
            "button.markup-swatch {{ background: rgb({}, {}, {}); }}",
            (rgb.0 * 255.0) as u8,
            (rgb.1 * 255.0) as u8,
            (rgb.2 * 255.0) as u8
        ));
        swatch
            .style_context()
            .add_provider(&provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
        {
            let state = state.clone();
            swatch.connect_clicked(move |_| state.borrow_mut().color = rgb);
        }
        toolbar.append(&swatch);
    }

    toolbar.append(&gtk4::Separator::new(gtk4::Orientation::Vertical));

    let undo_btn = gtk4::Button::builder()
        .label("Undo")
        .css_classes(["markup-action-btn"])
        .build();
    {
        let state = state.clone();
        let area = area.clone();
        undo_btn.connect_clicked(move |_| {
            state.borrow_mut().shapes.pop();
            area.queue_draw();
        });
    }
    toolbar.append(&undo_btn);

    let clear_btn = gtk4::Button::builder()
        .label("Clear")
        .css_classes(["markup-action-btn"])
        .build();
    {
        let state = state.clone();
        let area = area.clone();
        clear_btn.connect_clicked(move |_| {
            state.borrow_mut().shapes.clear();
            area.queue_draw();
        });
    }
    toolbar.append(&clear_btn);

    toolbar.append(&gtk4::Separator::new(gtk4::Orientation::Vertical));

    let cancel_btn = gtk4::Button::builder()
        .label("Cancel")
        .css_classes(["markup-action-btn"])
        .build();
    toolbar.append(&cancel_btn);

    let attach_btn = gtk4::Button::builder()
        .label("Attach to Chat")
        .css_classes(["markup-action-btn", "markup-attach-btn"])
        .build();
    toolbar.append(&attach_btn);

    (toolbar, attach_btn, cancel_btn)
}

/// Flattens the screenshot plus `shapes` into a PNG written to the capture dir.
pub fn export_markup_png(
    screenshot: &std::path::Path,
    shapes: &[MarkupShape],
    width: f64,
    height: f64,
) -> Result<PathBuf, String> {
    let source = gtk4::cairo::ImageSurface::create_from_png(&mut std::fs::File::open(screenshot).map_err(
        |e| format!("cannot open screenshot: {e}"),
    )?)
    .map_err(|e| format!("cannot decode screenshot: {e}"))?;

    let (img_w, img_h) = (source.width() as f64, source.height() as f64);
    if img_w <= 0.0 || img_h <= 0.0 {
        return Err("screenshot has no extent".to_string());
    }

    // Export at the capture's native resolution so the annotations stay sharp.
    let out_w = img_w as i32;
    let out_h = img_h as i32;
    let target = gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, out_w, out_h)
        .map_err(|e| format!("cannot create surface: {e}"))?;

    {
        let cr = gtk4::cairo::Context::new(&target).map_err(|e| format!("context: {e}"))?;
        cr.set_source_surface(&source, 0.0, 0.0)
            .map_err(|e| format!("paint source: {e}"))?;
        let _ = cr.paint();
        // Shapes use the same 0–100 space as on screen, mapped onto the image.
        draw_shapes(&cr, shapes, img_w, img_h, true);
        let _ = width;
        let _ = height;
    }

    let dir = crate::capture::capture_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create capture dir: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let out_path = dir.join(format!("pi-markup-{stamp}.png"));

    let mut file = std::fs::File::create(&out_path).map_err(|e| format!("cannot create png: {e}"))?;
    target
        .write_to_png(&mut file)
        .map_err(|e| format!("cannot write png: {e}"))?;
    Ok(out_path)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_box_tag() {
        let shapes = parse_agent_markup(r#"[[markup: box x=10 y=20 w=30 h=15 label="Retry"]]"#);
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            MarkupShape::Rect { x, y, w, h, label, .. } => {
                assert_eq!(*x, 10.0);
                assert_eq!(*y, 20.0);
                assert_eq!(*w, 30.0);
                assert_eq!(*h, 15.0);
                assert_eq!(label, "Retry");
            }
            other => panic!("expected rect, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_arrow_tag() {
        let shapes = parse_agent_markup("[[markup: arrow x1=50 y1=50 x2=25 y2=30 label=\"here\"]]");
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            MarkupShape::Arrow { x1, y1, x2, y2, label, .. } => {
                assert_eq!((*x1, *y1, *x2, *y2), (50.0, 50.0, 25.0, 30.0));
                assert_eq!(label, "here");
            }
            other => panic!("expected arrow, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_multiple_tags_and_surrounding_prose() {
        let text = "Look at [[markup: box x=1 y=2 w=3 h=4]] the button, \
                    then [[markup: arrow x1=9 y1=9 x2=5 y2=5]].";
        let shapes = parse_agent_markup(text);
        assert_eq!(shapes.len(), 2);
    }

    #[test]
    fn test_strip_tags_leaves_prose() {
        let text = "Look at [[markup: box x=1 y=2 w=3 h=4]] the button.";
        let stripped = strip_agent_markup_tags(text);
        assert!(!stripped.contains("markup"));
        assert!(stripped.contains("Look at"));
        assert!(stripped.contains("the button."));
    }

    #[test]
    fn test_strip_tags_leaves_plain_text_untouched() {
        assert_eq!(strip_agent_markup_tags("nothing here"), "nothing here");
    }

    #[test]
    fn test_strip_handles_unterminated_tag() {
        let stripped = strip_agent_markup_tags("Prose [[markup: box x=1");
        assert_eq!(stripped, "Prose");
    }

    #[test]
    fn test_open_tag_streaming_keeps_preceding_prose() {
        // Mid-stream the tag is incomplete; the prose before it must stay put.
        let visible = strip_agent_markup_tags_preserving_open("Look here [[markup: box x=1");
        assert_eq!(visible.trim(), "Look here");

        // Once complete, the tag is removed and later prose is kept.
        let visible = strip_agent_markup_tags_preserving_open(
            "Look here [[markup: box x=1 y=1 w=1 h=1]] and click",
        );
        assert!(visible.starts_with("Look here "));
        assert!(visible.ends_with("and click"));
        assert!(!visible.contains("markup"));
    }

    #[test]
    fn test_open_tag_only_affects_a_trailing_tag() {
        // A closed tag followed by prose must not hide that prose.
        let text = "[[markup: box x=1 y=1 w=1 h=1]] trailing words";
        let visible = strip_agent_markup_tags_preserving_open(text);
        assert!(visible.contains("trailing words"));
    }

    #[test]
    fn test_open_tag_plain_text_untouched() {
        assert_eq!(
            strip_agent_markup_tags_preserving_open("no tags at all"),
            "no tags at all"
        );
    }

    #[test]
    fn test_has_agent_markup() {
        assert!(!parse_agent_markup("[[markup: box x=1 y=1 w=1 h=1]]").is_empty());
        assert!(parse_agent_markup("just prose").is_empty());
        assert!(parse_agent_markup("[[markup: box x=oops]]").is_empty());
    }

    #[test]
    fn test_named_colors_resolve() {
        let shapes = parse_agent_markup(r#"[[markup: box x=1 y=1 w=1 h=1 color="blue"]]"#);
        assert_eq!(shapes[0].color(), PALETTE[1].1);
    }

    #[test]
    fn test_hex_color_resolves() {
        let shapes = parse_agent_markup(r##"[[markup: box x=1 y=1 w=1 h=1 color="#FF0000"]]"##);
        let (r, g, b) = shapes[0].color();
        assert!((r - 1.0).abs() < 0.01);
        assert!(g.abs() < 0.01);
        assert!(b.abs() < 0.01);
    }

    #[test]
    fn test_unknown_color_falls_back_to_default() {
        let shapes = parse_agent_markup(r#"[[markup: box x=1 y=1 w=1 h=1 color="chartreuse"]]"#);
        assert_eq!(shapes[0].color(), PALETTE[0].1);
    }

    #[test]
    fn test_incomplete_geometry_is_skipped() {
        // Missing h: must not produce a shape, and must not stop the scan.
        let text = "[[markup: box x=1 y=2 w=3]] [[markup: box x=4 y=5 w=6 h=7]]";
        let shapes = parse_agent_markup(text);
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            MarkupShape::Rect { x, .. } => assert_eq!(*x, 4.0),
            other => panic!("expected rect, got {other:?}"),
        }
    }

    #[test]
    fn test_malformed_tag_does_not_abort_scan() {
        let text = "[[markup: nonsense]] [[markup: box x=1 y=1 w=1 h=1]]";
        let shapes = parse_agent_markup(text);
        assert_eq!(shapes.len(), 1);
    }

    #[test]
    fn test_text_tag_with_label() {
        let shapes = parse_agent_markup(r#"[[markup: text x=12 y=34 label="Check here"]]"#);
        match &shapes[0] {
            MarkupShape::Text { x, y, text, .. } => {
                assert_eq!((*x, *y), (12.0, 34.0));
                assert_eq!(text, "Check here");
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn test_empty_text_tag_is_skipped() {
        assert!(parse_agent_markup("[[markup: text x=1 y=2]]").is_empty());
    }

    #[test]
    fn test_parse_attrs_handles_quoted_spaces() {
        let attrs = parse_attrs(r#"x=1 label="two words" y=2"#);
        assert_eq!(attrs.len(), 3);
        assert_eq!(attr_str(&attrs, "label"), "two words");
        assert_eq!(attr_f64(&attrs, "y"), Some(2.0));
    }

    #[test]
    fn test_percentage_clamps_to_bounds() {
        assert_eq!(pct_to_px(-5.0, 100.0), 0.0);
        assert_eq!(pct_to_px(150.0, 100.0), 100.0);
        assert_eq!(pct_to_px(50.0, 200.0), 100.0);
    }

    #[test]
    fn test_tool_labels_are_distinct() {
        let labels: Vec<&str> = MarkupTool::all().iter().map(|t| t.label()).collect();
        assert_eq!(labels, vec!["Box", "Arrow", "Pen", "Text"]);
    }

    #[test]
    fn test_parse_scale_1000_coordinates() {
        let shapes = parse_agent_markup(
            r#"[[markup: box scale=1000 x=125 y=250 w=300 h=50 label="Retry"]]"#,
        );
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            MarkupShape::Rect { x, y, w, h, label, .. } => {
                assert!((*x - 12.5).abs() < 0.01);
                assert!((*y - 25.0).abs() < 0.01);
                assert!((*w - 30.0).abs() < 0.01);
                assert!((*h - 5.0).abs() < 0.01);
                assert_eq!(label, "Retry");
            }
            other => panic!("expected rect, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_auto_infers_1000_when_coords_exceed_100() {
        let shapes = parse_agent_markup(r#"[[markup: arrow x1=500 y1=500 x2=250 y2=300 label="Here"]]"#);
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            MarkupShape::Arrow { x1, y1, x2, y2, .. } => {
                assert!((*x1 - 50.0).abs() < 0.01);
                assert!((*y1 - 50.0).abs() < 0.01);
                assert!((*x2 - 25.0).abs() < 0.01);
                assert!((*y2 - 30.0).abs() < 0.01);
            }
            other => panic!("expected arrow, got {other:?}"),
        }
    }
}
