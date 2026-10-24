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
}
