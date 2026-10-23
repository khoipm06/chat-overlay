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

    let mut out = glib::markup_escape_text(&working).to_string();

    // Parse inline code and inline math first to protect their contents
    out = format_inline_code(&out);
    out = format_inline_math(&out);
    out = format_kbd_tags(&out);
    out = format_links(&out);
    out = format_del(&out);
    out = format_bold_italic(&out);
    out = format_bold(&out);
    out = format_italic(&out);
    out = format_subscript(&out);
    out = format_superscript(&out);

    if !prefix.is_empty() {
        format!("{}{}", prefix, out)
    } else {
        out
    }
}

fn format_kbd_tags(s: &str) -> String {
    let s = s.replace(
        "&lt;kbd&gt;",
        "<tt><span background=\"#27272a\" foreground=\"#eeeef0\" size=\"smaller\"> ",
    );
    s.replace("&lt;/kbd&gt;", " </span></tt>")
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

fn format_inline_code(s: &str) -> String {
    let parts: Vec<&str> = s.split('`').collect();
    if parts.len().is_multiple_of(2) {
        return s.to_string();
    }
    let mut res = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i % 2 == 1 {
            res.push_str("<tt>");
            res.push_str(p);
            res.push_str("</tt>");
        } else {
            res.push_str(p);
        }
    }
    res
}
