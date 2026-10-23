pub use crate::render::apply_assistant_markup;
#[allow(unused_imports)]
pub use crate::render::format_markdown_to_pango;

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
        assert!(markup.contains("[python]"));
        assert!(markup.contains("def"));
        assert!(markup.contains("print"));
    }

    #[test]
    fn test_diagrams_and_details() {
        let text = "<details>\n<summary>Click to view</summary>\nContent here\n</details>\n```mermaid\nflowchart LR\nA --> B\n```";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
        assert!(markup.contains("▸ Click to view"));
        assert!(markup.contains("[mermaid]"));
    }

    #[test]
    fn test_unclosed_streaming_safety() {
        let text = "Streaming `unclosed inline\n```rust\nfn main() {";
        let markup = format_markdown_to_pango(text);
        assert!(pango::parse_markup(&markup, '\0').is_ok());
    }
}
