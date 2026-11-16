#[cfg(test)]
use crate::render::format_markdown_to_pango;

/// Format duration in seconds to human-readable string: "Xs" or "Xm Ys"
pub fn format_duration_sec(secs: u64) -> String {
    let s = secs.max(1);
    if s < 60 {
        format!("{}s", s)
    } else {
        let m = s / 60;
        let rem_s = s % 60;
        format!("{}m {}s", m, rem_s)
    }
}

/// Truncate and sanitize a single-line summary string for tool call display
fn truncate_summary(s: &str, max_len: usize) -> String {
    let flat = s.replace('\n', " ").replace('\r', "");
    let trimmed = flat.trim();
    if trimmed.chars().count() > max_len {
        format!("{}…", trimmed.chars().take(max_len).collect::<String>())
    } else {
        trimmed.to_string()
    }
}

/// Format a tool call name and JSON arguments into a standardized human-readable action label.
pub fn format_tool_call_label(name: &str, args_json: &str) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_str(args_json).ok();
    let generic = summarize_args(parsed.as_ref(), args_json);
    let lower = name.to_lowercase();

    if lower == "bash" || lower == "powershell" || lower == "terminal" || lower == "exec" || lower == "run_command" {
        let cmd = parsed
            .as_ref()
            .and_then(|v| {
                v.get("command")
                    .or_else(|| v.get("CommandLine"))
                    .or_else(|| v.get("cmd"))
            })
            .and_then(|c| c.as_str())
            .unwrap_or(generic.as_str());
        format!("Terminal: {} ›", truncate_summary(cmd, 36))
    } else if lower == "read" || lower == "view" || lower.contains("read_") || lower.contains("view_") || lower.contains("get_file") {
        let path = parsed
            .as_ref()
            .and_then(|v| {
                v.get("path")
                    .or_else(|| v.get("file"))
                    .or_else(|| v.get("filePath"))
                    .or_else(|| v.get("TargetFile"))
                    .or_else(|| v.get("AbsolutePath"))
            })
            .and_then(|p| p.as_str())
            .unwrap_or(generic.as_str());
        format!("Read {} ›", truncate_summary(path, 40))
    } else if lower == "edit" || lower.contains("edit_") || lower.contains("replace_") || lower.contains("patch") {
        let path = parsed
            .as_ref()
            .and_then(|v| {
                v.get("path")
                    .or_else(|| v.get("file"))
                    .or_else(|| v.get("filePath"))
                    .or_else(|| v.get("TargetFile"))
            })
            .and_then(|p| p.as_str())
            .unwrap_or(generic.as_str());
        format!("Edited {} ›", truncate_summary(path, 40))
    } else if lower == "write" || lower.contains("write_") || lower.contains("create") {
        let path = parsed
            .as_ref()
            .and_then(|v| {
                v.get("path")
                    .or_else(|| v.get("file"))
                    .or_else(|| v.get("filePath"))
                    .or_else(|| v.get("TargetFile"))
            })
            .and_then(|p| p.as_str())
            .unwrap_or(generic.as_str());
        format!("Created {} ›", truncate_summary(path, 40))
    } else if lower == "search" || lower == "grep" || lower == "find" || lower.contains("search") || lower.contains("grep") || lower.contains("find_") || lower.contains("list_") {
        let query = parsed
            .as_ref()
            .and_then(|v| {
                v.get("query")
                    .or_else(|| v.get("Query"))
                    .or_else(|| v.get("pattern"))
                    .or_else(|| v.get("Pattern"))
                    .or_else(|| v.get("SearchPath"))
                    .or_else(|| v.get("path"))
            })
            .and_then(|q| q.as_str())
            .unwrap_or(generic.as_str());
        format!("Search: {} ›", truncate_summary(query, 36))
    } else {
        format!("{}: {} ›", name, truncate_summary(&generic, 32))
    }
}

/// Resolves the bundled Codicon symbolic icon name for a tool invocation.
pub fn tool_icon_name(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower == "bash"
        || lower == "powershell"
        || lower == "terminal"
        || lower == "exec"
        || lower == "run_command"
    {
        "codicon-terminal-symbolic"
    } else if lower == "read"
        || lower == "view"
        || lower.contains("read_")
        || lower.contains("view_")
        || lower.contains("get_file")
    {
        "codicon-file-code-symbolic"
    } else if lower == "edit"
        || lower == "write"
        || lower.contains("edit_")
        || lower.contains("replace_")
        || lower.contains("patch")
        || lower.contains("write_")
    {
        "codicon-edit-symbolic"
    } else if lower == "search"
        || lower == "grep"
        || lower == "find"
        || lower.contains("search")
        || lower.contains("grep")
        || lower.contains("find_")
        || lower.contains("list_")
    {
        "codicon-search-symbolic"
    } else {
        "codicon-sparkle-symbolic"
    }
}

