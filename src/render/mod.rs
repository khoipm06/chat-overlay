pub mod blocks;
pub mod inline;
pub mod syntax;
pub mod table;

use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum MarkdownBlock {
    Text(String),
    Code { lang: String, code: String },
}

/// Parses raw markdown into structured blocks: text paragraphs and fenced code blocks.
pub fn parse_markdown_blocks(input: &str) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    let mut text_acc = String::new();
    let mut code_acc = String::new();
    let mut current_lang = String::new();
    let mut in_code_block = false;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if !in_code_block {
                in_code_block = true;
                if !text_acc.is_empty() {
                    blocks.push(MarkdownBlock::Text(text_acc.clone()));
                    text_acc.clear();
                }
                let lang = trimmed.trim_start_matches('`').trim();
                current_lang = if lang.is_empty() {
                    "text".to_string()
                } else {
                    lang.to_string()
                };
            } else {
                in_code_block = false;
                blocks.push(MarkdownBlock::Code {
                    lang: current_lang.clone(),
                    code: code_acc.trim_end_matches('\n').to_string(),
                });
                code_acc.clear();
                current_lang.clear();
            }
            continue;
        }

        if in_code_block {
            code_acc.push_str(line);
            code_acc.push('\n');
        } else {
            text_acc.push_str(line);
            text_acc.push('\n');
        }
    }

    if in_code_block {
        blocks.push(MarkdownBlock::Code {
            lang: current_lang,
            code: code_acc.trim_end_matches('\n').to_string(),
        });
    } else if !text_acc.is_empty() {
        blocks.push(MarkdownBlock::Text(text_acc));
    }

    blocks
}

/// Formats a markdown text chunk into valid Pango markup.
/// Handles headings, lists, tables, callouts, alerts, and inline styling.
pub fn format_markdown_to_pango(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    let mut table_accumulator: Vec<&str> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // 1. Table row accumulation
        if table::is_table_row(line) {
            table_accumulator.push(line);
            continue;
        } else if !table_accumulator.is_empty() {
            out.push_str(&table::render_markdown_table(&table_accumulator));
            out.push('\n');
            table_accumulator.clear();
        }

        // 2. Horizontal Rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            out.push_str("<span foreground=\"#27272a\">────────────────────────────────────────</span>\n");
            continue;
        }

        // 3. Alerts / Quotes
        if let Some(formatted_quote) = blocks::format_alerts_and_quotes(line) {
            out.push_str(&formatted_quote);
            out.push('\n');
            continue;
        }

        // 4. Headings
        if let Some(h) = trimmed.strip_prefix("# ") {
            let h_esc = inline::format_inline_elements(h);
            out.push_str(&format!(
                "<span size=\"x-large\" weight=\"bold\" foreground=\"#ffffff\">{}</span>\n",
                h_esc
            ));
            continue;
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            let h_esc = inline::format_inline_elements(h);
            out.push_str(&format!(
                "<span size=\"large\" weight=\"bold\" foreground=\"#f4f4f5\">{}</span>\n",
                h_esc
            ));
            continue;
        } else if let Some(h) = trimmed.strip_prefix("### ") {
            let h_esc = inline::format_inline_elements(h);
            out.push_str(&format!(
                "<span size=\"medium\" weight=\"bold\" foreground=\"#e4e4e7\">{}</span>\n",
                h_esc
            ));
            continue;
        } else if let Some(h) = trimmed.strip_prefix("#### ") {
            let h_esc = inline::format_inline_elements(h);
            out.push_str(&format!(
                "<span weight=\"bold\" foreground=\"#d4d4d8\">{}</span>\n",
                h_esc
            ));
            continue;
        }

        // 5. Bullet & Numbered Lists
        if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ ")) {
            let indent_spaces = line.len() - trimmed.len();
            let indent = " ".repeat(indent_spaces);
            let formatted = inline::format_inline_elements(rest);
            out.push_str(&format!("{}• {}\n", indent, formatted));
            continue;
        }

        // 6. Regular line with inline markdown elements
        let formatted_line = inline::format_inline_elements(line);
        out.push_str(&formatted_line);
        out.push('\n');
    }

    // Flush any pending table
    if !table_accumulator.is_empty() {
        out.push_str(&table::render_markdown_table(&table_accumulator));
        out.push('\n');
    }

    out.trim_end().to_string()
}

