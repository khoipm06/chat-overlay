use gtk4::glib;

/// Highlights a line of source code for known languages into clean Pango markup.
/// Adheres strictly to modern dark canvas syntax palette:
/// Keywords: #c586c0 (pink/purple), Types/Builtins: #4fc1ff (cyan), Strings: #ce9178 (orange),
/// Numbers: #b5cea8 (light green), Comments: #6a9955 (green).
pub fn highlight_code_line(lang: &str, line: &str) -> String {
    let lang_lower = lang.trim().to_lowercase();
    match lang_lower.as_str() {
        "rust" | "rs" => highlight_generic(
            line,
            &[
                "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "match", "if", "else",
                "for", "while", "loop", "return", "use", "mod", "async", "await", "where", "type",
                "const", "static", "ref", "move", "unsafe",
            ],
            &[
                "true", "false", "Some", "None", "Ok", "Err", "Self", "String", "Vec", "Option",
                "Result", "u8", "u32", "u64", "usize", "i32", "i64", "f64", "bool",
            ],
            "//",
        ),
        "python" | "py" => highlight_generic(
            line,
            &[
                "def", "class", "import", "from", "return", "if", "else", "elif", "for", "while",
                "try", "except", "finally", "with", "as", "pass", "break", "continue", "async",
                "await", "lambda", "yield",
            ],
            &[
                "True", "False", "None", "self", "print", "len", "range", "str", "int", "float",
                "bool", "dict", "list", "set",
            ],
            "#",
        ),
        "javascript" | "js" | "typescript" | "ts" => highlight_generic(
            line,
            &[
                "function", "const", "let", "var", "return", "if", "else", "for", "while", "switch",
                "case", "break", "continue", "import", "export", "from", "default", "class",
                "interface", "type", "extends", "implements", "async", "await", "new", "typeof",
                "try", "catch",
            ],
            &[
                "true", "false", "null", "undefined", "this", "console", "Promise", "Array", "Object",
                "String", "Number", "Boolean",
            ],
            "//",
        ),
        "go" | "golang" => highlight_generic(
            line,
            &[
                "package", "import", "func", "return", "var", "const", "type", "struct",
                "interface", "if", "else", "for", "range", "go", "chan", "select", "defer",
                "switch", "case", "default", "break", "continue",
            ],
            &[
                "string", "int", "int64", "float64", "bool", "byte", "error", "nil", "true",
                "false", "make", "new", "len", "cap", "append",
            ],
            "//",
        ),
        "bash" | "sh" | "shell" | "zsh" => highlight_generic(
            line,
            &[
                "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
                "function", "return", "exit", "set", "export",
            ],
            &[
                "echo", "printf", "cd", "pwd", "ls", "find", "grep", "cat", "mkdir", "rm", "cp",
                "mv", "chmod", "curl", "tar", "gzip", "sudo",
            ],
            "#",
        ),
        "sql" => highlight_generic(
            line,
            &[
                "SELECT", "FROM", "WHERE", "INSERT", "INTO", "UPDATE", "DELETE", "JOIN",
                "LEFT", "RIGHT", "INNER", "OUTER", "ON", "GROUP", "BY", "ORDER", "HAVING",
                "LIMIT", "CREATE", "TABLE", "ALTER", "DROP", "AND", "OR", "NOT", "AS",
                "select", "from", "where", "insert", "into", "update", "delete", "join",
                "left", "right", "inner", "outer", "on", "group", "by", "order", "having",
                "limit", "create", "table", "alter", "drop", "and", "or", "not", "as",
            ],
            &[
                "COUNT", "SUM", "AVG", "MIN", "MAX", "NULL", "TRUE", "FALSE",
                "count", "sum", "avg", "min", "max", "null", "true", "false",
            ],
            "--",
        ),
        "toml" | "yaml" | "yml" => highlight_config_line(line),
        "json" => highlight_json_line(line),
        _ => glib::markup_escape_text(line).to_string(),
    }
}