/// Resolves standard icon glyph for tool name according to VS Code Copilot Chat taxonomy.
#[cfg(test)]
pub fn tool_icon_for_name(name: &str) -> &'static str {
    let lower = name.to_lowercase();
    if lower == "bash" || lower == "powershell" || lower == "terminal" || lower == "exec" || lower == "run_command" {
        "⚙"
    } else if lower == "read" || lower == "view" || lower.contains("read_") || lower.contains("view_") || lower.contains("get_file") {
        "📄"
    } else if lower == "edit" || lower == "write" || lower.contains("edit_") || lower.contains("replace_") || lower.contains("patch") || lower.contains("write_") {
        "✏"
    } else if lower == "search" || lower == "grep" || lower == "find" || lower.contains("search") || lower.contains("grep") || lower.contains("find_") || lower.contains("list_") {
        "🔍"
    } else {
        "⚡"
    }
}

/// Strip ANSI escape codes (CSI, OSC, standard escapes) from text for clean rendering
pub fn strip_ansi_escapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next(); // consume '['
                    // Consume until a command byte in 0x40..=0x7E
                    while let Some(&cmd) = chars.peek() {
                        chars.next();
                        if ('\x40'..='\x7e').contains(&cmd) {
                            break;
                        }
                    }
                } else if next == ']' {
                    chars.next(); // consume ']'
                    // Consume until BEL (\x07) or ST (\x1b\\)
                    while let Some(&osc) = chars.peek() {
                        chars.next();
                        if osc == '\x07' {
                            break;
                        }
                        if osc == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                } else {
                    // Other 2-byte escape sequence
                    chars.next();
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn summarize_args(parsed: Option<&serde_json::Value>, raw: &str) -> String {
    if let Some(serde_json::Value::Object(map)) = parsed {
        for key in ["command", "CommandLine", "path", "pattern", "Pattern", "query", "Query", "url", "file", "TargetFile", "SearchPath"] {
            if let Some(v) = map.get(key).and_then(|v| v.as_str()) {
                return v.to_string();
            }
        }
        if let Some((_, first_val)) = map.iter().next() {
            if let Some(s) = first_val.as_str() {
                return s.to_string();
            }
        }
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_sec(0), "1s");
        assert_eq!(format_duration_sec(15), "15s");
        assert_eq!(format_duration_sec(65), "1m 5s");
    }

    #[test]
    fn test_format_tool_call() {
        let label = format_tool_call_label("bash", r#"{"command":"cargo build"}"#);
        assert!(label.contains("cargo build"));

        let label_read = format_tool_call_label("read", r#"{"path":"src/main.rs"}"#);
        assert!(label_read.contains("src/main.rs"));

        let label_search = format_tool_call_label("grep_search", r#"{"Query":"fn main"}"#);
        assert!(label_search.contains("Search: fn main"));

        let label_edit = format_tool_call_label("replace_file_content", r#"{"TargetFile":"src/main.rs"}"#);
        assert!(label_edit.contains("Edited src/main.rs"));
    }

    #[test]
    fn test_strip_ansi_escapes() {
        let raw = "\x1b[32mSuccess\x1b[0m: Finished in \x1b[1;33m0.42s\x1b[0m";
        assert_eq!(strip_ansi_escapes(raw), "Success: Finished in 0.42s");

        let plain = "No escape sequences here";
        assert_eq!(strip_ansi_escapes(plain), plain);

        let osc = "\x1b]0;Title\x07Hello";
        assert_eq!(strip_ansi_escapes(osc), "Hello");
    }

    #[test]
    fn test_tool_icons() {
        assert_eq!(tool_icon_for_name("bash"), "⚙");
        assert_eq!(tool_icon_for_name("terminal"), "⚙");
        assert_eq!(tool_icon_for_name("read"), "📄");
        assert_eq!(tool_icon_for_name("view_file"), "📄");
        assert_eq!(tool_icon_for_name("edit"), "✏");
        assert_eq!(tool_icon_for_name("replace_file_content"), "✏");
        assert_eq!(tool_icon_for_name("grep_search"), "🔍");
        assert_eq!(tool_icon_for_name("find_by_name"), "🔍");
        assert_eq!(tool_icon_for_name("custom_rpc"), "⚡");
    }

    #[test]
    fn test_markdown_rendering_valid_pango() {
        let raw = "# Header 1\nSome **bold** text and `inline_code`\n```rust\nfn main() {}\n```\n| A | B |\n|---|---|\n| 1 | 2 |";
        let pango = format_markdown_to_pango(raw);
        assert!(gtk4::pango::parse_markup(&pango, '\0').is_ok());
    }
}
