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
                    "<span font_family=\"monospace\" size=\"smaller\" foreground=\"#7c7a85\"><b>[{}]</b></span>\n<span font_family=\"monospace\" background=\"#1a191b\" foreground=\"#eeeef0\">",
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
                // Radix red-9 (identical in light and dark scales)
                formatted_line = format!(
                    "<span foreground=\"#e5484d\"><b>[Error]</b>{}</span>",
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
            // Color/background left to CSS (.chat-bubble-assistant tt) so it adapts per theme
            result.push_str("<tt>");
            result.push_str(part);
            result.push_str("</tt>");
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
        assert!(markup.contains("<tt>main.rs</tt>"));
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
        assert!(markup.contains("<span foreground=\"#e5484d\"><b>[Error]</b>"));
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

        // 2. Verify clean drawer layout & natural height measurement
        let win = gtk4::Window::builder()
            .default_width(480)
            .default_height(700)
            .build();

        let scrolled = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .build();
        win.set_child(Some(&scrolled));

        let messages_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        scrolled.set_child(Some(&messages_box));

        // User command block (Linear style flat block)
        let user_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        user_row.set_halign(gtk4::Align::Fill);
        let user_bubble = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        user_bubble.add_css_class("chat-bubble-user");
        let user_label = gtk4::Label::builder()
            .label("User test message")
            .wrap(true)
            .selectable(true)
            .build();
        user_bubble.append(&user_label);
        user_row.append(&user_bubble);
        messages_box.append(&user_row);

        // Assistant response (clean full-bleed layout)
        let asst_row = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let asst_bubble = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        asst_bubble.add_css_class("chat-bubble-assistant");
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
        win.allocate(480, 700, -1, None);

        let (min_h, nat_h, _, _) = asst_label.measure(gtk4::Orientation::Vertical, 440);
        assert!(nat_h > 0, "Assistant label must have non-zero natural height!");
        assert!(min_h > 0, "Assistant label must have non-zero minimum height!");

        let (_, asst_h) = asst_bubble.preferred_size();
        assert!(asst_h.height() > 0, "Assistant block must have non-zero height!");

        // 3. Verify Clean Drawer container (no floating bubble button)
        let drawer = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        drawer.set_widget_name("drawer-container");
        drawer.add_css_class("theme-dark-glass");
        drawer.set_width_request(480);
        drawer.set_vexpand(true);

        let root_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        root_box.set_halign(gtk4::Align::End);
        root_box.append(&drawer);

        let drawer_win = gtk4::Window::builder()
            .default_width(480)
            .default_height(700)
            .child(&root_box)
            .build();
        drawer_win.present();

        let (_, drawer_req) = root_box.preferred_size();
        assert!(drawer_req.width() >= 480, "Drawer must request >= 480px width");

        // Verify Theme Switching propagation on clean drawer
        let themes = ["theme-dark-glass", "theme-oled", "theme-slate", "theme-forest", "theme-zinc-light"];
        for t in &themes {
            for other in &themes {
                drawer.remove_css_class(other);
            }
            drawer.add_css_class(t);
            assert!(drawer.has_css_class(t));
        }
    }
}