/// Applies formatted text safely to a GTK4 label.
/// Validates with Pango parser; falls back gracefully to plain text if malformed.
pub fn apply_assistant_markup(label: &gtk4::Label, raw: &str) {
    let pango = format_markdown_to_pango(raw);
    if pango::parse_markup(&pango, '\0').is_ok() {
        label.set_markup(&pango);
    } else {
        label.set_text(raw);
    }
}

/// Renders complete rich markdown into a GTK4 Box with discrete interactive widgets.
/// Text blocks become selectable Pango-formatted labels.
/// Code blocks become styled cards with header language badge and functional Copy button.
pub fn render_markdown_blocks_into(container: &gtk4::Box, raw: &str) {
    // Clear old children
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let blocks = parse_markdown_blocks(raw);

    for block in blocks {
        match block {
            MarkdownBlock::Text(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let label = gtk4::Label::builder()
                    .selectable(true)
                    .wrap(true)
                    .wrap_mode(gtk4::pango::WrapMode::WordChar)
                    .width_chars(1)
                    .xalign(0.0)
                    .css_classes(["bubble-assistant-text"])
                    .build();
                apply_assistant_markup(&label, &text);
                container.append(&label);
            }
            MarkdownBlock::Code { lang, code } => {
                let code_card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                code_card.add_css_class("code-block-container");

                // Header Bar: Language Badge + Spacer + Copy Button
                let header_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
                header_bar.add_css_class("code-block-header");

                let display_lang = if lang.is_empty() {
                    "CODE".to_string()
                } else {
                    lang.to_uppercase()
                };
                let lang_badge = gtk4::Label::builder()
                    .label(&display_lang)
                    .css_classes(["code-block-lang"])
                    .xalign(0.0)
                    .build();
                header_bar.append(&lang_badge);

                let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
                spacer.set_hexpand(true);
                header_bar.append(&spacer);

                let copy_btn = gtk4::Button::builder()
                    .label("Copy")
                    .css_classes(["code-block-copy-btn"])
                    .tooltip_text("Copy snippet to clipboard")
                    .build();

                let code_to_copy = code.clone();
                let btn_clone = copy_btn.clone();
                copy_btn.connect_clicked(move |_| {
                    if let Some(display) = gdk::Display::default() {
                        display.clipboard().set_text(&code_to_copy);
                    }
                    btn_clone.set_label("Copied ✓");
                    let reset_btn = btn_clone.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                        reset_btn.set_label("Copy");
                    });
                });
                header_bar.append(&copy_btn);
                code_card.append(&header_bar);

                // Code Syntax Highlighted Body
                let mut code_pango = String::with_capacity(code.len() * 2);
                code_pango.push_str("<span font_family=\"monospace\" size=\"small\" foreground=\"#d4d4d4\">");
                for (idx, line) in code.lines().enumerate() {
                    if idx > 0 {
                        code_pango.push('\n');
                    }
                    code_pango.push_str(&syntax::highlight_code_line(&lang, line));
                }
                code_pango.push_str("</span>");

                let code_label = gtk4::Label::builder()
                    .selectable(true)
                    .wrap(false)
                    .xalign(0.0)
                    .css_classes(["code-block-content"])
                    .build();

                if pango::parse_markup(&code_pango, '\0').is_ok() {
                    code_label.set_markup(&code_pango);
                } else {
                    code_label.set_text(&code);
                }

                let scroll = gtk4::ScrolledWindow::builder()
                    .hscrollbar_policy(gtk4::PolicyType::Automatic)
                    .vscrollbar_policy(gtk4::PolicyType::Never)
                    .propagate_natural_width(false)
                    .build();
                scroll.set_child(Some(&code_label));
                code_card.append(&scroll);

                container.append(&code_card);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markdown_blocks() {
        let input = "Hello world\n```rust\nfn main() {}\n```\nAfter code";
        let blocks = parse_markdown_blocks(input);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0], MarkdownBlock::Text("Hello world\n".to_string()));
        assert_eq!(
            blocks[1],
            MarkdownBlock::Code {
                lang: "rust".to_string(),
                code: "fn main() {}".to_string(),
            }
        );
        assert_eq!(blocks[2], MarkdownBlock::Text("After code\n".to_string()));
    }

    #[test]
    fn test_parse_unclosed_streaming_code_block() {
        let input = "Intro\n```python\nprint('streaming')";
        let blocks = parse_markdown_blocks(input);
        assert_eq!(blocks.len(), 2);
        assert_eq!(
            blocks[1],
            MarkdownBlock::Code {
                lang: "python".to_string(),
                code: "print('streaming')".to_string(),
            }
        );
    }
}
