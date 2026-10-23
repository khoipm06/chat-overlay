use gtk4::glib;
use gtk4::pango;

/// Convert a markdown-formatted string into valid Pango markup.
/// Safely escapes XML entities (&, <, >) and translates common markdown syntax:
/// - Code blocks: ```lang ... ``` with distinct header tag and indented code box
/// - Inline code: `code` -> <tt>code</tt>
/// - Bold text: **bold** -> <b>bold</b>
/// - Italic text: *italic* -> <i>italic</i>
/// - Headers: # Title, ## Section, ### Subsection
/// - Bullet items: - item, * item -> • item
/// - Numbered items: 1. item, 2. item -> formatted list
/// - Blockquotes: > quote -> italicized indented text
/// - Error highlights: [Error] ... in red
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
            } else if let Some(quote) = formatted_line.strip_prefix("&gt; ") {
                // Blockquotes
                formatted_line = format!(
                    "<span style=\"italic\" foreground=\"#b5b2bc\">  │ {}</span>",
                    quote
                );
            } else if let Some(bullet) = formatted_line
                .strip_prefix("- ")
                .or_else(|| formatted_line.strip_prefix("* "))
            {
                formatted_line = format!("• {}", bullet);
            }

            // Error prefix highlight (Radix red-9)
            if formatted_line.starts_with("[Error]") {
                formatted_line = format!(
                    "<span foreground=\"#e5484d\"><b>[Error]</b>{}</span>",
                    &formatted_line[7..]
                );
            }

            // Inline code: `code`
            formatted_line = format_inline_code(&formatted_line);
            // Bold: **bold**
            formatted_line = format_bold(&formatted_line);
            // Italic: *italic*
            formatted_line = format_italic(&formatted_line);

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
    if parts.len().is_multiple_of(2) {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len() + 32);
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 1 {
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
    if parts.len().is_multiple_of(2) {
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

fn format_italic(s: &str) -> String {
    let parts: Vec<&str> = s.split('*').collect();
    if parts.len().is_multiple_of(2) {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len() + 16);
    for (i, part) in parts.iter().enumerate() {
        if i % 2 == 1 {
            result.push_str("<i>");
            result.push_str(part);
            result.push_str("</i>");
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
    fn test_blockquote_and_italic() {
        let text = "> Quoted text here with *emphasis*";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("│ Quoted text here"));
        assert!(markup.contains("<i>emphasis</i>"));
    }

    #[test]
    fn test_error_highlight() {
        let text = "[Error] Process exited with code 1";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("<span foreground=\"#e5484d\"><b>[Error]</b>"));
    }
}
