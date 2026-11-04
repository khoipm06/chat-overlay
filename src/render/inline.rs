use gtk4::glib;

/// Formats inline elements (backticks `code`, **bold**, *italic*, ~~strikethrough~~, [links](url))
pub fn format_inline_elements(line: &str) -> String {
    let mut working = line.to_string();
    let mut prefix = String::new();
    let trimmed = working.trim_start();
    let indent_len = working.len() - trimmed.len();
    let indent = &working[..indent_len];

    // Checkboxes
    if let Some(rest) = trimmed
        .strip_prefix("- [x] ")
        .or_else(|| trimmed.strip_prefix("* [x] "))
    {
        prefix = format!("{}• <span foreground=\"#4ade80\">☑</span> ", indent);
        working = rest.to_string();
    } else if let Some(rest) = trimmed
        .strip_prefix("- [ ] ")
        .or_else(|| trimmed.strip_prefix("* [ ] "))
    {
        prefix = format!("{}• <span foreground=\"#71717a\">☐</span> ", indent);
        working = rest.to_string();
    }

    // Protect inline code blocks
    let mut code_placeholders: Vec<String> = Vec::new();
    let mut without_code = String::with_capacity(working.len());
    let mut rest = working.as_str();

    while let Some(start) = rest.find('`') {
        without_code.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        if let Some(end) = after_start.find('`') {
            let code_content = &after_start[..end];
            let placeholder = format!("__CODEPH{}__", code_placeholders.len());
            let escaped_code = glib::markup_escape_text(code_content);
            code_placeholders.push(format!("<tt>{}</tt>", escaped_code));
            without_code.push_str(&placeholder);
            rest = &after_start[end + 1..];
        } else {
            without_code.push('`');
            rest = after_start;
        }
    }
    without_code.push_str(rest);

    let mut out = glib::markup_escape_text(&without_code).to_string();

    out = format_links(&out);
    out = format_del(&out);
    out = format_bold_italic(&out);
    out = format_bold(&out);
    out = format_italic(&out);

    // Restore protected code blocks
    for (idx, code_markup) in code_placeholders.into_iter().enumerate() {
        let ph = format!("__CODEPH{}__", idx);
        out = out.replace(&ph, &code_markup);
    }

    if !prefix.is_empty() {
        format!("{}{}", prefix, out)
    } else {
        out
    }
}

fn format_links(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(open_bracket) = rest.find('[') {
        result.push_str(&rest[..open_bracket]);
        let after_open = &rest[open_bracket + 1..];

        if let Some(close_bracket) = after_open.find(']') {
            let label = &after_open[..close_bracket];
            let after_close = &after_open[close_bracket + 1..];

            if after_close.starts_with('(') {
                if let Some(close_paren) = after_close.find(')') {
                    let url = &after_close[1..close_paren];
                    let formatted = format!(
                        "<a href=\"{}\"><span foreground=\"#38bdf8\" underline=\"single\">{}</span></a>",
                        url, label
                    );
                    result.push_str(&formatted);
                    rest = &after_close[close_paren + 1..];
                    continue;
                }
            }
        }

        result.push('[');
        rest = after_open;
    }

    result.push_str(rest);
    result
}

fn format_bold_italic(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("***") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 3..];
        if let Some(end) = after_start.find("***") {
            let inner = &after_start[..end];
            out.push_str(&format!("<b><i>{}</i></b>", inner));
            rest = &after_start[end + 3..];
        } else {
            out.push_str("***");
            rest = after_start;
        }
    }
    out.push_str(rest);
    out
}

fn format_bold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("**") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("**") {
            let inner = &after_start[..end];
            out.push_str(&format!("<b>{}</b>", inner));
            rest = &after_start[end + 2..];
        } else {
            out.push_str("**");
            rest = after_start;
        }
    }
    out.push_str(rest);
    out
}

fn format_italic(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find('*') {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        if let Some(end) = after_start.find('*') {
            let inner = &after_start[..end];
            out.push_str(&format!("<i>{}</i>", inner));
            rest = &after_start[end + 1..];
        } else {
            out.push('*');
            rest = after_start;
        }
    }
    out.push_str(rest);
    out
}

fn format_del(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find("~~") {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        if let Some(end) = after_start.find("~~") {
            let inner = &after_start[..end];
            out.push_str(&format!("<s>{}</s>", inner));
            rest = &after_start[end + 2..];
        } else {
            out.push_str("~~");
            rest = after_start;
        }
    }
    out.push_str(rest);
    out
}
