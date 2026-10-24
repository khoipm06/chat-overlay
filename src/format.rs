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

    match name {
        "bash" | "powershell" => {
            let cmd = parsed
                .as_ref()
                .and_then(|v| v.get("command"))
                .and_then(|c| c.as_str())
                .unwrap_or(args_json);
            format!("Ran command: {} ›", truncate_summary(cmd, 36))
        }
        "read" => {
            let path = parsed
                .as_ref()
                .and_then(|v| v.get("path").or_else(|| v.get("file")).or_else(|| v.get("filePath")))
                .and_then(|p| p.as_str())
                .unwrap_or(args_json);
            format!("Read file: {} ›", truncate_summary(path, 40))
        }
        "edit" => {
            let path = parsed
                .as_ref()
                .and_then(|v| v.get("path").or_else(|| v.get("file")).or_else(|| v.get("filePath")))
                .and_then(|p| p.as_str())
                .unwrap_or(args_json);
            format!("Edited file: {} ›", truncate_summary(path, 40))
        }
        "write" => {
            let path = parsed
                .as_ref()
                .and_then(|v| v.get("path").or_else(|| v.get("file")).or_else(|| v.get("filePath")))
                .and_then(|p| p.as_str())
                .unwrap_or(args_json);
            format!("Wrote file: {} ›", truncate_summary(path, 40))
        }
        "grep" | "find" => {
            let query = parsed
                .as_ref()
                .and_then(|v| {
                    v.get("query")
                        .or_else(|| v.get("pattern"))
                        .or_else(|| v.get("text"))
                        .or_else(|| v.get("findText"))
                })
                .and_then(|q| q.as_str())
                .unwrap_or(args_json);
            format!("Searched: \"{}\" ›", truncate_summary(query, 32))
        }
        "web_search" => {
            let query = parsed
                .as_ref()
                .and_then(|v| {
                    v.get("query").or_else(|| v.get("queries")).and_then(|q| {
                        if let Some(s) = q.as_str() {
                            Some(s.to_string())
                        } else {
                            q.as_array().map(|arr| arr.first().and_then(|v| v.as_str()).unwrap_or("").to_string())
                        }
                    })
                })
                .unwrap_or_else(|| truncate_summary(args_json, 32));
            format!("Searched web: \"{}\" ›", truncate_summary(&query, 32))
        }
        "fetch_content" => {
            let target = parsed
                .as_ref()
                .and_then(|v| v.get("url").or_else(|| v.get("urls")).and_then(|u| {
                    if let Some(s) = u.as_str() {
                        Some(s.to_string())
                    } else {
                        u.as_array().map(|arr| arr.first().and_then(|v| v.as_str()).unwrap_or("").to_string())
                    }
                }))
                .unwrap_or_else(|| truncate_summary(args_json, 32));
            format!("Fetched: {} ›", truncate_summary(&target, 36))
        }
        "get_search_content" | "source_check" => {
            let target = parsed
                .as_ref()
                .and_then(|v| v.get("query").or_else(|| v.get("url")).or_else(|| v.get("claim")).and_then(|s| s.as_str()))
                .unwrap_or(args_json);
            format!("Checked source: {} ›", truncate_summary(target, 32))
        }
        "subagent" => {
            let task = parsed
                .as_ref()
                .and_then(|v| v.get("task").or_else(|| v.get("agent")).and_then(|s| s.as_str()))
                .unwrap_or(args_json);
            format!("Delegated: {} ›", truncate_summary(task, 32))
        }
        "todo" => {
            let action = parsed
                .as_ref()
                .and_then(|v| v.get("action").and_then(|s| s.as_str()))
                .unwrap_or("manage");
            let subject = parsed
                .as_ref()
                .and_then(|v| v.get("subject").or_else(|| v.get("description")).and_then(|s| s.as_str()))
                .unwrap_or("");
            if subject.is_empty() {
                format!("Todo: {} ›", action)
            } else {
                format!("Todo {}: {} ›", action, truncate_summary(subject, 28))
            }
        }
        "fusion_reason" | "fusion_investigate" | "fusion_research" | "fusion_validate" => {
            let prompt = parsed
                .as_ref()
                .and_then(|v| v.get("objective").or_else(|| v.get("prompt")).or_else(|| v.get("claim")).and_then(|s| s.as_str()))
                .unwrap_or(args_json);
            let action_name = match name {
                "fusion_reason" => "Reasoned",
                "fusion_investigate" => "Investigated",
                "fusion_research" => "Researched",
                _ => "Validated",
            };
            format!("{}: {} ›", action_name, truncate_summary(prompt, 32))
        }
        "ctx_execute" | "ctx_execute_file" => {
            let target = parsed
                .as_ref()
                .and_then(|v| v.get("path").or_else(|| v.get("language")).and_then(|s| s.as_str()))
                .unwrap_or(args_json);
            format!("Sandbox exec: {} ›", truncate_summary(target, 32))
        }
        "ctx_search" | "ctx_index" | "ctx_fetch_and_index" => {
            let q = parsed
                .as_ref()
                .and_then(|v| v.get("query").or_else(|| v.get("source")).or_else(|| v.get("url")).and_then(|s| s.as_str()))
                .unwrap_or(args_json);
            format!("Context op: {} ›", truncate_summary(q, 32))
        }
        "bg_run" | "bg_status" | "bg_logs" | "bg_result" | "bg_kill" => {
            let target = parsed
                .as_ref()
                .and_then(|v| v.get("name").or_else(|| v.get("taskId")).or_else(|| v.get("command")).and_then(|s| s.as_str()))
                .unwrap_or(args_json);
            format!("Background task: {} ›", truncate_summary(target, 30))
        }
        "mcp" | "mcp__playwright" => {
            let action = parsed
                .as_ref()
                .and_then(|v| v.get("action").or_else(|| v.get("tool")).and_then(|s| s.as_str()))
                .unwrap_or("call");
            format!("MCP action: {} ›", truncate_summary(action, 30))
        }
        "ask_user_question" => {
            let q = parsed
                .as_ref()
                .and_then(|v| {
                    v.get("question").and_then(|s| s.as_str()).or_else(|| {
                        v.get("questions")
                            .and_then(|arr| arr.as_array())
                            .and_then(|a| a.first())
                            .and_then(|item| item.get("question"))
                            .and_then(|s| s.as_str())
                    })
                })
                .unwrap_or(args_json);
            format!("Prompted user: \"{}\" ›", truncate_summary(q, 32))
        }
        _ => {
            // General tools (e.g. web_search, fetch_content, subagent, etc.)
            let summary = if let Some(ref obj) = parsed {
                if let Some(cmd) = obj.get("command").and_then(|c| c.as_str()) {
                    truncate_summary(cmd, 30)
                } else if let Some(path) = obj.get("path").and_then(|p| p.as_str()) {
                    truncate_summary(path, 30)
                } else if let Some(q) = obj.get("query").or_else(|| obj.get("queries")).and_then(|q| {
                    if let Some(s) = q.as_str() {
                        Some(s.to_string())
                    } else {
                        q.as_array().map(|arr| arr.first().and_then(|v| v.as_str()).unwrap_or("").to_string())
                    }
                }) {
                    truncate_summary(&q, 30)
                } else if let Some(url) = obj.get("url").and_then(|u| u.as_str()) {
                    truncate_summary(url, 30)
                } else if let Some(first_val) = obj.as_object().and_then(|m| m.values().next()).and_then(|v| v.as_str()) {
                    truncate_summary(first_val, 30)
                } else {
                    String::new()
                }
            } else {
                truncate_summary(args_json, 24)
            };

            if summary.is_empty() {
                format!("Tool: {} ›", name)
            } else {
                format!("Tool {}: {} ›", name, summary)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::pango;

    #[test]
    fn test_plain_text() {
        let text = "Hello world! How are you?";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert_eq!(markup, text);
    }

    #[test]
    fn test_inline_typography_and_math() {
        let text = "Text formatting: **bold**, *italic*, ***bold-italic***, ~~strikethrough~~, `inline_code()`, and [hyperlink](https://example.com). Subscript: H~2~O, Superscript: E = mc^2^, Kbd: <kbd>Ctrl</kbd>. Math: $e^{i\\pi} + 1 = 0$.";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("<b>bold</b>"));
        assert!(markup.contains("<i>italic</i>"));
        assert!(markup.contains("<s>strikethrough</s>"));
        assert!(markup.contains("<tt>inline_code()</tt>"));
        assert!(markup.contains("<sub>2</sub>"));
        assert!(markup.contains("<sup>2</sup>"));
        assert!(
            markup.contains("<span foreground=\"#38bdf8\" underline=\"single\">hyperlink</span>")
        );
    }

    #[test]
    fn test_table_rendering() {
        let text = "| ID | Name | Port |\n| :--- | :--- | ---: |\n| srv-1 | Auth | 8080 |\n| srv-2 | DB | 5432 |";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains('┌'));
        assert!(markup.contains('┤'));
        assert!(markup.contains('└'));
        assert!(markup.contains("Auth"));
    }

    #[test]
    fn test_blockquotes_and_alerts() {
        let text = "> Standard quote\n> > Nested quote\n> [!NOTE]\n> Notice message\n> [!WARNING]\n> Warning alert";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("│ Standard quote"));
        assert!(markup.contains("│ │ Nested quote"));
        assert!(markup.contains("ℹ Note"));
        assert!(markup.contains("⚠ Warning"));
    }

    #[test]
    fn test_lists_and_task_states() {
        let text = "- Root tier\n  - Secondary tier\n1. First step\n2. Second step\n- [x] Completed task\n- [ ] Pending task";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("• Root tier"));
        assert!(markup.contains("1."));
        assert!(markup.contains("☑"));
        assert!(markup.contains("Completed task"));
        assert!(markup.contains("☐"));
        assert!(markup.contains("Pending task"));
    }

    #[test]
    fn test_code_blocks_and_syntax() {
        let text = "```python\ndef hello(name: str) -> None:\n    print(f'Hello {name}')\n```";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("python"));
        assert!(markup.contains("def"));
        assert!(markup.contains("print"));
    }

    #[test]
    fn test_diagrams_and_details() {
        let text = "<details>\n<summary>Click to view</summary>\nContent here\n</details>\n```mermaid\nflowchart LR\nA --> B\n```";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("▸ Click to view"));
        assert!(markup.contains("mermaid"));
    }

    #[test]
    fn test_unclosed_streaming_safety() {
        let text = "Streaming `unclosed inline\n```rust\nfn main() {";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
    }

    #[test]
    fn test_pango_fuzz_markup_injection() {
        let fuzz_cases = [
            "<script>alert(1)</script>",
            "& < > \" ' `",
            "**bold <span color='red'>unsafe</span>**",
            "`code with <tags> & ampersands`",
            "```html\n<div class=\"test\">&amp;</div>\n```",
            "```rust\nfn test<T: Display>(x: &T) -> bool { x < 10 && x > 2 }\n```",
            "```json\n{\"key\": \"<value> & 123\"}\n```",
            "```python\n# comment with <tag> & symbols\nx = 'str with <tags> & \" quotes'\n```",
            "```bash\necho '<div class=\"foo\">' && ls -la | grep '<test>'\n```",
            "```\nGeneric code block with <tag> & &amp; characters\n```",
            "| Col 1 & <tag> | Col 2 > |\n| --- | --- |\n| <b>x</b> | a & b |",
            "> [!NOTE]\n> Note with <unsafe> & markup",
            "- Item with <tag> & unclosed `code",
            "H~2<O~ and E=mc^2<3^",
            "[link <test>](https://example.com?a=1&b=2)",
            "Text with unmatched tags: <span>",
            "Math: $x < y && y > z$",
            "<kbd>Ctrl + <tag></kbd>",
            "# Header with <tag> & amp",
            "## H2 with **bold & <tag>**",
        ];

        for case in fuzz_cases {
            let markup = format_markdown_to_pango(case);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Pango parse failed on input: {:?}\nGenerated markup: {:?}\nError: {:?}",
                case,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_table_unicode_alignment() {
        let text = "| Ký tự | Phiên âm | Mô tả |\n| :--- | :--- | ---: |\n| Tiếng Việt | Chữ Quốc Ngữ | Đầy đủ dấu thanh sắc huyền hỏi ngã nặng |\n| 日本語 | にほんご | Kanji và Kana (toàn giác) |\n| 🦀 Rust | ロスト | Cua lập trình an toàn bộ nhớ |";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("日本語"));
        assert!(markup.contains("Tiếng Việt"));
        assert!(markup.contains("🦀 Rust"));
    }

    #[test]
    fn test_format_duration_sec() {
        assert_eq!(format_duration_sec(0), "1s");
        assert_eq!(format_duration_sec(5), "5s");
        assert_eq!(format_duration_sec(59), "59s");
        assert_eq!(format_duration_sec(60), "1m 0s");
        assert_eq!(format_duration_sec(125), "2m 5s");
    }

    #[test]
    fn test_format_tool_call_label() {
        // bash
        let b = format_tool_call_label("bash", r#"{"command":"cargo check"}"#);
        assert_eq!(b, "Ran command: cargo check ›");

        // read
        let r = format_tool_call_label("read", r#"{"path":"src/main.rs"}"#);
        assert_eq!(r, "Read file: src/main.rs ›");

        // edit
        let e = format_tool_call_label("edit", r#"{"path":"src/overlay.rs"}"#);
        assert_eq!(e, "Edited file: src/overlay.rs ›");

        // write
        let w = format_tool_call_label("write", r#"{"path":"src/tray.rs"}"#);
        assert_eq!(w, "Wrote file: src/tray.rs ›");

        // grep / find
        let g = format_tool_call_label("grep", r#"{"pattern":"refreshToken"}"#);
        assert_eq!(g, "Searched: \"refreshToken\" ›");

        // web_search
        let ws = format_tool_call_label("web_search", r#"{"query":"Rust GUI gtk4"}"#);
        assert_eq!(ws, "Searched web: \"Rust GUI gtk4\" ›");

        // fetch_content
        let fc = format_tool_call_label("fetch_content", r#"{"url":"https://crates.io/api/v1/crates"}"#);
        assert_eq!(fc, "Fetched: https://crates.io/api/v1/crates ›");

        // subagent
        let sa = format_tool_call_label("subagent", r#"{"task":"run security audit"}"#);
        assert_eq!(sa, "Delegated: run security audit ›");

        // todo
        let td = format_tool_call_label("todo", r#"{"action":"create","subject":"Implement test suite"}"#);
        assert_eq!(td, "Todo create: Implement test suite ›");

        // fusion
        let fr = format_tool_call_label("fusion_reason", r#"{"prompt":"Analyze architectural trade-offs"}"#);
        assert_eq!(fr, "Reasoned: Analyze architectural trade-offs ›");

        // ctx_execute
        let ce = format_tool_call_label("ctx_execute", r#"{"language":"typescript"}"#);
        assert_eq!(ce, "Sandbox exec: typescript ›");

        // bg_run
        let bg = format_tool_call_label("bg_run", r#"{"name":"Build Cargo Binary"}"#);
        assert_eq!(bg, "Background task: Build Cargo Binary ›");

        // mcp
        let m = format_tool_call_label("mcp", r#"{"action":"install","url":"http://localhost:3000"}"#);
        assert_eq!(m, "MCP action: install ›");

        // ask_user_question
        let au = format_tool_call_label("ask_user_question", r#"{"questions":[{"question":"Choose an approach?"}]}"#);
        assert_eq!(au, "Prompted user: \"Choose an approach?\" ›");

        // fallback generic
        let f = format_tool_call_label("custom_tool", r#"{"param":"value"}"#);
        assert_eq!(f, "Tool custom_tool: value ›");
    }

    #[test]
    fn test_fuzz_inline_formatting_edge_cases() {
        let cases = [
            // Unclosed inline elements
            "Unclosed bold **bold text without end",
            "Unclosed italic *italic text without end",
            "Unclosed strikethrough ~~strike text without end",
            "Unclosed inline code `code without end",
            "Unclosed math $x + y = z without end",
            "Unclosed subscript H~2 without end",
            "Unclosed superscript x^2 without end",
            "Empty inline code `` and empty bold **** and empty italic **",
            "Empty math $$",
            "Nested symbols: `foo *bar* baz`",
            "Symbols inside link text: [**Bold Link**](https://example.com)",
            "Multiple codes on one line: `c1` and `c2` and `c3`",
            "Multiple math on one line: $m1$ and $m2$",
            "Special chars in code: `<a> && <b>`",
            "Special chars in math: `$a < b && c > d$`",
            "Escaped html entities: &amp; &lt; &gt; &quot; &#39;",
            // Multi-line codeblock with unclosed fence
            "```rust\nfn main() {\n    let x = 1;\n",
            // Multi-line codeblock with syntax highlighting and special chars
            "```json\n{\n  \"message\": \"<hello> & goodbye\",\n  \"count\": 42\n}\n```",
            // Code block without language identifier
            "```\nPlain code block with <tags> & &amp;\n```",
            // Heading with complex inline elements
            "# Heading with `code`, **bold**, *italic*, and [link](https://test.com)",
            // Task list with inline elements
            "- [x] Task **completed** with `code` and <tags>",
            "- [ ] Task *pending* with $math$",
            // Blockquote with nested markdown
            "> Quote with `inline_code()` and **bold text**\n> > Nested quote with ~~strike~~",
            // Complex math block / dollar sign edge cases
            "Price is $10.99 and another price is $20.50.",
            "Math with escaped chars: $\\frac{a}{b} < \\sqrt{c} & d > 0$",
            // Nested code blocks or unmatched ticks
            "Here are four ticks ````code```` and five `````test`````",
            // Triple backtick on one line
            "```inline code inside triple```",
            // Mixed tags and markdown
            "<span foreground=\"#ff0000\">Raw HTML tag</span>",
            "<div><span>Nested HTML</span></div>",
            // Malformed XML entities
            "Entity like &notanentity; or incomplete &amp or &",
        ];

        for c in cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Pango parse failed on input:\n{}\nGenerated markup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_syntax_highlighting_all_languages() {
        let code_samples = [
            ("rust", "fn test<T: Display>(val: Option<Vec<T>>) -> Result<(), String> {\n    let s = \"hello \\\"world\\\"\";\n    if true && false || 10 > 5 {\n        println!(\"OK: {}\", s);\n    }\n    Ok(())\n}"),
            ("python", "def func(a: int = 10, b: str = 'test') -> bool:\n    \"\"\"Docstring with <tags> & symbols\"\"\"\n    # Comment with <tag>\n    return True if a > 0 else False"),
            ("typescript", "export async function handleRequest(req: Request): Promise<Response> {\n  const data: Array<string> = ['<tag>', \"&amp;\"];\n  // comment\n  return new Response(JSON.stringify({ ok: true, count: 12.5 }));\n}"),
            ("bash", "#!/bin/bash\nset -euo pipefail\nif [ \"$1\" != \"\" ] && [ -f \"$FILE\" ]; then\n  echo \"<output>: $1\" | grep -v 'pattern'\nfi"),
            ("json", "{\n  \"nested\": {\n    \"array\": [true, false, null, 123.45, \"string with <tags> & &amp;\"]\n  },\n  \"valid\": true\n}"),
            ("sql", "SELECT id, name, count(*) FROM users WHERE age > 18 AND status = 'active' GROUP BY id;"),
            ("unknown_lang", "<xml><node attr=\"val\">Test & Demo</node></xml>"),
        ];

        for (lang, code) in code_samples {
            let highlighted = crate::render::syntax::highlight_code_to_pango(lang, code);
            let wrapped = format!("<span font_family=\"monospace\">{}</span>", highlighted);
            let res = pango::parse_markup(&wrapped, '\0');
            assert!(
                res.is_ok(),
                "Pango parse failed on highlighted code for lang: {}\nOutput:\n{}\nError: {:?}",
                lang,
                wrapped,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_combinatorial_markdown_stress() {
        // Generate complex combinations of markdown elements
        let tokens = [
            "**bold**",
            "*italic*",
            "***bold-italic***",
            "~~strike~~",
            "`code <tag> &`",
            "$x < y && z > 0$",
            "H~2~O",
            "x^2^",
            "[Link](https://example.com)",
            "<kbd>Ctrl</kbd>",
            "& < > \" '",
            "🦀 Tiếng Việt có dấu 🚀",
        ];

        // Test multi-token lines
        for i in 0..tokens.len() {
            for j in 0..tokens.len() {
                let combined_line = format!("Start {} middle {} end", tokens[i], tokens[j]);
                let markup = format_markdown_to_pango(&combined_line);
                let res = pango::parse_markup(&markup, '\0');
                assert!(
                    res.is_ok(),
                    "Combinatorial markdown failed on: {}\nOutput: {}\nError: {:?}",
                    combined_line,
                    markup,
                    res.err()
                );
            }
        }
    }

    #[test]
    fn test_fuzz_pseudo_random_generative_markdown_stream() {
        // Pseudo-random generative stress test with PRNG LCG algorithm (no external rand crate needed)
        let fragments = [
            "# ", "## ", "### ", "- ", "* ", "1. ", "- [x] ", "- [ ] ",
            "> ", "> > ", "> [!NOTE]\n", "> [!WARNING]\n",
            "```rust\n", "```python\n", "```json\n", "```\n", "```",
            "**", "*", "***", "~~", "`", "$", "~", "^",
            "[Link text](https://example.com/api?a=1&b=2)",
            "<kbd>", "</kbd>", "<details>", "</details>", "<summary>", "</summary>",
            "| col 1 | col 2 |\n| --- | --- |\n| a | b |\n",
            "---", "***", "___",
            "<tag>", "</tag>", "<div attr=\"test\">", "</div>",
            "&", "<", ">", "\"", "'", "🦀 Tiếng Việt 🚀 漢字",
            "text with newline\n", "    indented code line\n",
        ];

        let mut seed: u64 = 0xDEADBEEFCAFE;
        let mut prng = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 32) as usize
        };

        // Generate 300 highly varied, random pseudo-markdown payloads
        for iter in 0..300 {
            let num_pieces = (prng() % 15) + 3;
            let mut payload = String::new();
            for _ in 0..num_pieces {
                let idx = prng() % fragments.len();
                payload.push_str(fragments[idx]);
                if prng() % 5 == 0 {
                    payload.push(' ');
                }
            }

            let markup = format_markdown_to_pango(&payload);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Generative stress test failed at iter {}:\nInput:\n{:?}\nMarkup:\n{:?}\nError: {:?}",
                iter,
                payload,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_multiline_math_and_code_blocks() {
        let cases = [
            // Multiline math equations with LaTeX matrix, fraction, exponents, and special chars
            "Here is a complex math formula:\n$$\n\\begin{pmatrix}\na & b \\\\\nc & d\n\\end{pmatrix}\n\\times \\vec{x} = \\lambda \\vec{x}\n$$\nAnd normal prose.",
            // Double dollar math inline and multiline
            "Formula $$\\int_{0}^{\\infty} e^{-x^2} dx = \\frac{\\sqrt{\\pi}}{2}$$ inside paragraph",
            // Code block with multiple backticks in code body
            "```markdown\nHere is how to write a codeblock:\n````rust\nfn main() {}\n````\n```",
            // Empty code fence
            "```rust\n```",
            // Code fence with strange language string
            "```c++ / c# & <generic>\nint main() { return 0; }\n```",
            // Code fence followed immediately by another code fence
            "```python\nprint(1)\n```\n```rust\nprintln!(\"2\");\n```",
            // Incomplete / ragged tables with varying column numbers
            "| A | B |\n| --- | --- |\n| 1 |\n| 1 | 2 | 3 | 4 |\n| | |",
            // Table with zero data rows
            "| A | B |\n| --- | --- |",
            // Nested alerts & callouts
            "> [!NOTE]\n> Alert line 1\n> Alert line 2 with `code`\n> > Nested quote\n> > > Triple nested quote",
            // Mixed HTML comments and entities
            "Text with <!-- comment --> and entities &copy; &reg; &euro; &amp; &lt;",
            // Highly repetitive tags (pathological nesting attack)
            "************************************",
            "~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~",
            "````````````````````````````````````",
            "$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$",
            // Unclosed streaming math block
            "$$\n\\sum_{i=1}^{n} x_i = X\n",
        ];

        for c in cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Pango parse failed on case:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_extreme_pathological_markdown_inputs() {
        let extreme_cases = [
            // Deeply nested list items (20 tiers)
            "                                        - Level 20 nested list item",
            // Unordered list with numeric content
            "- 1.2.3.4 version numbering in bullet",
            // Code block with zero-width characters and RTL / Hebrew / Arabic
            "```text\n\u{200B}\u{200C}\u{200D}שלום עולם مرحبا بالعالم\n```",
            // Tables with escaped pipes and unbalanced backticks
            "| Command | Syntax |\n| --- | --- |\n| Grep | `grep \\| sort` |\n| Cat | `cat file` |",
            // Pathological header with trailing hashes and formatting
            "####### Header Level 7 (standard only defines 1-6) with **bold**",
            // Consecutive horizontal rules
            "---\n***\n___\n---\n***",
            // Link with query params, port, fragments and brackets inside
            "[Complex URL](https://example.com:8443/path/to/resource?param1=val%201&param2=a[b]#section_3)",
            // Task checkboxes with strange casings and marks
            "- [X] Uppercase checked\n- [-] Dash checked\n- [?] Question checked",
            // Blockquote containing table and code fence
            "> Blockquote containing table:\n> | H1 | H2 |\n> | --- | --- |\n> | V1 | V2 |\n> And code:\n> ```\n> echo 1\n> ```",
            // Text with raw control characters (null, bell, backspace, escape)
            "Control chars: \u{0000}\u{0007}\u{0008}\u{001B} sanitized cleanly",
        ];

        for case in extreme_cases {
            let markup = format_markdown_to_pango(case);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Pango parse failed on extreme case:\n{}\nGenerated markup:\n{}\nError: {:?}",
                case,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_massive_payload_stress() {
        // Stress test with 10,000 lines of complex markdown (simulate long session)
        let mut big_doc = String::with_capacity(1024 * 1024);
        for i in 0..500 {
            big_doc.push_str(&format!("# Section {}\n\n", i));
            big_doc.push_str("Paragraph with **bold**, *italic*, `inline code`, and $E=mc^2$.\n\n");
            big_doc.push_str("```rust\nfn demo() -> i32 {\n    let val = 42;\n    val\n}\n```\n\n");
            big_doc.push_str("| Metric | Value | Status |\n| :--- | :--- | :---: |\n| CPU | 24% | OK |\n| Mem | 1.2GB | Normal |\n\n");
            big_doc.push_str("> [!NOTE]\n> Status updated in real time.\n\n");
            big_doc.push_str("$$\n\\int_{-\\infty}^{+\\infty} e^{-x^2} dx = \\sqrt{\\pi}\n$$\n\n");
            big_doc.push_str("- [x] Completed task\n- [ ] Pending task\n\n");
        }

        let start = std::time::Instant::now();
        let markup = format_markdown_to_pango(&big_doc);
        let elapsed = start.elapsed();

        assert!(elapsed.as_millis() < 500, "Rendering 500 blocks took too long: {:?}", elapsed);
        assert!(pango::parse_markup(&markup, '\0').is_ok(), "Pango parse failed on massive payload");
    }

    #[test]
    fn test_fuzz_latex_and_ascii_math_delimiters() {
        let math_cases = [
            // Matrix transpose, subscripts, superscripts, accents
            "$$\n\\mathbf{A}^{T} \\mathbf{A} = \\mathbf{I}_{n}\n$$",
            // Greek symbols and arrows
            "Inline $\\alpha, \\beta, \\gamma, \\theta, \\lambda, \\pi, \\sigma, \\omega$",
            // Limits, integrals, sums, products
            "$$\n\\lim_{x \\to 0} \\frac{\\sin x}{x} = 1\n$$",
            "$$\n\\sum_{n=1}^{\\infty} \\frac{1}{n^2} = \\frac{\\pi^2}{6}\n$$",
            "$$\n\\prod_{i=1}^{k} (1 - p_i) \\le e^{-\\sum p_i}\n$$",
            // Multiple math segments on one line separated by formatting
            "Let $a < b$ and **$c > d$**, then `f(x)` satisfies $\\forall x \\in \\mathbb{R}$.",
            // Math with brackets and curly braces
            "$$\n\\{ x \\in \\mathbb{Z} \\mid x \\equiv 0 \\pmod{2} \\}\n$$",
            // Dollar with escaped chars
            "$\\\\ \\{ \\} \\$ % & # _ ^$",
        ];

        for c in math_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Pango parse failed on LaTeX math case:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_nested_language_payloads() {
        // Diverse code payloads containing tricky tokens in strings / comments
        let payloads = [
            ("rust", "fn test() {\n    let re = regex::Regex::new(r\"^[a-zA-Z0-9_.-]+$\").unwrap();\n    let raw = r###\"raw string with \"quotes\" and # comments\"###;\n}"),
            ("python", "import re\npattern = r'^[\\w\\.-]+@[\\w\\.-]+\\.\\w+$'\nquery = f\"SELECT * FROM t WHERE c = '{pattern}' and 1 > 0\""),
            ("typescript", "const template = `Hello ${name.toUpperCase()} <span class=\"bold\">!</span>`;\nconst rx = /[a-z0-9]/gi;"),
            ("bash", "cat << 'EOF' > /tmp/output.sh\n#!/bin/bash\necho \"nested delimiter <tag>\"\nEOF"),
            ("sql", "WITH recursive_cte AS (\n  SELECT id, 1 as level FROM nodes WHERE parent_id IS NULL\n  UNION ALL\n  SELECT n.id, rc.level + 1 FROM nodes n JOIN recursive_cte rc ON n.parent_id = rc.id\n)\nSELECT * FROM recursive_cte;"),
            ("json", "{\n  \"escaped\": \"line 1\\nline 2\\t\\\"quoted\\\"\",\n  \"empty_arr\": [],\n  \"empty_obj\": {}\n}"),
            ("mermaid", "sequenceDiagram\n    autonumber\n    Alice->>Bob: Hello Bob, how are you?\n    Bob-->>Alice: I am good thanks!\n"),
            ("diff", "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -10,3 +10,4 @@\n-old line\n+new line with <tag>\n"),
        ];

        for (lang, code) in payloads {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Pango parse failed on language {} code block:\n{}\nMarkup:\n{}\nError: {:?}",
                lang,
                code,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_streaming_chunk_boundaries() {
        // Simulates incremental arrival of tokens across syntax boundaries
        let full_text = "### Real-time Streaming Response\n\nHere is some `streaming code` and a formula: $\\nabla \\cdot \\vec{B} = 0$.\n\n```python\nfor i in range(10):\n    print(f\"index: {i}\")\n```\n\n| Item | Qty |\n| --- | --- |\n| Widget | 5 |\n";

        // Break full_text into arbitrary chunk sizes (1, 3, 7, 15 chars) to test partial renders
        for chunk_size in [1, 3, 7, 15, 32] {
            let mut accumulator = String::new();
            let mut chars = full_text.chars().peekable();

            while chars.peek().is_some() {
                for _ in 0..chunk_size {
                    if let Some(c) = chars.next() {
                        accumulator.push(c);
                    }
                }

                let markup = format_markdown_to_pango(&accumulator);
                let res = pango::parse_markup(&markup, '\0');
                assert!(
                    res.is_ok(),
                    "Streaming partial render failed at chunk size {} with length {}:\nAccumulator:\n{:?}\nMarkup:\n{:?}\nError: {:?}",
                    chunk_size,
                    accumulator.len(),
                    accumulator,
                    markup,
                    res.err()
                );
            }
        }
    }

    #[test]
    fn test_fuzz_utf8_multibyte_boundary_splitting() {
        // Test chunk boundaries splitting inside complex UTF-8 multibyte characters
        // (4-byte emojis, 3-byte CJK/Vietnamese, combining diacritics)
        let sample = "🦀 Trí tuệ nhân tạo Tiếng Việt 🚀 漢字 (かんじ) \u{1F9D1}\u{200D}\u{1F4BB} (Technologist)";
        let bytes = sample.as_bytes();

        for len in 1..=bytes.len() {
            let partial_bytes = &bytes[..len];
            // Lossy decode simulates reading raw byte stream across socket boundaries
            let decoded = String::from_utf8_lossy(partial_bytes);
            let markup = format_markdown_to_pango(&decoded);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "UTF-8 multibyte boundary render failed at len {}:\nString: {:?}\nMarkup: {:?}\nError: {:?}",
                len,
                decoded,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_html_raw_tags_and_attribute_injection() {
        let tag_cases = [
            "<a href=\"https://example.com\" target=\"_blank\">Link</a>",
            "<img src=\"x\" onerror=\"alert(1)\" />",
            "<font color=\"red\" size=\"+2\">Colored font</font>",
            "<span style=\"color: red; font-size: 20px;\">Inline CSS</span>",
            "<!-- HTML comment with <embedded> tags and -- dashes -->",
            "<input type=\"text\" value=\"test\" />",
            "<iframe src=\"https://evil.com\"></iframe>",
            "<br><hr><wbr>",
            "<custom-element attribute=\"value\">Custom</custom-element>",
            "Unclosed <tag with attr=\"val\" without close",
            "Mismatched <b><i>bold and italic</b></i>",
        ];

        for c in tag_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "HTML raw tag injection failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_url_auto_linking_and_special_schemes() {
        let url_cases = [
            // Standard HTTP / HTTPS
            "Check [Rust Official](https://www.rust-lang.org) for details.",
            // FTP / SSH / Custom URI schemes
            "Connect to [FTP Server](ftp://files.example.com:21/archive.tar.gz).",
            "Clone repository: [Git SSH](git@github.com:khoipm/pi-overlay.git).",
            "Send feedback via [Email](mailto:developer@example.com?subject=Feedback).",
            // URLs with encoded spaces, accents, and unicode query parameters
            "[Search](https://www.google.com/search?q=l%E1%BA%ADp+tr%C3%ACnh+Rust&hl=vi)",
            // Markdown link with nested formatting inside link title
            "Click [**Bold Link** with `code`](https://example.com) to continue.",
            // Link without destination paren or unclosed bracket
            "Unclosed [Bracket text without url and [nested bracket]",
            // Image markdown syntax: ![alt](url)
            "![Diagram Preview](https://example.com/architecture.png)",
        ];

        for c in url_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "URL markdown case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_task_list_deep_hierarchy_and_unicode_bullets() {
        let list_cases = [
            // Deeply indented task list
            "- [x] Level 1\n  - [ ] Level 2\n    - [x] Level 3\n      - [ ] Level 4",
            // Mixed bullets (- and *)
            "* [x] Star checked\n* [ ] Star unchecked\n  - Sub item 1\n  * Sub item 2",
            // Task with rich inline styling
            "- [x] Task with **bold**, *italic*, `code()`, and $x=1$",
            // Numbered list with decimal and letters
            "1. Step one\n2. Step two\n10. Step ten\n999. Step large index",
            // Bullet with leading tabs and spaces
            "\t- Tab indented item\n\t\t- Double tab indented item",
        ];

        for c in list_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Task list case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_language_aliases_and_edge_identifiers() {
        let lang_cases = [
            ("rs", "pub fn main() {}"),
            ("rust", "fn test() -> bool { true }"),
            ("py", "def run(): pass"),
            ("python", "import sys\nprint(sys.version)"),
            ("js", "console.log('js');"),
            ("ts", "const x: number = 42;"),
            ("javascript", "let a = [1, 2, 3];"),
            ("typescript", "interface User { id: string; }"),
            ("sh", "echo 'sh'"),
            ("bash", "export PATH=$PATH:/usr/local/bin"),
            ("zsh", "autoload -Uz compinit && compinit"),
            ("shell", "pwd && whoami"),
            ("json", "{\"key\": \"val\"}"),
            ("c", "int main(void) { return 0; }"),
            ("cpp", "std::cout << \"cpp\" << std::endl;"),
            ("c++", "#include <iostream>"),
            ("c#", "Console.WriteLine(\"c#\");"),
            ("sql", "SELECT * FROM users WHERE active = 1;"),
            ("yaml", "name: pi-overlay\nversion: 1.0"),
            ("toml", "[package]\nname = \"pi-overlay\""),
            ("dockerfile", "FROM rust:1.80-alpine\nWORKDIR /app"),
            ("plaintext", "Plain text inside codeblock"),
            ("text", "Text fallback"),
            ("", "No language tag specified"),
            ("   ", "Whitespace only language tag"),
            ("UPPERCASE", "TEST UPPERCASE LANGUAGE ID"),
            ("lang-with-dashes", "Custom ID"),
            ("lang_with_underscores", "Custom ID 2"),
        ];

        for (lang, code) in lang_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Codeblock alias case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_blockquotes_nested_and_interleaved_markdown() {
        let quote_cases = [
            // Standard single quote with bold/italic
            "> This is a quote with **bold** and *italic* and `code`.",
            // 2-level nested quote
            "> Level 1\n> > Level 2\n> > > Level 3",
            // Quote interleaved with empty quote lines
            ">\n> Line after empty quote\n>",
            // GitHub alert variants
            "> [!NOTE]\n> Note content",
            "> [!TIP]\n> Tip content",
            "> [!IMPORTANT]\n> Important content",
            "> [!WARNING]\n> Warning content",
            "> [!CAUTION]\n> Caution content",
            // Alert with complex markdown following
            "> [!NOTE]\n> Alert with `command --flag` and $math$ and [Link](https://test.com)",
            // Broken quote prefix
            ">Without space after angle bracket",
            ">>Double angle without spaces",
        ];

        for c in quote_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Quote case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_header_levels_and_inline_markup_combinations() {
        let header_cases = [
            "# H1 with **bold**",
            "## H2 with *italic*",
            "### H3 with `code_fn()`",
            "# H1 with [Hyperlink](https://example.com)",
            "## H2 with <tag> & symbols and ~~strikethrough~~",
            "### H3 with $E = mc^2$",
            "#### H4 without standard markdown syntax",
            "##### H5 without standard markdown syntax",
            "###### H6 without standard markdown syntax",
            "#    Multiple leading spaces in header text",
            "#",
            "##",
            "###",
        ];

        for c in header_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Header case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_html_details_summary_variants() {
        let details_cases = [
            "<details>\n<summary>Click here to expand</summary>\nDetailed content inside\n</details>",
            "<details>\n<summary>Summary with **bold** and `code`</summary>\nContent with table:\n| A | B |\n| --- | --- |\n| 1 | 2 |\n</details>",
            // Unclosed details or summary
            "<details>\n<summary>Unclosed details block\nContent continues",
            "<summary>Dangling summary tag without details</summary>",
            "<details></details>",
            // Multiple details on same line
            "<details><summary>Inline details</summary>Body</details>",
        ];

        for c in details_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Details/summary case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_extreme_unicode_and_combining_marks() {
        let unicode_code_cases = [
            // Zalgo / Combining diacritical marks in code
            ("python", "# Zalgo test: H̶e̶l̶l̶o̶ W̶o̶r̶l̶d̶\nx = 'Z̸a̸l̸g̸o̸'"),
            // Bidirectional bidi override characters (Trojan Source attack vectors: RLO, LRO, PDF)
            ("rust", "fn check_access() {\n    // \u{202E} } if is_admin { \u{202C}\n    let access = true;\n}"),
            // Emoji identifiers & variable names
            ("javascript", "const 🚀 = 42;\nconst 🦀 = 'rust';\nconsole.log(🚀, 🦀);"),
            // Non-breaking spaces and zero-width joiners
            ("bash", "echo\u{00A0}\"NBSP argument\"\nVAR=\"ZWJ:\u{200D}\""),
            // Mathematical Alphanumeric Symbols (𝕏, ℂ, ℜ, 𝔽)
            ("python", "𝕏 = 10\nℂ = 20\nprint(𝕏 + ℂ)"),
        ];

        for (lang, code) in unicode_code_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Unicode codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_table_cells_with_multiline_and_nested_formatting() {
        let table_cases = [
            // Table with bold, italic, code, math inside cells
            "| Feature | Status | Formula |\n| :--- | :---: | ---: |\n| **Fast** | `v1.2` | $O(1)$ |\n| *Secure* | ☑ | $O(\\log n)$ |",
            // Table with empty cells and missing borders
            "| Col 1 | Col 2 | Col 3 |\n| --- | --- | --- |\n| | Middle only | |\n| Left only | | |",
            // Table with unicode characters of varying display widths
            "| Quốc Gia | Thủ Đô | Ký Hiệu |\n| :--- | :--- | :--- |\n| Việt Nam | Hà Nội | 🇻🇳 |\n| Nhật Bản (日本) | Tokyo (東京) | 🇯🇵 |\n| Hoa Kỳ | Washington D.C. | 🇺🇸 |",
            // Table with single column
            "| Single Column |\n| --- |\n| Item 1 |\n| Item 2 |",
        ];

        for c in table_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Table nested formatting case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_footnote_and_reference_styles() {
        let footnote_cases = [
            // Standard Markdown footnote citation and definition
            "Here is a simple statement[^1] with another citation[^note].\n\n[^1]: First footnote content with `code`.\n[^note]: Second footnote with **bold** text.",
            // Multiple citations in one line
            "Claim A[^1] and Claim B[^2] and Claim C[^3].",
            // Unmatched footnote syntax
            "Statement with [^unclosed bracket and ^dangling caret",
            // Reference style links: [label][ref] and [ref]: url
            "[Pi Overlay][pi-repo] is a frameless desktop overlay.\n\n[pi-repo]: https://github.com/khoipm/pi-overlay",
            // Reference link without definition
            "Reference [Unknown Reference][missing_ref] continues inline.",
        ];

        for c in footnote_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Footnote/reference case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_definition_lists_and_abbreviations() {
        let def_cases = [
            // Markdown definition list style
            "Term 1\n: Definition for term 1 with `code`\n\nTerm 2\n: Definition for term 2 with **bold**",
            // Colon without leading term
            ": Dangling colon definition line",
            // Multi-line term definition
            "Complex Algorithm\n: Step 1: Initialize values.\n: Step 2: Compute $x = \\int f(t) dt$.",
            // Abbreviation style
            "*[HTML]: Hyper Text Markup Language\n*[W3C]: World Wide Web Consortium\nThe HTML specification is maintained by the W3C.",
        ];

        for c in def_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Definition list case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_nested_backticks_and_fence_collision() {
        let code_cases = [
            // 4 backticks outer fence containing 3 backticks inner fence
            ("markdown", "````markdown\n```rust\nfn inner() {}\n```\n````"),
            // 5 backticks outer fence containing 4 backticks inner fence
            ("markdown", "`````markdown\n````rust\nfn deeper() {}\n````\n`````"),
            // Code with lines starting with backticks
            ("bash", "```bash\n`whoami` && `hostname`\n```"),
            // Inline code with triple backtick inside
            ("text", "```\nHere is a string with ``` inside it\n```"),
            // Codeblock with trailing spaces after closing fence
            ("python", "```python\nx = 1\n```   "),
            // Codeblock with language containing dot or version
            ("vue.js", "```vue.js\n<template><div>Hello</div></template>\n```"),
            ("c++20", "```c++20\nauto concept Integral = std::is_integral_v<T>;\n```"),
        ];

        for (lang, code) in code_cases {
            let markup = format_markdown_to_pango(code);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Nested backtick case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_math_block_with_complex_environments_and_align() {
        let math_env_cases = [
            // LaTeX align environment with & column separators and \\ row breaks
            "$$\n\\begin{align}\n  f(x) &= (x + 1)^2 \\\\\n       &= x^2 + 2x + 1\n\\end{align}\n$$",
            // Cases environment for piecewise functions
            "$$\nf(x) = \\begin{cases}\n  1, & \\text{if } x \\ge 0 \\\\\n  0, & \\text{otherwise}\n\\end{cases}\n$$",
            // Matrix with determinants and absolute values
            "$$\n\\det(\\mathbf{A}) = \\begin{vmatrix}\n  a & b \\\\\n  c & d\n\\end{vmatrix} = ad - bc\n$$",
            // Unclosed math environment
            "$$\n\\begin{matrix}\n  1 & 2 \\\\\n  3 & 4\n",
        ];

        for c in math_env_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Math environment case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_syntax_highlighter_token_boundaries_and_unterminated_literals() {
        // Test language syntax highlighter on edge-case unterminated strings, comments, and numbers
        let syntax_edge_cases = [
            ("rust", "let unterminated = \"string without closing quote\nlet x = 10;"),
            ("rust", "let raw_unterminated = r#\"unterminated raw string;\nlet y = 20;"),
            ("rust", "/* block comment without closing\nlet z = 30;"),
            ("python", "s = 'unterminated single quote\nx = 1"),
            ("python", "s = '''unterminated triple quote\ny = 2"),
            ("typescript", "const str = `unterminated template literal\nconst a = 1;"),
            ("json", "{\"key\": \"unterminated string value"),
            ("bash", "echo \"unterminated double quote"),
            ("bash", "echo 'unterminated single quote"),
            ("rust", "let num = 123.456.789; // malformed decimal"),
            ("rust", "let hex = 0xZZZZ; // invalid hex"),
        ];

        for (lang, code) in syntax_edge_cases {
            let highlighted = crate::render::syntax::highlight_code_to_pango(lang, code);
            let wrapped = format!("<span font_family=\"monospace\">{}</span>", highlighted);
            let res = pango::parse_markup(&wrapped, '\0');
            assert!(
                res.is_ok(),
                "Syntax highlighter failed on unterminated literal for {}:\nOutput:\n{}\nError: {:?}",
                lang,
                wrapped,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_rapid_multiturn_dialogue_state_transitions() {
        // Simulates rapid accumulation of multi-turn user/assistant conversation
        let mut session = String::new();
        for turn in 1..=50 {
            session.push_str(&format!("**Turn {} User Query:**\nHow to solve issue #{}?\n\n", turn, turn));
            session.push_str(&format!("**Turn {} Assistant Response:**\nHere is the fix:\n```rust\nfn fix_{}() -> bool {{ true }}\n```\nResult: $2^{} = {}$.\n\n", turn, turn, turn, 1u64.wrapping_shl(turn % 32)));
            session.push_str("---\n\n");
        }

        let markup = format_markdown_to_pango(&session);
        let res = pango::parse_markup(&markup, '\0');
        assert!(
            res.is_ok(),
            "Rapid multi-turn conversation render failed:\nError: {:?}",
            res.err()
        );
    }

    #[test]
    fn test_fuzz_mermaid_and_ascii_diagram_blocks() {
        let diagram_cases = [
            // Flowchart with shapes and arrows
            "```mermaid\nflowchart TD\n    A[Start] --> B{Is valid?}\n    B -- Yes --> C[Process]\n    B -- No --> D[Error]\n```",
            // Git graph diagram
            "```mermaid\ngitGraph\n    commit\n    branch hotfix\n    checkout hotfix\n    commit\n    checkout main\n    merge hotfix\n```",
            // Entity Relationship diagram
            "```mermaid\nerDiagram\n    CUSTOMER ||--o{ ORDER : places\n    ORDER ||--|{ LINE-ITEM : contains\n```",
            // ASCII diagram with unicode box-drawing characters
            "```ascii\n+-------------------+\n|  Desktop Overlay  |\n+---------+---------+\n          |\n          v\n+---------+---------+\n|   Wayland Surface |\n+-------------------+\n```",
            // Unclosed mermaid diagram
            "```mermaid\nsequenceDiagram\n    Client->>Server: SYN\n",
        ];

        for c in diagram_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Diagram case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_diff_unified_patch_syntax_highlighting() {
        let diff_cases = [
            // Standard git unified diff with hunk header
            "```diff\n@@ -1,5 +1,6 @@\n-use std::collections::HashMap;\n+use std::collections::BTreeMap;\n+// Added new dependency\n```",
            // Diff containing HTML tags and markdown symbols
            "```diff\n- <div class=\"old-badge\">Deprecated</div>\n+ <div class=\"new-badge\">**Active**</div>\n```",
            // Diff with leading/trailing blank lines
            "```diff\n\n-line a\n+line b\n\n```",
            // Diff with only additions or only deletions
            "```diff\n+added 1\n+added 2\n```",
            "```diff\n-deleted 1\n-deleted 2\n```",
        ];

        for c in diff_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Diff case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_tensor_calculus_and_polyglot_code() {
        let cases = [
            // Tensor calculus and Dirac bra-ket quantum notation
            "$$\n|\\psi\\rangle = \\sum_{i} c_i |i\\rangle \\quad \\text{and} \\quad \\langle\\phi|\\psi\\rangle = \\int \\phi^*(x) \\psi(x) dx\n$$",
            // Einstein summation convention and covariant derivatives
            "$$\n\\nabla_\\mu V^\\nu = \\partial_\\mu V^\\nu + \\Gamma^\\nu_{\\mu\\lambda} V^\\lambda\n$$",
            // Deeply nested triple formatting inside strike
            "~~*this is **bold italic strike** and `code` inside*~~",
            // Broken XML/HTML entities
            "Entity test: &incomplete &amp; &#1234; &#xABCD; &lt;valid&gt;",
            // 10-column table
            "| c1 | c2 | c3 | c4 | c5 | c6 | c7 | c8 | c9 | c10 |\n|---|---|---|---|---|---|---|---|---|---|\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |",
            // Mixed languages polyglot snippet
            "```polyglot\n# Python\ndef f(): pass\n// Rust\nfn g() {}\n/* C */\nint h(void);\n```",
        ];

        for c in cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Tensor/polyglot case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_repeated_delimiter_pathologies() {
        // Pathological runs of delimiters that stress recursion / regex lookaheads
        let pathologies = [
            "__underline__ and ___underline-italic___",
            "****************************************************************",
            "================================================================",
            "----------------------------------------------------------------",
            "~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~",
            "````````````````````````````````````````````````````````````````",
            "$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$$",
            "> > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > > Level 33 quote",
            "- - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - Dashed list",
        ];

        for p in pathologies {
            let markup = format_markdown_to_pango(p);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Pathological delimiter case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                p,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_mixed_inline_delimiters_without_whitespace() {
        // Delimiters directly touching words or each other without spaces
        let compact_cases = [
            "**bold***italic*~~strike~~`code`$math$",
            "a**b**c*d*e~~f~~g`h`i$j$k",
            "***bolditalic***and***more***",
            "~~**boldstrike**~~and**~~strikebold~~**",
            "`code1``code2``code3`",
            "$x_1$$x_2$$x_3$",
            "H~2~O+CO~2~->H~2~CO~3~",
            "10^2^+10^3^=1100",
            "[L1](u1)[L2](u2)[L3](u3)",
        ];

        for c in compact_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Compact delimiter case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_multilingual_asian_and_african_scripts() {
        let script_cases = [
            // Devanagari (Hindi, Sanskrit)
            "# नमस्ते दुनिया\nयह **बोल्ड** और *इटैलिक* टेक्स्ट है।\n```python\nprint('नमस्ते')\n```",
            // Thai (no word boundaries)
            "## สวัสดีชาวโลก\nข้อความที่มี `โค้ด` และ **ตัวหนา**",
            // Arabic and Hebrew RTL in table
            "| English | Arabic | Hebrew |\n| :--- | :--- | :--- |\n| Hello | مرحبا | שלום |\n| World | العالم | עולם |",
            // Korean Hangul
            "### 안녕하세요 세계\n- [x] 완료된 작업: `cargo test`\n- [ ] 대기 중인 작업: **Pango 렌더링**",
            // Amharic (Ethiopic script)
            "> ሰላም ልዑል\n> ይህ **የተመረጠ** ጽሑፍ ነው።",
        ];

        for c in script_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Multilingual script case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_ansi_color_escapes_and_terminal_output() {
        let terminal_cases = [
            // ANSI escape sequences inside terminal code block (e.g. bash test results)
            "```bash\n\x1B[32mPASS\x1B[0m src/test.rs\n\x1B[31mFAIL\x1B[0m src/backend.rs\n\x1B[33mWARN\x1B[0m clippy suggestion\n```",
            // SGR color codes and cursor movement
            "```output\n\x1B[1;34mINFO\x1B[0m: [2026-10-24 14:00] Server listening on :8080\n\x1B[2K\rLoading... 100%\n```",
            // Incomplete or fragmented ANSI escape sequence
            "```text\nRaw escape prefix \x1B[ and dangling \x1B without brackets\n```",
            // Control characters (CR, LF, TAB, VT, FF) inside code
            "```text\r\nLine with CRLF\r\nLine with VT \x0B and FF \x0C\r\n```",
        ];

        for c in terminal_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "ANSI terminal code case failed on:\n{:?}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_table_delimiter_alignment_and_colons() {
        let align_cases = [
            // Center, Left, Right alignments with arbitrary number of hyphens
            "| L | C | R |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |",
            // Short 1-hyphen separators
            "| A | B |\n| :- | -: |\n| x | y |",
            // Only colons or extra wide separators
            "| Left | Right |\n| :----------------- | -----------------: |\n| Hello | World |",
            // Malformed separator row (treated as normal row without crashing)
            "| Col 1 | Col 2 |\n| not a separator | still not |\n| a | b |",
        ];

        for c in align_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Table alignment case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_calculus_summations_and_integrals() {
        let calculus_cases = [
            // Surface and volume integrals with differentials
            "$$\n\\iint_{S} (\\nabla \\times \\mathbf{F}) \\cdot d\\mathbf{S} = \\oint_{C} \\mathbf{F} \\cdot d\\mathbf{r}\n$$",
            // Divergence theorem
            "$$\n\\iiint_{V} (\\nabla \\cdot \\mathbf{F}) \\, dV = \\oiint_{S} (\\mathbf{F} \\cdot \\mathbf{n}) \\, dS\n$$",
            // Partial derivatives and wave equation
            "$$\n\\frac{\\partial^2 u}{\\partial t^2} = c^2 \\nabla^2 u = c^2 \\left( \\frac{\\partial^2 u}{\\partial x^2} + \\frac{\\partial^2 u}{\\partial y^2} + \\frac{\\partial^2 u}{\\partial z^2} \\right)\n$$",
            // Fourier transform and inverse Fourier transform
            "$$\n\\hat{f}(\\xi) = \\int_{-\\infty}^{\\infty} f(x) e^{-2\\pi i x \\xi} dx, \\quad f(x) = \\int_{-\\infty}^{\\infty} \\hat{f}(\\xi) e^{2\\pi i x \\xi} d\\xi\n$$",
            // Continued fraction
            "$$\nx = a_0 + \\frac{1}{a_1 + \\frac{1}{a_2 + \\frac{1}{a_3 + \\dots}}}\n$$",
        ];

        for c in calculus_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Calculus case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_multiline_mixed_prose_and_code_blocks() {
        let mixed_cases = [
            // Prose with multiple backticks followed by code block
            "Check `config.json` and `settings.toml` before running:\n```bash\ncargo run --release\n```\nDone.",
            // Multiple code blocks separated by single line of prose
            "```rust\nfn a() {}\n```\nIntermediate text with **bold**.\n```python\ndef b(): pass\n```",
            // Code block followed immediately by a table without blank line
            "```rust\nlet x = 1;\n```\n| K | V |\n|---|---|\n| 1 | 2 |",
            // Table followed immediately by code block without blank line
            "| A | B |\n|---|---|\n| a | b |\n```bash\necho ok\n```",
        ];

        for c in mixed_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Mixed prose/code case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_markdown_table_escaping_and_pipe_in_code_cells() {
        let table_pipe_cases = [
            // Code with pipe inside table cells
            "| Command | Description |\n| :--- | :--- |\n| `cat file \\| grep ok` | Filter output with pipe |\n| `a && b \\|\\| c` | Boolean chaining |",
            // Empty table cells with whitespace
            "| Col A | Col B |\n| --- | --- |\n|    |    |\n|   val   |   |",
            // Table with multiple consecutive pipes
            "| A || B |\n| --- || --- |\n| 1 || 2 |",
            // Table with math formulas containing pipes (absolute values / norms)
            "| Metric | Formula |\n| :--- | :--- |\n| Absolute | $|x - y| < \\epsilon$ |\n| Norm | $\\|\\mathbf{v}\\|_2 = 1$ |",
        ];

        for c in table_pipe_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Table with pipes case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_deep_indentation_tabs_and_spaces_mix() {
        let indent_cases = [
            // Deeply indented code block (4 spaces vs tabs)
            "    let x = 1;\n    let y = 2;\n    x + y",
            "\t\tlet indented_with_tabs = true;",
            " \t \t Mixed space and tab indentation",
            // Deeply indented list items with mixed whitespace
            "  - Space 2\n    - Space 4\n      - Space 6\n        - Space 8",
            "\t- Tab 1\n\t\t- Tab 2\n\t\t\t- Tab 3",
            // Trailing whitespace on empty lines
            "Paragraph\n   \n   \nNext paragraph",
        ];

        for c in indent_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Indentation case failed on:\n{:?}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_superscript_subscript_chemical_and_algebraic() {
        let sub_sup_cases = [
            // Chemical formulas with multiple subscripts
            "Glucose: C~6~H~12~O~6~ and Sulfuric acid: H~2~SO~4~",
            // Ions and charges (subscript and superscript adjacent)
            "Phosphate ion: PO~4~^3-^ and Calcium ion: Ca^2+^",
            // Algebraic polynomials
            "P(x) = a~n~x^n^ + a~n-1~x^n-1^ + ... + a~1~x + a~0~",
            // Einstein mass-energy and power towers
            "E = mc^2^ and 2^2^ = 4",
            // Dangling or invalid tildes/carets with spaces
            "Price approx ~ 100 USD and not~subscript~",
            "Power ^ 2 and not^superscript^",
            // Empty tildes/carets
            "Empty ~~ and ^^ inside text",
        ];

        for c in sub_sup_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Sub/Sup case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_syntax_color_palette_contrast_and_accessibility() {
        // Test that highlighted code uses valid Pango foreground hex codes matching Antigravity palette
        let code_sample = "fn calculate(input: &str) -> Option<u64> {\n    let val: u64 = 42;\n    if true {\n        return Some(val * 2);\n    }\n    None\n}";
        let highlighted = crate::render::syntax::highlight_code_to_pango("rust", code_sample);

        // Ensure palette hex values are valid 7-character `#RRGGBB` strings
        for line in highlighted.lines() {
            let mut rest = line;
            while let Some(pos) = rest.find("foreground=\"") {
                let color_str = &rest[pos + 12..];
                if let Some(end_quote) = color_str.find('"') {
                    let hex = &color_str[..end_quote];
                    assert!(hex.starts_with('#'), "Color code must start with #: {}", hex);
                    assert!(hex.len() == 7, "Hex color code must be 7 chars (#RRGGBB): {}", hex);
                    assert!(hex[1..].chars().all(|c| c.is_ascii_hexdigit()), "Invalid hex digits: {}", hex);
                    rest = &color_str[end_quote + 1..];
                } else {
                    break;
                }
            }
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_crlf_and_mixed_newlines() {
        let newline_cases = [
            // CRLF Windows style line endings in code
            "```rust\r\nfn hello() {\r\n    println!(\"Hello CRLF\");\r\n}\r\n```",
            // CR only old Mac style line endings
            "```python\rdef run():\r    pass\r```",
            // Mixed LF and CRLF in same code block
            "```bash\necho \"LF line\"\r\necho \"CRLF line\"\necho \"another LF\"\n```",
            // Code block with multiple trailing blank lines
            "```json\n{\n  \"ok\": true\n}\n\n\n\n```",
        ];

        for c in newline_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "CRLF codeblock case failed on:\n{:?}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_nested_callout_alerts_with_code_and_lists() {
        let nested_callouts = [
            // Callout note with internal list and code snippet
            "> [!NOTE]\n> Notice summary:\n> - Step 1: `cargo build`\n> - Step 2: `cargo test`\n> All tests must pass.",
            // Warning with bold and math equation
            "> [!WARNING]\n> High latency detected: $\\Delta t > 500\\text{ms}$.\n> Take action immediately.",
            // Important with link and checklist
            "> [!IMPORTANT]\n> Required reading: [Spec](https://specs.example.com)\n> - [x] Approved\n> - [ ] Pending signature",
            // Caution with multiple consecutive quote lines
            "> [!CAUTION]\n> Danger zone ahead.\n>\n> Do not edit generated files manually.",
        ];

        for c in nested_callouts {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Nested callout case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_mathematical_logic_quantifiers_and_symbols() {
        let logic_cases = [
            // Predicate logic formulas with quantifiers
            "$$\n\\forall x \\in \\mathbb{R}, \\; \\exists y \\in \\mathbb{R} : x + y = 0\n$$",
            // De Morgan's laws and set operations
            "$$\n\\neg (P \\land Q) \\iff (\\neg P \\lor \\neg Q) \\quad \\text{and} \\quad A \\cap (B \\cup C) = (A \\cap B) \\cup (A \\cap C)\n$$",
            // Subset, superset, empty set, element of
            "Inline logic: $x \\in A \\subseteq B$, $A \\setminus B \\neq \\emptyset$, and $f : X \\to Y$.",
            // Turnstile, consequence, equivalence
            "$$\n\\Gamma \\vdash \\phi \\implies \\Gamma \\models \\phi\n$$",
            // Modus ponens and inference rules
            "$$\n\\frac{P \\quad P \\to Q}{Q}\n$$",
        ];

        for c in logic_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Logic quantifier case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_nested_brackets_and_link_parsing_robustness() {
        let bracket_cases = [
            // Link with multiple bracket pairs in label
            "[Text [with] nested [brackets] inside](https://example.com)",
            // Empty link label or empty destination
            "[](https://example.com) and [Label]() and []()",
            // Links separated only by commas or slashes
            "[Link 1](https://a.com), [Link 2](https://b.com)/[Link 3](https://c.com)",
            // Consecutive opening brackets
            "[[[[Multiple open brackets without close",
            // Consecutive closing brackets
            "Multiple close brackets]]]] without open",
            // Markdown link with spaces in URL
            "[Space Link](https://example.com/path with spaces)",
        ];

        for c in bracket_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Bracket link case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_c_cpp_macro_preprocessors_and_includes() {
        let cpp_cases = [
            ("c", "#include <stdio.h>\n#include <stdlib.h>\n#define MAX(a, b) ((a) > (b) ? (a) : (b))\n\nint main(int argc, char **argv) {\n    printf(\"Hello %s\\n\", \"World\");\n    return 0;\n}"),
            ("cpp", "#include <iostream>\n#include <vector>\n#include <memory>\n\n#ifdef DEBUG\n#define LOG(x) std::cout << (x) << std::endl\n#else\n#define LOG(x)\n#endif\n\ntemplate<typename T>\nconcept Numeric = std::is_arithmetic_v<T>;\n\nint main() {\n    std::vector<int> v = {1, 2, 3};\n    return 0;\n}"),
            ("c++", "#pragma once\n#ifndef HEADER_H\n#define HEADER_H\nstruct Node {\n    int val;\n    Node *next;\n};\n#endif"),
            ("c", "#define STR(x) #x\n#define CONCAT(a, b) a##b\nint CONCAT(var, 1) = 42;"),
        ];

        for (lang, code) in cpp_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "C/C++ preprocessor case failed for {}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_streaming_single_character_expansion_stress() {
        // Feed text 1 character at a time across a challenging mixed markdown document
        let stream_doc = "## Live Header\n> [!TIP]\n> Pro tip with `code`.\n$$\\sum_{i=1}^n i = \\frac{n(n+1)}{2}$$\n- [x] Item 1\n| A | B |\n|---|---|\n| 1 | 2 |\n```rust\nfn main() {}\n```\n";
        let mut buffer = String::new();

        for ch in stream_doc.chars() {
            buffer.push(ch);
            let markup = format_markdown_to_pango(&buffer);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Single-char expansion failed at char {:?} (len {}):\nBuffer:\n{:?}\nMarkup:\n{:?}\nError: {:?}",
                ch,
                buffer.len(),
                buffer,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_json_syntax_highlighter_deep_structures_and_escapes() {
        let json_cases = [
            // Deeply nested JSON objects and arrays
            "```json\n{\n  \"a\": {\n    \"b\": {\n      \"c\": {\n        \"d\": [1, 2, 3, true, false, null]\n      }\n    }\n  }\n}\n```",
            // JSON with escaped quotes, backslashes, and unicode escapes
            "```json\n{\n  \"key\\\"with\\\"quotes\": \"value\\\\with\\\\backslashes\",\n  \"unicode\": \"\\u0041\\u0042\\u0043 (ABC)\",\n  \"html_like\": \"<div id=\\\"main\\\">Hello & Goodbye</div>\"\n}\n```",
            // Empty and scalar JSON structures
            "```json\n{}\n```",
            "```json\n[]\n```",
            "```json\n[true, false, null, 0, -1, 3.14159, 1e-10]\n```",
            // JSON key without space after colon or compact minified line
            "```json\n{\"id\":1,\"active\":true,\"name\":null,\"tags\":[\"rust\",\"gtk4\"]}\n```",
        ];

        for c in json_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "JSON highlighter case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_unicode_mathematical_operators_and_fractions() {
        let op_cases = [
            // Unicode math operators outside LaTeX
            "Arithmetic: 5 × 4 ÷ 2 = 10, and 10 ± 2 ≠ 7.",
            // Vulgar fractions (½, ⅓, ¼, ¾, ⅛, ⅜, ⅝, ⅞)
            "Measurements: ½ cup flour, 1¾ teaspoons salt, ⅜ ounce sugar.",
            // Comparison symbols (≤, ≥, ≪, ≫, ≈, ≡, ≅)
            "Approximations: π ≈ 3.14159, and x ≪ y implies x / y ≈ 0.",
            // Calculus symbols outside math mode (∂, ∇, ∫, ∬, ∭, ∮)
            "Operators: ∇f is gradient, ∂f/∂x is partial, and ∮ E·dl = -dΦ/dt.",
            // Set symbols outside math mode (∈, ∉, ∋, ∌, ⊂, ⊃, ⊆, ⊇)
            "Set logic: x ∈ S and S ⊆ U, where S ∩ T = ∅.",
        ];

        for c in op_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Unicode math operator case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_mermaid_gantt_and_pie_chart_diagrams() {
        let chart_cases = [
            // Gantt chart syntax
            "```mermaid\ngantt\n    title A Gantt Diagram\n    dateFormat YYYY-MM-DD\n    section Section\n    A task :a1, 2026-01-01, 30d\n    Another task :after a1, 20d\n```",
            // Pie chart syntax
            "```mermaid\npie title Key Performance Metrics\n    \"Rust Backend\" : 75\n    \"GTK4 Shell\" : 20\n    \"CSS Custom\" : 5\n```",
            // State diagram
            "```mermaid\nstateDiagram-v2\n    [*] --> Ready\n    Ready --> Working : Submit\n    Working --> Ready : Done\n    Working --> [*] : Abort\n```",
            // Mindmap syntax
            "```mermaid\nmindmap\n  root((Pi Overlay))\n    Universal Linux Desktop\n      Layer-Shell\n      X11 Fallback\n    High Performance\n      Zero Zombie\n      Safe Markdown\n```",
        ];

        for c in chart_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Mermaid chart case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_sql_complex_syntax_joins_and_subqueries() {
        let sql_cases = [
            "```sql\nSELECT u.id, u.username, COUNT(p.id) AS post_count\nFROM users u\nLEFT JOIN posts p ON u.id = p.user_id AND p.status = 'published'\nWHERE u.created_at >= '2026-01-01'\nGROUP BY u.id, u.username\nHAVING COUNT(p.id) > 10\nORDER BY post_count DESC\nLIMIT 50;\n```",
            "```sql\nINSERT INTO sessions (id, token, user_id, expires_at)\nVALUES ('sess_123', 'tok_abc', 42, NOW() + INTERVAL '7 days')\nON CONFLICT (id) DO UPDATE SET expires_at = EXCLUDED.expires_at;\n```",
            "```sql\n-- Create partitioned table\nCREATE TABLE logs (\n    log_id BIGSERIAL,\n    created_at TIMESTAMPTZ NOT NULL,\n    level VARCHAR(10),\n    message TEXT\n) PARTITION BY RANGE (created_at);\n```",
        ];

        for c in sql_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "SQL complex syntax case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_statistical_and_probability_distributions() {
        let prob_cases = [
            // Gaussian / Normal distribution PDF
            "$$\nf(x | \\mu, \\sigma^2) = \\frac{1}{\\sqrt{2\\pi\\sigma^2}} \\exp\\left( -\\frac{(x - \\mu)^2}{2\\sigma^2} \\right)\n$$",
            // Bayes theorem and conditional expectation
            "$$\nP(A | B) = \\frac{P(B | A) P(A)}{P(B)} = \\frac{P(B | A) P(A)}{\\sum_i P(B | A_i) P(A_i)}\n$$",
            "$$\n\\mathbb{E}[X | Y] = \\int_{-\\infty}^{\\infty} x f_{X|Y}(x | y) dx\n$$",
            // Poisson and Binomial distributions
            "$$\nP(X = k) = \\frac{\\lambda^k e^{-\\lambda}}{k!}, \\quad \\binom{n}{k} p^k (1-p)^{n-k}\n$$",
            // Covariance matrix and multivariate normal
            "$$\n\\mathbf{\\Sigma} = \\mathbb{E}[(\\mathbf{X} - \\mathbf{\\mu})(\\mathbf{X} - \\mathbf{\\mu})^T]\n$$",
        ];

        for c in prob_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Probability LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_indented_nested_codeblock_and_list_interleaving() {
        let nested_list_code = [
            // List item containing code block
            "- Step 1: Install tools\n\n  ```bash\n  curl -fsSL https://get.pi.dev | sh\n  ```\n\n- Step 2: Run overlay",
            // Numbered list item with indented code block
            "1. First step\n   ```rust\n   let ok = true;\n   ```\n2. Second step",
            // Task list with nested code block
            "- [x] Configure environment\n\n  ```json\n  {\"port\": 8080}\n  ```\n\n- [ ] Launch daemon",
        ];

        for c in nested_list_code {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Nested list code case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_physics_lagrangian_hamiltonian_and_quantum_field() {
        let physics_cases = [
            // Euler-Lagrange equations of motion
            "$$\n\\frac{d}{dt} \\left( \\frac{\\partial L}{\\partial \\dot{q}_i} \\right) - \\frac{\\partial L}{\\partial q_i} = 0\n$$",
            // Hamiltonian mechanics and Poisson brackets
            "$$\n\\dot{q}_i = \\frac{\\partial H}{\\partial p_i}, \\quad \\dot{p}_i = -\\frac{\\partial H}{\\partial q_i}, \\quad \\{f, g\\} = \\sum_i \\left( \\frac{\\partial f}{\\partial q_i} \\frac{\\partial g}{\\partial p_i} - \\frac{\\partial f}{\\partial p_i} \\frac{\\partial g}{\\partial q_i} \\right)\n$$",
            // Dirac equation in covariant form with gamma matrices
            "$$\n(i \\gamma^\\mu \\partial_\\mu - m) \\psi = 0, \\quad \\{\\gamma^\\mu, \\gamma^\\nu\\} = 2 g^{\\mu\\nu} \\mathbf{I}\n$$",
            // Quantum field theory path integral
            "$$\n\\mathcal{Z} = \\int \\mathcal{D}[\\phi] \\exp\\left( \\frac{i}{\\hbar} \\int d^4x \\, \\mathcal{L}(\\phi, \\partial_\\mu \\phi) \\right)\n$$",
            // Maxwell's equations in differential forms
            "$$\ndF = 0, \\quad d{*F} = {*J}\n$$",
        ];

        for c in physics_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Physics LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_dockerfile_and_yaml_configurations() {
        let config_cases = [
            // Multi-stage Dockerfile
            ("dockerfile", "# Build stage\nFROM rust:1.80-alpine AS builder\nWORKDIR /build\nCOPY . .\nRUN cargo build --release\n\n# Runtime stage\nFROM alpine:3.20\nRUN apk add --no-cache gtk4-layer-shell\nCOPY --from=builder /build/target/release/pi-overlay /usr/local/bin/\nENTRYPOINT [\"pi-overlay\"]"),
            // Complex Kubernetes YAML manifest
            ("yaml", "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: pi-overlay-worker\n  labels:\n    app: pi-overlay\nspec:\n  replicas: 3\n  selector:\n    matchLabels:\n      app: pi-overlay\n  template:\n    metadata:\n      labels:\n        app: pi-overlay\n    spec:\n      containers:\n      - name: worker\n        image: pi-overlay:latest\n        resources:\n          limits:\n            memory: \"512Mi\"\n            cpu: \"500m\""),
            // GitHub Actions Workflow YAML
            ("yaml", "name: CI Test\non: [push, pull_request]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n    - uses: actions/checkout@v4\n    - name: Run tests\n      run: cargo test --all-targets"),
        ];

        for (lang, code) in config_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Config codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_number_theory_modular_arithmetic_and_cryptography() {
        let crypto_cases = [
            // RSA key generation and Euler's totient function
            "$$\n\\phi(n) = (p - 1)(q - 1), \\quad e \\cdot d \\equiv 1 \\pmod{\\phi(n)}\n$$",
            // Elliptic curve equation and group law
            "$$\ny^2 \\equiv x^3 + ax + b \\pmod{p}, \\quad 4a^3 + 27b^2 \\not\\equiv 0 \\pmod{p}\n$$",
            // Diffie-Hellman key exchange
            "$$\ng^{ab} \\equiv (g^a)^b \\equiv (g^b)^a \\pmod{p}\n$$",
            // Chinese Remainder Theorem
            "$$\nx \\equiv a_i \\pmod{m_i}, \\quad \\gcd(m_i, m_j) = 1 \\quad \\forall i \\neq j\n$$",
            // Quadratic residues and Legendre / Jacobi symbol
            "$$\n\\left( \\frac{a}{p} \\right) \\equiv a^{\\frac{p-1}{2}} \\pmod{p}\n$$",
        ];

        for c in crypto_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Number theory LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_assembly_and_bytecode() {
        let asm_cases = [
            // x86_64 Assembly AT&T and Intel syntax
            ("asm", "; x86_64 Intel syntax\nglobal _start\nsection .text\n_start:\n    mov rax, 1          ; sys_write\n    mov rdi, 1          ; stdout\n    mov rsi, msg\n    mov rdx, 13\n    syscall\n    mov rax, 60         ; sys_exit\n    xor rdi, rdi\n    syscall"),
            // ARM64 / AArch64 Assembly
            ("assembly", "// AArch64 assembly\n.global _start\n_start:\n    mov x0, #1\n    ldr x1, =msg\n    mov x2, #13\n    mov x8, #64\n    svc #0"),
            // WebAssembly WAT format
            ("wat", "(module\n  (func $add (param $lhs i32) (param $rhs i32) (result i32)\n    local.get $lhs\n    local.get $rhs\n    i32.add)\n  (export \"add\" (func $add)))"),
        ];

        for (lang, code) in asm_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Assembly codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_differential_geometry_and_topology() {
        let geom_cases = [
            // Metric tensor and Riemannian curvature tensor
            "$$\nR^\\rho{}_{\\sigma\\mu\\nu} = \\partial_\\mu \\Gamma^\\rho_{\\nu\\sigma} - \\partial_\\nu \\Gamma^\\rho_{\\mu\\sigma} + \\Gamma^\\rho_{\\mu\\lambda} \\Gamma^\\lambda_{\\nu\\sigma} - \\Gamma^\\rho_{\\nu\\lambda} \\Gamma^\\lambda_{\\mu\\sigma}\n$$",
            // Ricci scalar and Einstein field equations with cosmological constant
            "$$\nG_{\\mu\\nu} + \\Lambda g_{\\mu\\nu} = \\frac{8\\pi G}{c^4} T_{\\mu\\nu}, \\quad R = g^{\\mu\\nu} R_{\\mu\\nu}\n$$",
            // Exterior derivative, Hodge star operator, and differential forms
            "$$\nd(\\alpha \\wedge \\beta) = d\\alpha \\wedge \\beta + (-1)^p \\alpha \\wedge d\\beta, \\quad {*}({*\\alpha}) = (-1)^{k(n-k)} s \\alpha\n$$",
            // Euler characteristic and Gauss-Bonnet theorem
            "$$\n\\chi(M) = 2 - 2g = \\frac{1}{2\\pi} \\int_M K \\, dA\n$$",
            // Homotopy groups and fundamental group
            "$$\n\\pi_1(S^1) \\cong \\mathbb{Z}, \\quad \\pi_n(S^n) \\cong \\mathbb{Z} \\quad (n \\ge 1)\n$$",
        ];

        for c in geom_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Differential geometry LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_functional_languages_and_type_signatures() {
        let fp_cases = [
            // Haskell currying, monads and typeclasses
            ("haskell", "-- Haskell Monad and Functor\nclass Functor f => Monad f where\n    (>>=)  :: f a -> (a -> f b) -> f b\n    return :: a -> f a\n\nquicksort :: Ord a => [a] -> [a]\nquicksort []     = []\nquicksort (x:xs) = quicksort [y | y <- xs, y <= x] ++ [x] ++ quicksort [y | y <- xs, y > x]"),
            // OCaml pattern matching and modules
            ("ocaml", "(* OCaml pattern matching *)\ntype 'a tree = Leaf | Node of 'a * 'a tree * 'a tree\n\nlet rec depth = function\n  | Leaf -> 0\n  | Node (_, l, r) -> 1 + max (depth l) (depth r)"),
            // Elixir pattern matching and pipes
            ("elixir", "# Elixir pipeline\ndef process_data(input) do\n  input\n  |> String.trim()\n  |> String.split(\",\")\n  |> Enum.map(&String.to_integer/1)\n  |> Enum.sum()\nend"),
            // Lisp / Clojure macros and S-expressions
            ("clojure", ";; Clojure macro\n(defmacro unless [pred a b]\n  `(if (not ~pred) ~a ~b))\n\n(defn factorial [n]\n  (reduce *' (range 1 (inc n))))"),
        ];

        for (lang, code) in fp_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "FP codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_abstract_algebra_and_category_theory() {
        let cat_cases = [
            // Commutative diagram and functoriality
            "$$\nF(f \\circ g) = F(f) \\circ F(g), \\quad F(\\mathrm{id}_A) = \\mathrm{id}_{F(A)}\n$$",
            // Natural transformation square
            "$$\n\\eta_B \\circ F(f) = G(f) \\circ \\eta_A, \\quad \\forall f : A \\to B\n$$",
            // Adjunction (left adjoint -| right adjoint)
            "$$\n\\mathrm{Hom}_{\\mathcal{D}}(F(C), D) \\cong \\mathrm{Hom}_{\\mathcal{C}}(C, G(D))\n$$",
            // Yoneda lemma
            "$$\n\\mathrm{Nat}(\\mathrm{Hom}(A, -), F) \\cong F(A)\n$$",
            // Exact sequence in homological algebra
            "$$\n0 \\longrightarrow A \\xrightarrow{\\;f\\;} B \\xrightarrow{\\;g\\;} C \\longrightarrow 0\n$$",
        ];

        for c in cat_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Category theory LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_scientific_and_numerical_computing() {
        let sci_cases = [
            // Python NumPy / SciPy matrix vectorization
            ("python", "import numpy as np\nfrom scipy.linalg import eig, svd\n\nA = np.random.randn(100, 100)\nU, s, Vt = svd(A)\nvals, vecs = eig(A)\nprint(f'Condition number: {s[0] / s[-1]:.2e}')"),
            // Julia broadcasting, multiple dispatch, and unicode operators
            ("julia", "# Julia numerical computing\nfunction fast_fourier(x::Vector{ComplexF64})::Vector{ComplexF64}\n    N = length(x)\n    N <= 1 && return x\n    even = fast_fourier(x[1:2:end])\n    odd  = fast_fourier(x[2:2:end])\n    T = [exp(-2π * im * (k - 1) / N) * odd[k] for k in 1:div(N, 2)]\n    return vcat(even .+ T, even .- T)\nend"),
            // R statistical data frame manipulation
            ("r", "# R tidyverse analysis\nlibrary(dplyr)\nlibrary(ggplot2)\n\ndf %>% filter(status == 'active' & score > 90) %>%\n  group_by(category) %>%\n  summarise(mean_val = mean(value, na.rm = TRUE))"),
            // MATLAB / Octave matrix operations
            ("matlab", "% SVD and QR decomposition\nA = [1, 2, 3; 4, 5, 6; 7, 8, 9];\n[Q, R] = qr(A);\n[U, S, V] = svd(A);\neigs = eig(A);"),
        ];

        for (lang, code) in sci_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Scientific computing codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_information_theory_entropy_and_channel_capacity() {
        let info_cases = [
            // Shannon entropy formula
            "$$\nH(X) = -\\sum_{x \\in \\mathcal{X}} P(x) \\log_2 P(x)\n$$",
            // Kullback-Leibler divergence (relative entropy)
            "$$\nD_{\\mathrm{KL}}(P \\parallel Q) = \\sum_{x \\in \\mathcal{X}} P(x) \\log \\left( \\frac{P(x)}{Q(x)} \\right)\n$$",
            // Mutual information and Venn relation
            "$$\nI(X; Y) = H(X) - H(X | Y) = H(Y) - H(Y | X) = H(X) + H(Y) - H(X, Y)\n$$",
            // Shannon-Hartley channel capacity theorem
            "$$\nC = B \\log_2 \\left( 1 + \\frac{S}{N} \\right) = B \\log_2 (1 + \\mathrm{SNR})\n$$",
            // Cross-entropy loss in machine learning
            "$$\n\\mathcal{L}_{\\mathrm{CE}} = -\\frac{1}{N} \\sum_{i=1}^N \\sum_{c=1}^C y_{i,c} \\log(\\hat{y}_{i,c})\n$$",
        ];

        for c in info_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Information theory LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_solidity_and_smart_contracts() {
        let web3_cases = [
            ("solidity", "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.20;\n\nimport \"@openzeppelin/contracts/token/ERC20/ERC20.sol\";\n\ncontract PiToken is ERC20 {\n    address public immutable owner;\n\n    event TokensMinted(address indexed to, uint256 amount);\n\n    constructor() ERC20(\"Pi Overlay\", \"PIO\") {\n        owner = msg.sender;\n        _mint(msg.sender, 1_000_000 * 10 ** decimals());\n    }\n}"),
            ("vyper", "# @version ^0.3.7\nfrom vyper.interfaces import ERC20\n\nimplements: ERC20\n\nbalanceOf: public(HashMap[address, uint256])\ntotalSupply: public(uint256)\n\n@external\ndef transfer(_to: address, _val: uint256) -> bool:\n    self.balanceOf[msg.sender] -= _val\n    self.balanceOf[_to] += _val\n    return True"),
        ];

        for (lang, code) in web3_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Web3 codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_graph_theory_and_network_flows() {
        let graph_cases = [
            // Max-flow min-cut theorem
            "$$\n\\max_{f} \\, |f| = \\min_{S, T} \\, c(S, T) = \\sum_{u \\in S, v \\in T} c(u, v)\n$$",
            // Graph Laplacian matrix L = D - A
            "$$\n\\mathbf{L}_{i,j} = \\begin{cases}\n  \\deg(v_i) & \\text{if } i = j \\\\\n  -1 & \\text{if } i \\neq j \\text{ and } (v_i, v_j) \\in E \\\\\n  0 & \\text{otherwise}\n\\end{cases}\n$$",
            // PageRank random walk equation
            "$$\n\\mathbf{r} = d \\mathbf{M} \\mathbf{r} + \\frac{1 - d}{N} \\mathbf{1}\n$$",
            // Adjacency matrix eigenvalue spectrum
            "$$\n\\operatorname{spec}(G) = \\{ \\lambda_1^{m_1}, \\lambda_2^{m_2}, \\dots, \\lambda_k^{m_k} \\}\n$$",
            // Chromatic polynomial and Euler formula for planar graphs
            "$$\nV - E + F = 2, \\quad P(G, k) = k(k-1)^{n-1}\n$$",
        ];

        for c in graph_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Graph theory LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_graphql_and_protobuf_schemas() {
        let schema_cases = [
            // GraphQL schema definition
            ("graphql", "# GraphQL Schema\ntype Query {\n  user(id: ID!): User\n  activeSessions(limit: Int = 10): [Session!]!\n}\n\ntype User {\n  id: ID!\n  username: String!\n  email: String\n  role: Role!\n}\n\nenum Role {\n  ADMIN\n  DEVELOPER\n  GUEST\n}"),
            // Protocol Buffers proto3 definition
            ("protobuf", "syntax = \"proto3\";\npackage pi.overlay.v1;\n\nmessage StreamEvent {\n  uint64 generation = 1;\n  oneof event {\n    string text_delta = 2;\n    string thinking_delta = 3;\n    ToolCall tool_call = 4;\n  }\n}\n\nmessage ToolCall {\n  string name = 1;\n  string args = 2;\n}"),
        ];

        for (lang, code) in schema_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Schema codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_combinatorics_and_generating_functions() {
        let comb_cases = [
            // Ordinary generating function (OGF) and Fibonacci recurrence
            "$$\nG(x) = \\sum_{n=0}^{\\infty} F_n x^n = \\frac{x}{1 - x - x^2}\n$$",
            // Exponential generating function (EGF)
            "$$\nE(x) = \\sum_{n=0}^{\\infty} a_n \\frac{x^n}{n!} = \\exp\\left( \\sum_{k=1}^{\\infty} c_k \\frac{x^k}{k!} \\right)\n$$",
            // Stirling numbers of the second kind and partition numbers
            "$$\n\\left\\{ \\begin{matrix} n \\\\ k \\end{matrix} \\right\\} = \\frac{1}{k!} \\sum_{j=0}^k (-1)^{k-j} \\binom{k}{j} j^n\n$$",
            // Inclusion-Exclusion principle
            "$$\n\\left| \\bigcup_{i=1}^n A_i \\right| = \\sum_{k=1}^n (-1)^{k-1} \\sum_{1 \\le i_1 < \\dots < i_k \\le n} |A_{i_1} \\cap \\dots \\cap A_{i_k}|\n$$",
            // Catalan numbers formula
            "$$\nC_n = \\frac{1}{n+1} \\binom{2n}{n} = \\frac{(2n)!}{(n+1)! \\, n!}\n$$",
        ];

        for c in comb_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Combinatorics LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_hardware_description_verilog_vhdl() {
        let hdl_cases = [
            // Verilog module
            ("verilog", "// 4-bit binary counter\nmodule counter_4bit (\n    input wire clk,\n    input wire rst_n,\n    output reg [3:0] count\n);\n    always @(posedge clk or negedge rst_n) begin\n        if (!rst_n)\n            count <= 4'b0000;\n        else\n            count <= count + 1'b1;\n    end\nendmodule"),
            // VHDL entity and architecture
            ("vhdl", "-- 2-input AND gate\nlibrary IEEE;\nuse IEEE.STD_LOGIC_1164.ALL;\n\nentity and_gate is\n    Port ( a : in STD_LOGIC;\n           b : in STD_LOGIC;\n           y : out STD_LOGIC);\nend and_gate;\n\narchitecture Behavioral of and_gate is\nbegin\n    y <= a and b;\nend Behavioral;"),
        ];

        for (lang, code) in hdl_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "HDL codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_latex_complex_analysis_and_contour_integration() {
        let complex_cases = [
            // Cauchy's integral formula
            "$$\nf(z_0) = \\frac{1}{2\\pi i} \\oint_{\\gamma} \\frac{f(z)}{z - z_0} \\, dz\n$$",
            // Residue theorem
            "$$\n\\oint_{\\gamma} f(z) \\, dz = 2\\pi i \\sum_{k=1}^n \\operatorname{Res}(f, a_k)\n$$",
            // Cauchy-Riemann equations
            "$$\n\\frac{\\partial u}{\\partial x} = \\frac{\\partial v}{\\partial y}, \\quad \\frac{\\partial u}{\\partial y} = -\\frac{\\partial v}{\\partial x}\n$$",
            // Riemann zeta function and functional equation
            "$$\n\\zeta(s) = \\sum_{n=1}^{\\infty} \\frac{1}{n^s} = \\prod_{p \\text{ prime}} \\frac{1}{1 - p^{-s}}, \\quad \\zeta(s) = 2^s \\pi^{s-1} \\sin\\left(\\frac{\\pi s}{2}\\right) \\Gamma(1-s) \\zeta(1-s)\n$$",
            // Laurent series expansion
            "$$\nf(z) = \\sum_{n=-\\infty}^{\\infty} a_n (z - c)^n, \\quad a_n = \\frac{1}{2\\pi i} \\oint_{\\gamma} \\frac{f(w)}{(w - c)^{n+1}} \\, dw\n$$",
        ];

        for c in complex_cases {
            let markup = format_markdown_to_pango(c);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Complex analysis LaTeX case failed on:\n{}\nMarkup:\n{}\nError: {:?}",
                c,
                markup,
                res.err()
            );
        }
    }

    #[test]
    fn test_fuzz_codeblock_with_markup_and_stylesheet_languages() {
        let markup_cases = [
            // Modern CSS with custom properties and media queries
            ("css", ":root {\n  --primary: #3b82f6;\n  --surface: #18191b;\n}\n\n@media (prefers-color-scheme: dark) {\n  body {\n    background-color: var(--surface);\n    color: rgba(255, 255, 255, 0.9);\n  }\n}"),
            // SCSS / SASS with nesting and mixins
            ("scss", "@mixin flex-center {\n  display: flex;\n  justify-content: center;\n  align-items: center;\n}\n\n.card {\n  @include flex-center;\n  &__header {\n    font-weight: 600;\n  }\n}"),
            // HTML5 with doctype, meta, scripts, and canvas
            ("html", "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"UTF-8\" />\n  <title>Pi Overlay & Demo</title>\n</head>\n<body>\n  <canvas id=\"gl-surface\" width=\"800\" height=\"600\"></canvas>\n</body>\n</html>"),
            // SVG vector graphic definition
            ("svg", "<svg viewBox=\"0 0 100 100\" xmlns=\"http://www.w3.org/2000/svg\">\n  <circle cx=\"50\" cy=\"50\" r=\"40\" stroke=\"#8ab4f8\" stroke-width=\"4\" fill=\"none\" />\n  <path d=\"M 30 50 L 45 65 L 70 35\" stroke=\"#81c995\" stroke-width=\"4\" fill=\"none\" />\n</svg>"),
        ];

        for (lang, code) in markup_cases {
            let md = format!("```{lang}\n{code}\n```");
            let markup = format_markdown_to_pango(&md);
            let res = pango::parse_markup(&markup, '\0');
            assert!(
                res.is_ok(),
                "Stylesheet/markup codeblock case failed for lang {:?}:\nMarkup:\n{}\nError: {:?}",
                lang,
                markup,
                res.err()
            );
        }
    }
}
