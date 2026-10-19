use gtk4::glib;
use gtk4::pango;

/// Convert a markdown-formatted string into valid Pango markup.
/// Safely escapes XML entities (&, <, >) and translates common markdown syntax:
/// - Code blocks: ```lang ... ```
/// - Inline code: `code`
/// - Bold text: **bold**
/// - Headers: # Title, ## Section, ### Subsection
/// - Bullet items: - item, * item
/// - Error highlights: [Error] ...
pub fn format_markdown_to_pango(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len() + 128);
    let mut in_code_block = false;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                let lang = trimmed.trim_start_matches('`').trim();
                let lang_label = if lang.is_empty() { "code" } else { lang };
                let escaped_lang = glib::markup_escape_text(lang_label);
                out.push_str(&format!(
                    "<span font_family=\"monospace\" size=\"smaller\" foreground=\"#38bdf8\"><b>[{}]</b></span>\n<span font_family=\"monospace\" background=\"#1e1e24\" foreground=\"#e4e4e7\">",
                    escaped_lang
                ));
            } else {
                in_code_block = false;
                out.push_str("</span>\n");
            }
            continue;
        }

        let escaped = glib::markup_escape_text(line);
        if in_code_block {
            out.push_str("  ");
            out.push_str(&escaped);
            out.push('\n');
        } else {
            let mut formatted_line = escaped.to_string();

            // Headers
            if let Some(h) = formatted_line.strip_prefix("# ") {
                formatted_line = format!("<span size=\"large\" weight=\"bold\">{}</span>", h);
            } else if let Some(h) = formatted_line.strip_prefix("## ") {
                formatted_line = format!("<span weight=\"bold\">{}</span>", h);
            } else if let Some(h) = formatted_line.strip_prefix("### ") {
                formatted_line = format!("<span weight=\"bold\" style=\"italic\">{}</span>", h);
            } else if let Some(bullet) = formatted_line.strip_prefix("- ").or_else(|| formatted_line.strip_prefix("* ")) {
                formatted_line = format!("• {}", bullet);
            }

            // Error prefix highlight
            if formatted_line.starts_with("[Error]") {
                formatted_line = format!(
                    "<span foreground=\"#f87171\"><b>[Error]</b>{}</span>",
                    &formatted_line[7..]
                );
            }

            // Inline code: `code`
            formatted_line = format_inline_code(&formatted_line);
            // Bold: **bold**
            formatted_line = format_bold(&formatted_line);

            out.push_str(&formatted_line);
            out.push('\n');
        }
    }

    if in_code_block {
        out.push_str("</span>");
    }

    out.trim_end().to_string()
}

fn format_inline_code(s: &str) -> String {
    let parts: Vec<&str> = s.split('`').collect();
    if parts.len() % 2 == 0 {
        // Unclosed backtick (e.g. while actively streaming): keep raw escaped
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len() + 32);
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 1 {
            result.push_str("<tt><span foreground=\"#38bdf8\">");
            result.push_str(part);
            result.push_str("</span></tt>");
        } else {
            result.push_str(part);
        }
    }
    result
}

fn format_bold(s: &str) -> String {
    let parts: Vec<&str> = s.split("**").collect();
    if parts.len() % 2 == 0 {
        // Unclosed bold marker: keep raw escaped
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len() + 16);
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 1 {
            result.push_str("<b>");
            result.push_str(part);
            result.push_str("</b>");
        } else {
            result.push_str(part);
        }
    }
    result
}

