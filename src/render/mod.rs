pub mod blocks;
pub mod inline;
pub mod syntax;
pub mod table;

use gtk4::glib;
use gtk4::pango;

/// Master function compiling raw markdown text to robust, styled Pango markup.
/// If any syntax produces malformed markup, safely falls back to valid XML escaped text.
pub fn format_markdown_to_pango(markdown: &str) -> String {
    // Sanitize interior NUL bytes (`\0`) which violate GLib/C string invariants and cause panic
    let sanitized;
    let safe_markdown = if markdown.contains('\0') {
        sanitized = markdown.replace('\0', "");
        &sanitized
    } else {
        markdown
    };

    let candidate = format_markdown_to_pango_inner(safe_markdown);
    if pango::parse_markup(&candidate, '\0').is_ok() {
        candidate
    } else {
        // Safe fallback: escape all text so Pango parser is 100% guaranteed to succeed
        glib::markup_escape_text(safe_markdown).to_string()
    }
}

fn format_markdown_to_pango_inner(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len() + 256);
    let mut in_code_block = false;
    let mut current_lang = String::new();
    let mut code_accumulator = String::new();

    let mut in_math_block = false;
    let mut math_accumulator = String::new();

    let mut table_accumulator: Vec<&str> = Vec::new();

    for line in markdown.lines() {
        let trimmed = line.trim();

        // 1. Math Block ($$) multi-line start / end
        if trimmed == "$$" {
            // Flush any pending table first
            if !table_accumulator.is_empty() {
                out.push_str(&table::render_markdown_table_to_pango(&table_accumulator));
                out.push('\n');
                table_accumulator.clear();
            }

            if !in_math_block {
                in_math_block = true;
                math_accumulator.clear();
            } else {
                in_math_block = false;
                let escaped = glib::markup_escape_text(math_accumulator.trim());
                out.push_str(&format!(
                    "<span font_family=\"serif\" size=\"large\" style=\"italic\" foreground=\"#a5b4fc\">    {}</span>\n",
                    escaped
                ));
                math_accumulator.clear();
            }
            continue;
        }

        if in_math_block {
            math_accumulator.push_str(line);
            math_accumulator.push('\n');
            continue;
        }

        // 2. Code Block start / end
        if trimmed.starts_with("```") {
            // Flush any pending table first
            if !table_accumulator.is_empty() {
                out.push_str(&table::render_markdown_table_to_pango(&table_accumulator));
                out.push('\n');
                table_accumulator.clear();
            }

            if !in_code_block {
                in_code_block = true;
                let lang = trimmed.trim_start_matches('`').trim();
                current_lang = if lang.is_empty() {
                    "code".to_string()
                } else {
                    lang.to_string()
                };
                let escaped_lang = glib::markup_escape_text(&current_lang);
                // Copilot Chat code container header: subtle clean bar without ascii box art
                out.push_str(&format!(
                    "<span font_family=\"monospace\" size=\"small\" foreground=\"#858585\" background=\"#1e1e1e\"> {} </span>\n<span font_family=\"monospace\" background=\"#18181b\" foreground=\"#d4d4d4\">",
                    escaped_lang
                ));
            } else {
                in_code_block = false;
                let highlighted = syntax::highlight_code_to_pango(&current_lang, &code_accumulator);
                out.push_str(&highlighted);
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

        // 2. Accumulate table rows
        if table::is_table_row(line) {
            table_accumulator.push(line);
            continue;
        } else if !table_accumulator.is_empty() {
            out.push_str(&table::render_markdown_table_to_pango(&table_accumulator));
            out.push('\n');
            table_accumulator.clear();
        }

        // 3. Horizontal Rule
        if trimmed == "---" || trimmed == "***" || trimmed == "___" {
            out.push_str(
                "<span foreground=\"#3f3f46\">────────────────────────────────────────</span>\n",
            );
            continue;
        }

        // 4. Alerts, Callouts & Blockquotes
        if let Some(formatted_quote) = blocks::format_alerts_and_quotes(line) {
            out.push_str(&formatted_quote);
            out.push('\n');
            continue;
        }

        // 5. HTML Details / Summary tags
        if trimmed.starts_with("<details>") || trimmed == "</details>" {
            continue;
        }
        if let Some(summary) = trimmed
            .strip_prefix("<summary>")
            .and_then(|s| s.strip_suffix("</summary>"))
        {
            out.push_str(&format!(
                "<span foreground=\"#a1a1aa\" weight=\"bold\">▸ {}</span>\n",
                glib::markup_escape_text(summary)
            ));
            continue;
        }

        // 6. Section Headers
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
                "<span weight=\"bold\" foreground=\"#d4d4d8\">{}</span>\n",
                h_esc
            ));
            continue;
        }

        // 7. Lists & Task checkboxes
        let indent_spaces = line.len() - line.trim_start().len();
        let indent_str = " ".repeat(indent_spaces);

        if trimmed.starts_with("- [x] ") || trimmed.starts_with("* [x] ") {
            let rest = &trimmed[6..];
            let formatted_rest = inline::format_inline_elements(rest);
            out.push_str(&format!(
                "{}• <span foreground=\"#4ade80\">☑</span> {}\n",
                indent_str, formatted_rest
            ));
            continue;
        } else if trimmed.starts_with("- [ ] ") || trimmed.starts_with("* [ ] ") {
            let rest = &trimmed[6..];
            let formatted_rest = inline::format_inline_elements(rest);
            out.push_str(&format!(
                "{}• <span foreground=\"#71717a\">☐</span> {}\n",
                indent_str, formatted_rest
            ));
            continue;
        } else if let Some(bullet) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            let formatted_bullet = inline::format_inline_elements(bullet);
            out.push_str(&format!("{}• {}\n", indent_str, formatted_bullet));
            continue;
        }

        // Numbered list: 1. Item
        if let Some(dot_pos) = trimmed.find(". ") {
            let prefix = &trimmed[..dot_pos];
            if prefix.chars().all(|c| c.is_ascii_digit()) {
                let item = &trimmed[dot_pos + 2..];
                let formatted_item = inline::format_inline_elements(item);
                out.push_str(&format!(
                    "{}<span foreground=\"#a1a1aa\">{}.</span> {}\n",
                    indent_str, prefix, formatted_item
                ));
                continue;
            }
        }

        // 8. General inline elements
        let formatted_line = inline::format_inline_elements(line);
        out.push_str(&formatted_line);
        out.push('\n');
    }

    // Flush any pending unclosed table
    if !table_accumulator.is_empty() {
        out.push_str(&table::render_markdown_table_to_pango(&table_accumulator));
        out.push('\n');
    }

    // Handle streaming edge-case: unclosed code block
    if in_code_block {
        let highlighted = syntax::highlight_code_to_pango(&current_lang, &code_accumulator);
        out.push_str(&highlighted);
        out.push_str("</span>");
    }

    // Handle streaming edge-case: unclosed math block
    if in_math_block {
        let escaped = glib::markup_escape_text(math_accumulator.trim());
        out.push_str(&format!(
            "<span font_family=\"serif\" size=\"large\" style=\"italic\" foreground=\"#a5b4fc\">    {}</span>",
            escaped
        ));
    }

    out.trim_end().to_string()
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
