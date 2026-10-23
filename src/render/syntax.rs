use gtk4::glib;

/// Syntax highlighter for code blocks with popular programming languages:
/// python, typescript/javascript, shell/bash, json, rust, c/cpp, html/xml, sql
pub fn highlight_code_to_pango(lang: &str, code: &str) -> String {
    let lang_lower = lang.trim().to_lowercase();
    let mut out = String::with_capacity(code.len() + 256);

    for line in code.lines() {
        out.push_str("  ");
        let highlighted_line = match lang_lower.as_str() {
            "python" | "py" => highlight_python_line(line),
            "typescript" | "ts" | "javascript" | "js" => highlight_ts_line(line),
            "bash" | "sh" | "shell" | "zsh" => highlight_sh_line(line),
            "json" => highlight_json_line(line),
            "rust" | "rs" => highlight_rust_line(line),
            _ => glib::markup_escape_text(line).to_string(),
        };
        out.push_str(&highlighted_line);
        out.push('\n');
    }

    out
}

fn highlight_python_line(line: &str) -> String {
    let keywords = [
        "def", "class", "import", "from", "return", "if", "else", "elif", "for", "while", "try",
        "except", "finally", "with", "as", "pass", "break", "continue", "async", "await", "lambda",
        "yield",
    ];
    let builtins = [
        "True", "False", "None", "self", "print", "len", "range", "str", "int", "float", "bool",
        "dict", "list", "set",
    ];
    highlight_generic_keywords(line, &keywords, &builtins, "#")
}

fn highlight_rust_line(line: &str) -> String {
    let keywords = [
        "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "match", "if", "else", "for",
        "while", "loop", "return", "use", "mod", "async", "await", "where", "type", "const",
        "static", "ref", "move", "unsafe",
    ];
    let builtins = [
        "true", "false", "Some", "None", "Ok", "Err", "Self", "String", "Vec", "Option", "Result",
        "u8", "u32", "u64", "usize", "i32", "i64", "f64", "bool",
    ];
    highlight_generic_keywords(line, &keywords, &builtins, "//")
}

fn highlight_ts_line(line: &str) -> String {
    let keywords = [
        "function",
        "const",
        "let",
        "var",
        "return",
        "if",
        "else",
        "for",
        "while",
        "switch",
        "case",
        "break",
        "continue",
        "import",
        "export",
        "from",
        "default",
        "class",
        "interface",
        "type",
        "extends",
        "implements",
        "async",
        "await",
        "new",
        "typeof",
        "instanceof",
        "try",
        "catch",
    ];
    let builtins = [
        "true",
        "false",
        "null",
        "undefined",
        "this",
        "console",
        "Promise",
        "Array",
        "Object",
        "String",
        "Number",
        "Boolean",
        "Date",
    ];
    highlight_generic_keywords(line, &keywords, &builtins, "//")
}

fn highlight_sh_line(line: &str) -> String {
    let keywords = [
        "if", "then", "else", "elif", "fi", "for", "while", "do", "done", "case", "esac",
        "function", "return", "exit", "set",
    ];
    let builtins = [
        "echo", "printf", "cd", "pwd", "ls", "find", "grep", "cat", "mkdir", "rm", "cp", "mv",
        "chmod", "curl", "tar", "gzip",
    ];
    highlight_generic_keywords(line, &keywords, &builtins, "#")
}

fn highlight_json_line(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.starts_with('"') && trimmed.contains(':') {
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            let key = glib::markup_escape_text(parts[0]);
            let rest = highlight_json_value(parts[1]);
            return format!("<span foreground=\"#93c5fd\">{}</span>:{}", key, rest);
        }
    }
    highlight_json_value(line)
}

fn highlight_json_value(val: &str) -> String {
    let escaped = glib::markup_escape_text(val);
    let mut out = escaped.to_string();
    let literals = [
        ("true", "#4ade80"),
        ("false", "#f87171"),
        ("null", "#a1a1aa"),
    ];
    for (lit, col) in literals {
        let pattern = format!("<span foreground=\"{}\"><b>{}</b></span>", col, lit);
        out = out.replace(lit, &pattern);
    }
    out
}

fn highlight_generic_keywords(
    line: &str,
    keywords: &[&str],
    builtins: &[&str],
    comment_prefix: &str,
) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with(comment_prefix) {
        return format!(
            "<span foreground=\"#71717a\" style=\"italic\">{}</span>",
            glib::markup_escape_text(line)
        );
    }

    // Tokenize line respecting strings
    let mut out = String::with_capacity(line.len() + 64);
    let mut in_str = false;
    let mut str_char = '"';
    let mut cur_word = String::new();

    for c in line.chars() {
        if in_str {
            cur_word.push(c);
            if c == str_char {
                in_str = false;
                out.push_str(&format!(
                    "<span foreground=\"#86efac\">{}</span>",
                    glib::markup_escape_text(&cur_word)
                ));
                cur_word.clear();
            }
            continue;
        }

        if c == '"' || c == '\'' || c == '`' {
            if !cur_word.is_empty() {
                out.push_str(&format_token(&cur_word, keywords, builtins));
                cur_word.clear();
            }
            in_str = true;
            str_char = c;
            cur_word.push(c);
            continue;
        }

        if c.is_alphanumeric() || c == '_' {
            cur_word.push(c);
        } else {
            if !cur_word.is_empty() {
                out.push_str(&format_token(&cur_word, keywords, builtins));
                cur_word.clear();
            }
            out.push_str(&glib::markup_escape_text(&c.to_string()));
        }
    }

    if in_str {
        out.push_str(&format!(
            "<span foreground=\"#86efac\">{}</span>",
            glib::markup_escape_text(&cur_word)
        ));
    } else if !cur_word.is_empty() {
        out.push_str(&format_token(&cur_word, keywords, builtins));
    }

    out
}

fn format_token(token: &str, keywords: &[&str], builtins: &[&str]) -> String {
    let escaped = glib::markup_escape_text(token);
    if keywords.contains(&token) {
        // Linear/Raycast magenta/violet keyword tone
        format!(
            "<span foreground=\"#c084fc\" weight=\"bold\">{}</span>",
            escaped
        )
    } else if builtins.contains(&token) {
        // Cyan/amber tone for types/builtins
        format!("<span foreground=\"#38bdf8\">{}</span>", escaped)
    } else if token.chars().all(|c| c.is_ascii_digit() || c == '.') {
        // Numbers
        format!("<span foreground=\"#fbbf24\">{}</span>", escaped)
    } else {
        escaped.to_string()
    }
}