/// Applies formatted text to the assistant label. Validates against Pango parser
/// and falls back gracefully to plain text if malformed.
pub fn apply_assistant_markup(label: &gtk4::Label, raw: &str) {
    let pango = format_markdown_to_pango(raw);
    if pango::parse_markup(&pango, '\0').is_ok() {
        label.set_markup(&pango);
    } else {
        label.set_text(raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::prelude::*;

    #[test]
    fn test_plain_text() {
        let text = "Hello world! How are you?";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert_eq!(markup, text);
    }

    #[test]
    fn test_xml_escaping() {
        let text = "Symbols: <tag> & \"quotes\" and 'apostrophe' with > arrow";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("&lt;tag&gt;"));
        assert!(markup.contains("&amp;"));
    }

    #[test]
    fn test_code_block() {
        let text = "Code example:\n```rust\nfn main() {\n    println!(\"Hi\");\n}\n```\nDone.";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("[rust]"));
        assert!(markup.contains("font_family=\"monospace\""));
    }

    #[test]
    fn test_unclosed_code_block_during_streaming() {
        let text = "Streaming code:\n```python\nprint(123)\n";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.ends_with("</span>"));
    }

    #[test]
    fn test_inline_code_and_bold() {
        let text = "Check `main.rs` and **important note** here.";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("<tt><span foreground=\"#38bdf8\">main.rs</span></tt>"));
        assert!(markup.contains("<b>important note</b>"));
    }

    #[test]
    fn test_unclosed_inline_markers() {
        let text = "Streaming `unfinished code";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());

        let text2 = "Streaming **unfinished bold";
        let markup2 = format_markdown_to_pango(text2);
        assert!(pango::parse_markup(&markup2, '\0').is_ok());
    }

    #[test]
    fn test_headers_and_bullets() {
        let text = "# Main Title\n## Sub Title\n- First item\n* Second item";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("<span size=\"large\" weight=\"bold\">Main Title</span>"));
        assert!(markup.contains("• First item"));
        assert!(markup.contains("• Second item"));
    }

    #[test]
    fn test_error_highlight() {
        let text = "[Error] Process exited with code 1";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("<span foreground=\"#f87171\"><b>[Error]</b>"));
    }

    #[test]
    fn test_gtk_ui_and_styling() {
        let _ = gtk4::init();

        // 1. Verify CSS syntax validity
        let provider = gtk4::CssProvider::new();
        let error_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let error_count_clone = error_count.clone();
        let css_data = include_str!("../resources/style.css");

        provider.connect_parsing_error(move |_, _, _| {
            error_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });

        provider.load_from_data(css_data);
        assert_eq!(
            error_count.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "style.css contains GTK CSS parsing errors!"
        );

        // 2. Verify Bubble layout & natural height measurement
        let win = gtk4::Window::builder()
            .default_width(500)
            .default_height(700)
            .build();

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .build();
        win.set_child(Some(&scrolled));

        let messages_box = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
        scrolled.set_child(Some(&messages_box));

        // User Bubble
        let user_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        user_row.set_halign(gtk4::Align::End);
        let user_bubble = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let user_label = gtk4::Label::builder()
            .label("User test message")
            .wrap(true)
            .selectable(true)
            .build();
        user_bubble.append(&user_label);
        user_row.append(&user_bubble);
        messages_box.append(&user_row);

        // Assistant Bubble
        let asst_row = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let asst_bubble = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let asst_label = gtk4::Label::builder()
            .wrap(true)
            .wrap_mode(pango::WrapMode::WordChar)
            .selectable(true)
            .xalign(0.0)
            .build();
        apply_assistant_markup(&asst_label, "Line 1\nLine 2\nLine 3\n```code\nfn test() {}\n```");
        asst_bubble.append(&asst_label);
        asst_row.append(&asst_bubble);
        messages_box.append(&asst_row);

        win.present();
        win.allocate(500, 700, -1, None);

        let (min_h, nat_h, _, _) = asst_label.measure(gtk4::Orientation::Vertical, 450);
        assert!(nat_h > 0, "Assistant label must have non-zero natural height!");
        assert!(min_h > 0, "Assistant label must have non-zero minimum height!");

        let (_, asst_h) = asst_bubble.preferred_size();
        assert!(asst_h.height() > 0, "Assistant bubble must have non-zero height!");

        // 3. Verify Floating Bubble (Messenger Chat Head) widget and size transitions
        let bubble_button = gtk4::Button::builder()
            .css_classes(["floating-bubble-btn", "theme-acrylic"])
            .valign(gtk4::Align::Center)
            .halign(gtk4::Align::End)
            .width_request(56)
            .height_request(56)
            .visible(false)
            .build();

        let bubble_overlay = gtk4::Overlay::new();
        let bubble_label = gtk4::Label::builder()
            .label("π")
            .css_classes(["floating-bubble-icon"])
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .build();
        bubble_overlay.set_child(Some(&bubble_label));

        let bubble_dot = gtk4::Box::builder()
            .css_classes(["floating-bubble-dot"])
            .halign(gtk4::Align::End)
            .valign(gtk4::Align::Start)
            .width_request(10)
            .height_request(10)
            .visible(false)
            .build();
        bubble_overlay.add_overlay(&bubble_dot);
        bubble_button.set_child(Some(&bubble_overlay));

        // Drawer simulation
        let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        drawer.add_css_class("theme-acrylic");
        drawer.set_width_request(500);
        drawer.set_vexpand(true);

        // Assemble Root Box
        let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        root_box.set_halign(gtk4::Align::End);
        root_box.append(&bubble_button);
        root_box.append(&drawer);

        let bubble_win = gtk4::Window::builder()
            .default_width(500)
            .default_height(700)
            .child(&root_box)
            .build();
        bubble_win.present();

        // Verify Expanded state measurement
        drawer.set_visible(true);
        bubble_button.set_visible(false);
        let (_, expanded_req) = root_box.preferred_size();
        assert!(expanded_req.width() >= 500, "Expanded mode must request >= 500px width");

        // Verify Collapsed state measurement
        drawer.set_visible(false);
        bubble_button.set_visible(true);
        let (_, collapsed_req) = root_box.preferred_size();
        assert!(collapsed_req.width() <= 70, "Collapsed bubble mode must request compact width (~56px)");
        assert!(collapsed_req.height() <= 70, "Collapsed bubble mode must request compact height (~56px)");

        // Verify Thinking status indicator
        bubble_dot.set_visible(true);
        bubble_button.add_css_class("thinking");
        assert!(bubble_dot.is_visible());
        assert!(bubble_button.has_css_class("thinking"));

        bubble_dot.set_visible(false);
        bubble_button.remove_css_class("thinking");
        assert!(!bubble_dot.is_visible());
        assert!(!bubble_button.has_css_class("thinking"));

        // Verify Theme Switching propagation
        let themes = ["theme-acrylic", "theme-oled", "theme-ocean", "theme-emerald", "theme-light"];
        for t in &themes {
            for other in &themes {
                drawer.remove_css_class(other);
                bubble_button.remove_css_class(other);
            }
            drawer.add_css_class(t);
            bubble_button.add_css_class(t);
            assert!(drawer.has_css_class(t));
            assert!(bubble_button.has_css_class(t));
        }
    }
}