fn highlight_config_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return format!(
            "<span foreground=\"#6a9955\" style=\"italic\">{}</span>",
            glib::markup_escape_text(line)
        );
    }
    if let Some(pos) = line.find(':').or_else(|| line.find('=')) {
        let key = &line[..pos];
        let sep = &line[pos..=pos];
        let val = &line[pos + 1..];
        return format!(
            "<span foreground=\"#9cdcfe\" weight=\"bold\">{}</span>{}<span foreground=\"#ce9178\">{}</span>",
            glib::markup_escape_text(key),
            sep,
            glib::markup_escape_text(val)
        );
    }
    glib::markup_escape_text(line).to_string()
}

fn highlight_json_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with('"') && trimmed.contains(':') {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            let key = glib::markup_escape_text(parts[0]);
            let rest = glib::markup_escape_text(parts[1]);
            return format!("<span foreground=\"#9cdcfe\">{}</span>:{}", key, rest);
        }
    }
    glib::markup_escape_text(line).to_string()
}

fn highlight_generic(
    line: &str,
    keywords: &[&str],
    builtins: &[&str],
    comment_prefix: &str,
) -> String {
    let trimmed = line.trim_start();
    let indent_len = line.len() - trimmed.len();
    let indent = &line[..indent_len];

    // Whole line comment
    if trimmed.starts_with(comment_prefix) {
        return format!(
            "{}<span foreground=\"#6a9955\" style=\"italic\">{}</span>",
            indent,
            glib::markup_escape_text(trimmed)
        );
    }

    let mut out = String::with_capacity(line.len() * 2);
    out.push_str(indent);

    let mut in_string = false;
    let mut string_char = ' ';
    let mut cur_token = String::new();

    let mut chars = trimmed.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_string {
            cur_token.push(ch);
            if ch == string_char {
                out.push_str(&format!(
                    "<span foreground=\"#ce9178\">{}</span>",
                    glib::markup_escape_text(&cur_token)
                ));
                cur_token.clear();
                in_string = false;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            if !cur_token.is_empty() {
                out.push_str(&format_token(&cur_token, keywords, builtins));
                cur_token.clear();
            }
            in_string = true;
            string_char = ch;
            cur_token.push(ch);
            continue;
        }

        // Check for inline comment start
        if !comment_prefix.is_empty() && ch == comment_prefix.chars().next().unwrap() {
            let mut matched = true;
            let mut comment_rest = String::new();
            comment_rest.push(ch);
            for p_ch in comment_prefix.chars().skip(1) {
                if chars.peek() == Some(&p_ch) {
                    comment_rest.push(chars.next().unwrap());
                } else {
                    matched = false;
                    break;
                }
            }
            if matched {
                if !cur_token.is_empty() {
                    out.push_str(&format_token(&cur_token, keywords, builtins));
                    cur_token.clear();
                }
                for c in chars.by_ref() {
                    comment_rest.push(c);
                }
                out.push_str(&format!(
                    "<span foreground=\"#6a9955\" style=\"italic\">{}</span>",
                    glib::markup_escape_text(&comment_rest)
                ));
                break;
            } else {
                cur_token.push_str(&comment_rest);
                continue;
            }
        }

        if ch.is_alphanumeric() || ch == '_' {
            cur_token.push(ch);
        } else {
            if !cur_token.is_empty() {
                out.push_str(&format_token(&cur_token, keywords, builtins));
                cur_token.clear();
            }
            out.push_str(&glib::markup_escape_text(&ch.to_string()));
        }
    }

    if !cur_token.is_empty() {
        if in_string {
            out.push_str(&format!(
                "<span foreground=\"#ce9178\">{}</span>",
                glib::markup_escape_text(&cur_token)
            ));
        } else {
            out.push_str(&format_token(&cur_token, keywords, builtins));
        }
    }

    out
}

fn format_token(token: &str, keywords: &[&str], builtins: &[&str]) -> String {
    let escaped = glib::markup_escape_text(token);
    if keywords.contains(&token) {
        format!("<span foreground=\"#c586c0\" weight=\"bold\">{}</span>", escaped)
    } else if builtins.contains(&token) {
        format!("<span foreground=\"#4fc1ff\">{}</span>", escaped)
    } else if token.chars().all(|c| c.is_ascii_digit() || c == '.') {
        format!("<span foreground=\"#b5cea8\">{}</span>", escaped)
    } else {
        escaped.to_string()
    }
}
