//! Syntax highlighting via `syntect` (replaces the old hand-rolled keyword tables).

use std::sync::LazyLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

pub const THEME_NAME: &str = "base16-ocean.dark";

/// Maps markdown fence info strings onto syntect syntax tokens.
pub fn normalize_lang(lang: &str) -> String {
    let lower = lang.trim().to_ascii_lowercase();
    match lower.as_str() {
        "rs" => "rust",
        "py" | "python3" => "python",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "sh" | "zsh" | "shell" | "console" => "bash",
        "yml" => "yaml",
        "md" => "markdown",
        "htm" => "html",
        "h" | "hpp" | "cc" | "cxx" | "c++" => "c++",
        "golang" => "go",
        "kt" => "kotlin",
        "rb" => "ruby",
        "patch" => "diff",
        "makefile" => "make",
        other => other,
    }
    .to_string()
}

/// Renders a fenced code block into valid Pango markup using syntect colours.
pub fn highlight_to_pango(code: &str, lang: &str) -> String {
    let ps = &*SYNTAX_SET;
    let themes = &*THEME_SET;
    let theme = themes
        .themes
        .get(THEME_NAME)
        .or_else(|| themes.themes.values().next())
        .expect("syntect ships default themes");

    let token = normalize_lang(lang);
    let token = token.as_str();
    let syntax = ps
        .find_syntax_by_token(token)
        .or_else(|| ps.find_syntax_by_extension(token))
        .unwrap_or_else(|| ps.find_syntax_plain_text());

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut out = String::with_capacity(code.len() * 2);

    for line in LinesWithEndings::from(code) {
        match highlighter.highlight_line(line, ps) {
            Ok(ranges) => {
                for (style, piece) in ranges {
                    let text = glib::markup_escape_text(piece.trim_end_matches('\n'));
                    if text.is_empty() {
                        continue;
                    }
                    let c = style.foreground;
                    out.push_str(&format!(
                        "<span foreground=\"#{:02X}{:02X}{:02X}\">{}</span>",
                        c.r, c.g, c.b, text
                    ));
                }
                if line.ends_with('\n') {
                    out.push('\n');
                }
            }
            Err(_) => {
                out.push_str(&glib::markup_escape_text(line));
            }
        }
    }

    out
}

use gtk4::glib;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_lang_aliases() {
        assert_eq!(normalize_lang("rs"), "rust");
        assert_eq!(normalize_lang("Python"), "python");
        assert_eq!(normalize_lang(" tsx "), "typescript");
        assert_eq!(normalize_lang("shell"), "bash");
        assert_eq!(normalize_lang("unknownlang"), "unknownlang");
    }

    #[test]
    fn test_highlight_produces_valid_pango() {
        let code = "fn main() {\n    let x: u32 = 42;\n    println!(\"{}\", x);\n}\n";
        let pango = highlight_to_pango(code, "rust");
        assert!(pango.contains("<span foreground=\"#"));
        assert!(gtk4::pango::parse_markup(&pango, '\0').is_ok());
    }

    #[test]
    fn test_highlight_escapes_xml() {
        let pango = highlight_to_pango("if a < b && c > d { }", "rust");
        assert!(!pango.contains("< b"));
        assert!(gtk4::pango::parse_markup(&pango, '\0').is_ok());
    }

    #[test]
    fn test_highlight_plaintext_fallback() {
        let pango = highlight_to_pango("just text\n", "not-a-real-lang");
        assert!(gtk4::pango::parse_markup(&pango, '\0').is_ok());
    }

    #[test]
    fn test_highlight_preserves_line_breaks() {
        // Line structure must survive highlighting: one `\n` per source line.
        let pango = highlight_to_pango("let a = 1;\nlet b = 2;\n", "rust");
        assert_eq!(pango.matches('\n').count(), 2);
        assert!(gtk4::pango::parse_markup(&pango, '\0').is_ok());
    }
}
