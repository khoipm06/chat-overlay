pub mod blocks;
pub mod inline;
pub mod syntax;
pub mod table;

use gtk4::glib;
use gtk4::pango;

/// Formats a complete raw markdown document into valid Pango markup.
/// Translates headings, lists, tables, callouts, and fenced code blocks.
pub fn format_markdown_to_pango(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    let mut in_code_block = false;
    let mut current_lang = String::new();
    let mut code_accumulator = String::new();
    let mut table_accumulator: Vec<&str> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // 1. Code block fence
        if trimmed.starts_with("```") {
            // Flush any pending table first
            if !table_accumulator.is_empty() {
                out.push_str(&table::render_markdown_table(&table_accumulator));
                out.push('\n');
                table_accumulator.clear();
            }

            if !in_code_block {
                in_code_block = true;
                let lang = trimmed.trim_start_matches('`').trim();
                current_lang = if lang.is_empty() {
                    "text".to_string()
                } else {
                    lang.to_string()
                };
                let escaped_lang = glib::markup_escape_text(&current_lang);
                // Clean Copilot header pill
                out.push_str(&format!(
                    "\n<span size=\"small\" weight=\"bold\" foreground=\"#94a3b8\">[ {} ]</span>\n<span font_family=\"monospace\" size=\"small\" foreground=\"#e2e8f0\">",
                    escaped_lang
                ));
            } else {
                in_code_block = false;
                for code_line in code_accumulator.lines() {
                    out.push_str("  ");
                    out.push_str(&syntax::highlight_code_line(&current_lang, code_line));
                    out.push('\n');
                }
                out.push_str("</span>\n");
                code_accumulator.clear();
            }
            continue;
        }

        if in_code_block {
            code_accumulator.push_str(line);
            code_accumulator.push('\n');
            continue;
        }

        // 2. Table row accumulation
        if table::is_table_row(line) {
            table_accumulator.push(line);
            continue;
        } else if !table_accumulator.is_empty() {
            out.push_str(&table::render_markdown_table(&table_accumulator));
            out.push('\n');
            table_accumulator.clear();
        }

        // 3. Horizontal Rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            out.push_str("<span foreground=\"#27272a\">────────────────────────────────────────</span>\n");
            continue;
        }

        // 4. Alerts / Quotes
        if let Some(formatted_quote) = blocks::format_alerts_and_quotes(line) {
            out.push_str(&formatted_quote);
            out.push('\n');
            continue;
        }

        // 5. Headings
        if let Some(h) = trimmed.strip_prefix("# ") {
            let h_esc = inline::format_inline_elements(h);
            out.push_str(&format!(
                "<span size=\"x-large\" weight=\"bold\" foreground=\"#f4f4f5\">{}</span>\n",
                h_esc
            ));
            continue;
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            let h_esc = inline::format_inline_elements(h);
            out.push_str(&format!(
                "<span size=\"large\" weight=\"bold\" foreground=\"#e4e4e7\">{}</span>\n",
                h_esc
            ));
            continue;
        } else if let Some(h) = trimmed.strip_prefix("### ") {
            let h_esc = inline::format_inline_elements(h);
            out.push_str(&format!(
                "<span size=\"medium\" weight=\"bold\" foreground=\"#d4d4d8\">{}</span>\n",
                h_esc
            ));
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

    // Streaming edge case: unclosed code block
    if in_code_block {
        for code_line in code_accumulator.lines() {
            out.push_str("  ");
            out.push_str(&syntax::highlight_code_line(&current_lang, code_line));
            out.push('\n');
        }
        out.push_str("</span>");
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
