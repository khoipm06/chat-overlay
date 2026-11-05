pub use crate::render::apply_assistant_markup;
#[allow(unused_imports)]
pub use crate::render::format_markdown_to_pango;

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

    match name {
        "bash" | "powershell" => {
            let cmd = parsed
                .as_ref()
                .and_then(|v| v.get("command"))
                .and_then(|c| c.as_str())
                .unwrap_or(generic.as_str());
            format!("Terminal: {} ›", truncate_summary(cmd, 36))
        }
        "read" => {
            let path = parsed
                .as_ref()
                .and_then(|v| {
                    v.get("path")
                        .or_else(|| v.get("file"))
                        .or_else(|| v.get("filePath"))
                })
                .and_then(|p| p.as_str())
                .unwrap_or(generic.as_str());
            format!("Read {} ›", truncate_summary(path, 40))
        }
        "edit" => {
            let path = parsed
                .as_ref()
                .and_then(|v| {
                    v.get("path")
                        .or_else(|| v.get("file"))
                        .or_else(|| v.get("filePath"))
                })
                .and_then(|p| p.as_str())
                .unwrap_or(generic.as_str());
            format!("Edited {} ›", truncate_summary(path, 40))
        }
        "write" => {
            let path = parsed
                .as_ref()
                .and_then(|v| {
                    v.get("path")
                        .or_else(|| v.get("file"))
                        .or_else(|| v.get("filePath"))
                })
                .and_then(|p| p.as_str())
                .unwrap_or(generic.as_str());
            format!("Created {} ›", truncate_summary(path, 40))
        }
        _ => format!("Used 1 reference: {} ›", truncate_summary(&generic, 32)),
    }
}

fn summarize_args(parsed: Option<&serde_json::Value>, raw: &str) -> String {
    if let Some(serde_json::Value::Object(map)) = parsed {
        for key in ["command", "path", "pattern", "query", "url", "file"] {
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
    }

    #[test]
    fn test_markdown_rendering_valid_pango() {
        let raw = "# Header 1\nSome **bold** text and `inline_code`\n```rust\nfn main() {}\n```\n| A | B |\n|---|---|\n| 1 | 2 |";
        let pango = format_markdown_to_pango(raw);
        assert!(gtk4::pango::parse_markup(&pango, '\0').is_ok());
    }
}
