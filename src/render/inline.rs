use gtk4::glib;

pub fn format_inline_elements(line: &str) -> String {
    // 1. Task list checkboxes before XML escape:
    let mut working = line.to_string();
    let mut prefix = String::new();
    let trimmed = working.trim_start();
    let indent_len = working.len() - trimmed.len();
    let indent = &working[..indent_len];

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
            // Unclosed backtick: leave as literal backtick
            without_code.push('`');
            rest = after_start;
        }
    }
    without_code.push_str(rest);

    let mut out = glib::markup_escape_text(&without_code).to_string();

    out = format_inline_math(&out);
    out = format_kbd_tags(&out);
    out = format_links(&out);
    out = format_del(&out);
    out = format_bold_italic(&out);
    out = format_bold(&out);
    out = format_italic(&out);
    out = format_subscript(&out);
    out = format_superscript(&out);

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

fn format_kbd_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let open_tag = "&lt;kbd&gt;";
    let close_tag = "&lt;/kbd&gt;";

    while let Some(start_idx) = rest.find(open_tag) {
        out.push_str(&rest[..start_idx]);
        let after_open = &rest[start_idx + open_tag.len()..];
        if let Some(close_idx) = after_open.find(close_tag) {
            let inner = &after_open[..close_idx];
            out.push_str(
                "<tt><span background=\"#202226\" foreground=\"#e8e9ed\" size=\"smaller\"> ",
            );
            out.push_str(inner);
            out.push_str(" </span></tt>");
            rest = &after_open[close_idx + close_tag.len()..];
        } else {
            // Unmatched opening tag: preserve as escaped text
            out.push_str(open_tag);
            rest = after_open;
            break;
        }
    }
    out.push_str(rest);
    out
}

fn format_links(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start_bracket) = rest.find('[') {
        result.push_str(&rest[..start_bracket]);
        let after_start = &rest[start_bracket + 1..];
        if let Some(end_bracket) = after_start.find(']') {
            let label = &after_start[..end_bracket];
            let after_bracket = &after_start[end_bracket + 1..];
            if after_bracket.starts_with('(') {
                if let Some(end_paren) = after_bracket.find(')') {
                    result.push_str(&format!(
                        "<span foreground=\"#38bdf8\" underline=\"single\">{}</span>",
                        label
                    ));
                    rest = &after_bracket[end_paren + 1..];
                    continue;
                }
            }
        }
        result.push('[');
        rest = after_start;
    }
    result.push_str(rest);
    result
}

fn format_del(s: &str) -> String {
    let parts: Vec<&str> = s.split("~~").collect();
    if parts.len().is_multiple_of(2) {
        return s.to_string();
    }
    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 1 {
            res.push_str("<s>");
            res.push_str(p);
            res.push_str("</s>");
        } else {
            res.push_str(p);
        }
    }
    res
}

fn format_bold_italic(s: &str) -> String {
    let parts: Vec<&str> = s.split("***").collect();
    if parts.len().is_multiple_of(2) {
        return s.to_string();
    }
    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 1 {
            res.push_str("<b><i>");
            res.push_str(p);
            res.push_str("</i></b>");
        } else {
            res.push_str(p);
        }
    }
    res
}

fn format_bold(s: &str) -> String {
    let parts: Vec<&str> = s.split("**").collect();
    if parts.len().is_multiple_of(2) {
        return s.to_string();
    }
    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 1 {
            res.push_str("<b>");
            res.push_str(p);
            res.push_str("</b>");
        } else {
            res.push_str(p);
        }
    }
    res
}

fn format_italic(s: &str) -> String {
    let parts: Vec<&str> = s.split('*').collect();
    if parts.len().is_multiple_of(2) {
        return s.to_string();
    }
    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 1 {
            res.push_str("<i>");
            res.push_str(p);
            res.push_str("</i>");
        } else {
            res.push_str(p);
        }
    }
    res
}

fn format_subscript(s: &str) -> String {
    let mut res = String::with_capacity(s.len() + 16);
    let mut rest = s;
    while let Some(start) = rest.find('~') {
        res.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        if let Some(end) = after_start.find('~') {
            let inner = &after_start[..end];
            if !inner.is_empty() && !inner.contains(' ') && !inner.contains('<') {
                res.push_str("<sub>");
                res.push_str(inner);
                res.push_str("</sub>");
                rest = &after_start[end + 1..];
                continue;
            }
        }
        res.push('~');
        rest = after_start;
    }
    res.push_str(rest);
    res
}

fn format_superscript(s: &str) -> String {
    let mut res = String::with_capacity(s.len() + 16);
    let mut rest = s;
    while let Some(start) = rest.find('^') {
        res.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        if let Some(end) = after_start.find('^') {
            let inner = &after_start[..end];
            // Only convert if inner has no spaces or tags
            if !inner.is_empty() && !inner.contains(' ') && !inner.contains('<') {
                res.push_str("<sup>");
                res.push_str(inner);
                res.push_str("</sup>");
                rest = &after_start[end + 1..];
                continue;
            }
        }
        res.push('^');
        rest = after_start;
    }
    res.push_str(rest);
    res
}

fn format_inline_math(s: &str) -> String {
    let parts: Vec<&str> = s.split('$').collect();
    if parts.len().is_multiple_of(2) {
        return s.to_string();
    }
    // Verify that inner segments look like math formulas, not currency ($10, $20.50) or spaces
    let all_valid = parts.iter().enumerate().all(|(i, p)| {
        if i % 2 == 1 {
            !p.is_empty()
                && !p.starts_with(' ')
                && !p.ends_with(' ')
                && !p
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
        } else {
            true
        }
    });
    if !all_valid {
        return s.to_string();
    }
    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 1 {
            res.push_str("<span font_family=\"serif\" style=\"italic\" foreground=\"#a5b4fc\">");
            res.push_str(p);
            res.push_str("</span>");
        } else {
            res.push_str(p);
        }
    }
    res
}
