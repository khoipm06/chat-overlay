//! Markdown → GTK4 widget rendering.
//!
//! Parsing is delegated to `pulldown-cmark` (CommonMark + GFM tables /
//! strikethrough / task lists); syntax highlighting to `syntect`. Rendering is
//! incremental: during streaming only the tail blocks that actually changed are
//! rebuilt, so finished blocks are never destroyed (no flicker, selection kept).

pub mod syntax;
pub mod table;

use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownBlock {
    /// Inline markup for a paragraph.
    Paragraph(String),
    /// Heading level (1–6) plus inline markup.
    Heading(u8, String),
    /// Inline markup for a block quote body.
    BlockQuote(String),
    /// Items of `(marker, inline markup)`.
    List(Vec<(String, String)>),
    Code { lang: String, code: String },
    Table(table::TableData),
    Rule,
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Heading Pango size for a level.
fn heading_size(level: u8) -> &'static str {
    match level {
        1 => "x-large",
        2 => "large",
        3 => "medium",
        _ => "small",
    }
}

/// Accumulates inline Pango markup for the open inline container.
#[derive(Default)]
struct InlineBuf {
    out: String,
    links: Vec<String>,
    bold: u32,
    italic: u32,
    strike: u32,
}

impl InlineBuf {
    fn is_blank(&self) -> bool {
        self.out.trim().is_empty()
    }

    /// Drains the buffer, resetting the formatting state.
    fn take(&mut self) -> String {
        self.bold = 0;
        self.italic = 0;
        self.strike = 0;
        self.links.clear();
        std::mem::take(&mut self.out)
    }

    /// Appends `text`, escaped, wrapped in the currently open inline tags.
    fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let mut open = String::new();
        let mut close = String::new();
        if self.strike > 0 {
            open.push_str("<s>");
            close.insert_str(0, "</s>");
        }
        if self.bold > 0 {
            open.push_str("<b>");
            close.insert_str(0, "</b>");
        }
        if self.italic > 0 {
            open.push_str("<i>");
            close.insert_str(0, "</i>");
        }
        if !self.links.is_empty() {
            open.push_str("<span foreground=\"#3794FF\" underline=\"single\">");
            close.insert_str(0, "</span>");
        }
        self.out.push_str(&open);
        self.out.push_str(&glib::markup_escape_text(text));
        self.out.push_str(&close);
    }

    /// Appends a pre-formed markup fragment verbatim (already escaped by caller).
    fn push_raw(&mut self, markup: &str) {
        self.out.push_str(markup);
    }
}

#[derive(Default)]
struct TableBuf {
    headers: Vec<String>,
    alignments: Vec<table::ColumnAlign>,
    rows: Vec<Vec<String>>,
    row: Vec<String>,
    in_head: bool,
}

fn alignment_from(cmark: pulldown_cmark::Alignment) -> table::ColumnAlign {
    match cmark {
        pulldown_cmark::Alignment::Center => table::ColumnAlign::Center,
        pulldown_cmark::Alignment::Right => table::ColumnAlign::Right,
        _ => table::ColumnAlign::Left,
    }
}

/// Parses markdown into render-ready blocks.
pub fn parse_markdown_blocks(input: &str) -> Vec<MarkdownBlock> {
    let mut blocks: Vec<MarkdownBlock> = Vec::new();
    let mut inline = InlineBuf::default();
    let mut list_items: Vec<(String, String)> = Vec::new();
    let mut list_marker: Option<String> = None;
    let mut list_depth = 0usize;
    let mut code: Option<(String, String)> = None;
    let mut quote_depth = 0usize;
    let mut table: Option<TableBuf> = None;
    let mut heading: Option<u8> = None;

    macro_rules! flush_inline {
        () => {
            if !inline.is_blank() {
                let text = inline.take();
                if quote_depth > 0 {
                    blocks.push(MarkdownBlock::BlockQuote(text));
                } else {
                    blocks.push(MarkdownBlock::Paragraph(text));
                }
            } else {
                inline.take();
            }
        };
    }

    macro_rules! flush_list {
        () => {
            if !list_items.is_empty() {
                blocks.push(MarkdownBlock::List(std::mem::take(&mut list_items)));
            }
        };
    }

    for event in Parser::new_ext(input, parser_options()) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {}
                Tag::Heading { level, .. } => heading = Some(heading_level(level)),
                Tag::BlockQuote(_) => quote_depth += 1,
                Tag::CodeBlock(kind) => {
                    flush_inline!();
                    flush_list!();
                    let lang = match kind {
                        CodeBlockKind::Fenced(info) => info
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code = Some((lang, String::new()));
                }
                Tag::List(start) => {
                    list_depth += 1;
                    if list_depth == 1 {
                        list_marker = Some(match start {
                            Some(n) => format!("{n}."),
                            None => "\u{2022}".to_string(),
                        });
                    }
                }
                Tag::Item => flush_inline!(),
                Tag::Table(aligns) => {
                    flush_inline!();
                    flush_list!();
                    table = Some(TableBuf {
                        alignments: aligns.iter().copied().map(alignment_from).collect(),
                        ..Default::default()
                    });
                }
                Tag::TableHead => {
                    if let Some(t) = table.as_mut() {
                        t.in_head = true;
                        t.row.clear();
                    }
                }
                Tag::TableRow => {
                    if let Some(t) = table.as_mut() {
                        t.row.clear();
                    }
                }
                Tag::TableCell => {
                    if let Some(t) = table.as_mut() {
                        t.row.push(String::new());
                    }
                }
                Tag::Emphasis => inline.italic += 1,
                Tag::Strong => inline.bold += 1,
                Tag::Strikethrough => inline.strike += 1,
                Tag::Link { dest_url, .. } => inline.links.push(dest_url.to_string()),
                Tag::Image { dest_url, .. } => inline.push(&format!("[{dest_url}]")),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    if list_depth == 0 && table.is_none() && code.is_none() {
                        flush_inline!();
                    }
                }
                TagEnd::Heading(_) => {
                    let body = inline.take();
                    if !body.trim().is_empty() {
                        blocks.push(MarkdownBlock::Heading(heading.take().unwrap_or(1), body));
                    }
                    heading = None;
                }
                TagEnd::BlockQuote(_) => {
                    quote_depth = quote_depth.saturating_sub(1);
                    flush_inline!();
                }
                TagEnd::CodeBlock => {
                    if let Some((lang, body)) = code.take() {
                        blocks.push(MarkdownBlock::Code {
                            lang,
                            code: body.trim_end_matches('\n').to_string(),
                        });
                    }
                }
                TagEnd::List(_) => {
                    list_depth = list_depth.saturating_sub(1);
                    if list_depth == 0 {
                        flush_list!();
                        list_marker = None;
                    }
                }
                TagEnd::Item => {
                    let body = inline.take();
                    if !body.trim().is_empty() {
                        let marker = list_marker.clone().unwrap_or_else(|| "\u{2022}".into());
                        list_items.push((marker, body));
                    }
                }
                TagEnd::Table => {
                    if let Some(t) = table.take() {
                        let (headers, rows) = if t.headers.is_empty() {
                            // No explicit head row: promote the first body row.
                            let mut rows = t.rows;
                            let headers = if rows.is_empty() {
                                Vec::new()
                            } else {
                                rows.remove(0)
                            };
                            (headers, rows)
                        } else {
                            (t.headers, t.rows)
                        };
                        let mut alignments = t.alignments;
                        while alignments.len() < headers.len() {
                            alignments.push(table::ColumnAlign::Left);
                        }
                        blocks.push(MarkdownBlock::Table(table::TableData {
                            headers,
                            alignments,
                            rows,
                        }));
                    }
                }
                TagEnd::TableHead => {
                    if let Some(t) = table.as_mut() {
                        t.headers = std::mem::take(&mut t.row);
                        t.in_head = false;
                    }
                }
                TagEnd::TableRow => {
                    if let Some(t) = table.as_mut() {
                        let row = std::mem::take(&mut t.row);
                        if !t.in_head {
                            t.rows.push(row);
                        }
                    }
                }
                TagEnd::TableCell => {
                    if let Some(t) = table.as_mut() {
                        let cell = inline.take();
                        if let Some(last) = t.row.last_mut() {
                            *last = cell;
                        }
                    }
                }
                TagEnd::Emphasis => inline.italic = inline.italic.saturating_sub(1),
                TagEnd::Strong => inline.bold = inline.bold.saturating_sub(1),
                TagEnd::Strikethrough => inline.strike = inline.strike.saturating_sub(1),
                TagEnd::Link => {
                    inline.links.pop();
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some((_, body)) = code.as_mut() {
                    body.push_str(&text);
                } else {
                    // Covers both plain paragraphs and table cells; the table
                    // cell is committed when its `TagEnd::TableCell` arrives.
                    inline.push(&text);
                }
            }
            Event::Code(text) => {
                inline.push_raw(&format!(
                    "<span font_family=\"monospace\" background=\"#2D2D2D\">{}</span>",
                    glib::markup_escape_text(&text)
                ));
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                if code.is_none() && table.is_none() {
                    inline.push(html.trim_end_matches('\n'));
                }
            }
            Event::SoftBreak => {
                if code.is_none() && table.is_none() {
                    inline.out.push(' ');
                }
            }
            Event::HardBreak => {
                if code.is_none() && table.is_none() {
                    inline.out.push('\n');
                }
            }
            Event::Rule => {
                flush_inline!();
                flush_list!();
                blocks.push(MarkdownBlock::Rule);
            }
            Event::TaskListMarker(checked) => {
                inline.out.push_str(if checked { "\u{2611} " } else { "\u{2610} " });
            }
            Event::FootnoteReference(name) => {
                inline
                    .out
                    .push_str(&format!("<sup>{}</sup>", glib::markup_escape_text(&name)));
            }
            Event::InlineMath(tex) | Event::DisplayMath(tex) => inline.push(&tex),
        }
    }

    flush_inline!();
    flush_list!();

    if let Some((lang, body)) = code {
        blocks.push(MarkdownBlock::Code {
            lang,
            code: body.trim_end_matches('\n').to_string(),
        });
    }

    blocks
}

/// Builds the widget for one parsed block.
pub fn build_block_widget(block: &MarkdownBlock) -> gtk4::Widget {
    match block {
        MarkdownBlock::Paragraph(markup) => text_label(markup, &["bubble-assistant-text"]).upcast(),
        MarkdownBlock::BlockQuote(markup) => {
            let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            card.add_css_class("markdown-quote");
            card.append(&text_label(markup, &["bubble-assistant-text"]));
            card.upcast()
        }
        MarkdownBlock::Heading(level, markup) => text_label(
            &format!(
                "<span size=\"{}\" weight=\"bold\">{}</span>",
                heading_size(*level),
                markup
            ),
            &["bubble-assistant-heading"],
        )
        .upcast(),
        MarkdownBlock::List(items) => {
            let list = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
            list.add_css_class("markdown-list");
            for (marker, item) in items {
                let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
                let bullet = gtk4::Label::builder()
                    .label(marker)
                    .xalign(0.0)
                    .valign(gtk4::Align::Start)
                    .css_classes(["markdown-list-marker"])
                    .build();
                row.append(&bullet);
                row.append(&text_label(item, &["bubble-assistant-text"]));
                list.append(&row);
            }
            list.upcast()
        }
        MarkdownBlock::Code { lang, code } => build_code_card(lang, code).upcast(),
        MarkdownBlock::Table(data) => table::build_table_widget(data).upcast(),
        MarkdownBlock::Rule => {
            let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
            sep.add_css_class("markdown-rule");
            sep.upcast()
        }
    }
}

/// Text label: wraps at word boundaries, expands horizontally, no width cap.
fn text_label(markup: &str, classes: &[&str]) -> gtk4::Label {
    let label = gtk4::Label::builder()
        .selectable(true)
        .wrap(true)
        .wrap_mode(pango::WrapMode::WordChar)
        .xalign(0.0)
        .hexpand(true)
        .css_classes(classes.to_vec())
        .build();
    apply_markup_or_text(&label, markup);
    label
}

/// Sets markup when it parses, otherwise falls back to the raw text.
fn apply_markup_or_text(label: &gtk4::Label, markup: &str) {
    if pango::parse_markup(markup, '\0').is_ok() {
        label.set_markup(markup);
    } else {
        label.set_text(markup);
    }
}

fn build_code_card(lang: &str, code: &str) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("code-block-container");
    card.set_hexpand(true);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.add_css_class("code-block-header");

    let badge_text = if lang.trim().is_empty() {
        "CODE".to_string()
    } else {
        lang.trim().to_uppercase()
    };
    header.append(
        &gtk4::Label::builder()
            .label(&badge_text)
            .xalign(0.0)
            .css_classes(["code-block-lang"])
            .build(),
    );

    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);

    let copy_btn = gtk4::Button::builder()
        .label("Copy")
        .css_classes(["code-block-copy-btn"])
        .tooltip_text("Copy snippet")
        .build();
    {
        let code = code.to_string();
        let btn = copy_btn.clone();
        copy_btn.connect_clicked(move |_| {
            if let Some(display) = gtk4::gdk::Display::default() {
                display.clipboard().set_text(&code);
            }
            btn.set_label("\u{2713} Copied");
            let btn_reset = btn.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                btn_reset.set_label("Copy");
            });
        });
    }
    header.append(&copy_btn);
    card.append(&header);

    let highlighted = syntax::highlight_to_pango(code, lang);
    let body: String = if highlighted.trim().is_empty() {
        glib::markup_escape_text(code).to_string()
    } else {
        highlighted
    };
    let wrapped = format!("<span font_family=\"monospace\" size=\"small\">{body}</span>");

    let code_label = gtk4::Label::builder()
        .selectable(true)
        .wrap(false)
        .xalign(0.0)
        .css_classes(["code-block-content"])
        .build();
    apply_markup_or_text(&code_label, &wrapped);

    let scroll = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .propagate_natural_width(false)
        .build();
    scroll.set_child(Some(&code_label));
    card.append(&scroll);
    card
}

/// Removes every child of `container`.
fn clear_container(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

/// Renders markdown, rebuilding everything. Use for one-shot final renders.
pub fn render_markdown_blocks_into(container: &gtk4::Box, raw: &str) {
    clear_container(container);
    for block in parse_markdown_blocks(raw) {
        container.append(&build_block_widget(&block));
    }
}

/// Streams markdown into `container`, reusing widgets for the unchanged prefix.
///
/// `prev_blocks` is the parse result of the previous delta; only blocks from the
/// first difference onward are rebuilt, so completed content never flickers and
/// text selection survives.
pub fn render_markdown_streaming_into(
    container: &gtk4::Box,
    prev_blocks: &mut Vec<MarkdownBlock>,
    raw: &str,
) {
    let new_blocks = parse_markdown_blocks(raw);

    // A leading `\u{0}`-free parse keeps completed blocks byte-identical as text
    // is appended, so the shared prefix is exactly the set of finished blocks.
    let stable_len = prev_blocks
        .iter()
        .zip(new_blocks.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let existing: Vec<gtk4::Widget> = {
        let mut v = Vec::new();
        let mut child = container.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            v.push(w);
        }
        v
    };

    for widget in existing.iter().skip(stable_len) {
        container.remove(widget);
    }

    for block in new_blocks.iter().skip(stable_len) {
        container.append(&build_block_widget(block));
    }

    *prev_blocks = new_blocks;
}

/// Formats markdown as a single Pango markup string (logs, tooltips, tests).
#[cfg_attr(not(test), allow(dead_code))]
pub fn format_markdown_to_pango(input: &str) -> String {
    let mut out = String::new();
    for block in parse_markdown_blocks(input) {
        match block {
            MarkdownBlock::Heading(level, markup) => out.push_str(&format!(
                "<span size=\"{}\" weight=\"bold\">{}</span>\n",
                heading_size(level),
                markup
            )),
            MarkdownBlock::Paragraph(markup) | MarkdownBlock::BlockQuote(markup) => {
                out.push_str(&markup);
                out.push('\n');
            }
            MarkdownBlock::List(items) => {
                for (marker, item) in items {
                    out.push_str(&format!("{marker} {item}\n"));
                }
            }
            MarkdownBlock::Code { code, .. } => {
                out.push_str(&format!(
                    "<span font_family=\"monospace\">{}</span>\n",
                    glib::markup_escape_text(&code)
                ));
            }
            MarkdownBlock::Table(data) => {
                let cell = |s: &str| glib::markup_escape_text(s).to_string();
                let header: Vec<String> = data.headers.iter().map(|h| cell(h)).collect();
                out.push_str(&header.join(" \u{2502} "));
                out.push('\n');
                for row in &data.rows {
                    let cells: Vec<String> = row.iter().map(|c| cell(c)).collect();
                    out.push_str(&cells.join(" \u{2502} "));
                    out.push('\n');
                }
            }
            MarkdownBlock::Rule => out.push_str("\u{2500}\u{2500}\u{2500}\u{2500}\n"),
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(blocks: &[MarkdownBlock]) -> Vec<&'static str> {
        blocks
            .iter()
            .map(|b| match b {
                MarkdownBlock::Paragraph(_) => "p",
                MarkdownBlock::Heading(..) => "h",
                MarkdownBlock::BlockQuote(_) => "q",
                MarkdownBlock::List(_) => "l",
                MarkdownBlock::Code { .. } => "c",
                MarkdownBlock::Table(_) => "t",
                MarkdownBlock::Rule => "r",
            })
            .collect()
    }

    #[test]
    fn test_parse_code_block_keeps_language() {
        let blocks = parse_markdown_blocks("Intro\n```rust\nfn main() {}\n```\nAfter");
        assert_eq!(kinds(&blocks), vec!["p", "c", "p"]);
        match &blocks[1] {
            MarkdownBlock::Code { lang, code } => {
                assert_eq!(lang, "rust");
                assert_eq!(code, "fn main() {}");
            }
            other => panic!("expected code, got {other:?}"),
        }
    }

    #[test]
    fn test_unterminated_fence_streams_as_code() {
        let blocks = parse_markdown_blocks("Intro\n```python\nprint('x')");
        assert_eq!(kinds(&blocks), vec!["p", "c"]);
        match &blocks[1] {
            MarkdownBlock::Code { lang, code } => {
                assert_eq!(lang, "python");
                assert_eq!(code, "print('x')");
            }
            other => panic!("expected code, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_heading_and_inline_semantics() {
        let blocks = parse_markdown_blocks("## Title **bold**\n\nBody with `code`.");
        assert_eq!(kinds(&blocks), vec!["h", "p"]);
        match &blocks[0] {
            MarkdownBlock::Heading(2, markup) => assert!(markup.contains("<b>bold</b>")),
            other => panic!("expected h2, got {other:?}"),
        }
        match &blocks[1] {
            MarkdownBlock::Paragraph(markup) => {
                assert!(markup.contains("font_family=\"monospace\""))
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_table_with_alignment() {
        let blocks = parse_markdown_blocks("| A | B |\n|:--|--:|\n| 1 | 2 |");
        assert_eq!(kinds(&blocks), vec!["t"]);
        match &blocks[0] {
            MarkdownBlock::Table(t) => {
                assert_eq!(t.headers, vec!["A", "B"]);
                assert_eq!(t.rows, vec![vec!["1".to_string(), "2".to_string()]]);
                assert_eq!(t.alignments[0], table::ColumnAlign::Left);
                assert_eq!(t.alignments[1], table::ColumnAlign::Right);
            }
            other => panic!("expected table, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_list_items() {
        let blocks = parse_markdown_blocks("- one\n- two");
        let items = match blocks.iter().find(|b| matches!(b, MarkdownBlock::List(_))) {
            Some(MarkdownBlock::List(items)) => items.clone(),
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "\u{2022}");
        assert_eq!(items[0].1, "one");
        assert_eq!(items[1].1, "two");
    }

    #[test]
    fn test_ordered_list_marker_uses_index() {
        let blocks = parse_markdown_blocks("1. first\n2. second");
        match blocks.iter().find(|b| matches!(b, MarkdownBlock::List(_))) {
            Some(MarkdownBlock::List(items)) => assert_eq!(items[0].0, "1."),
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn test_blockquote_and_rule() {
        let blocks = parse_markdown_blocks("> quoted\n\n---\n");
        assert_eq!(kinds(&blocks), vec!["q", "r"]);
    }

    #[test]
    fn test_task_list_marker_rendered() {
        let blocks = parse_markdown_blocks("- [x] done\n- [ ] todo");
        let items = match blocks.iter().find(|b| matches!(b, MarkdownBlock::List(_))) {
            Some(MarkdownBlock::List(items)) => items.clone(),
            other => panic!("expected list, got {other:?}"),
        };
        assert!(items[0].1.contains('\u{2611}'));
        assert!(items[1].1.contains('\u{2610}'));
    }

    #[test]
    fn test_strikethrough_and_escape_are_valid_pango() {
        let blocks = parse_markdown_blocks("~~gone~~ and 5 < 6 & 7 > 6");
        match &blocks[0] {
            MarkdownBlock::Paragraph(markup) => {
                assert!(markup.contains("<s>gone</s>"));
                assert!(!markup.contains("< 6"));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
        let pango = format_markdown_to_pango("~~gone~~ and 5 < 6 & 7 > 6");
        assert!(pango::parse_markup(&pango, '\0').is_ok(), "got {pango}");
    }

    #[test]
    fn test_link_gets_blue_underline() {
        let blocks = parse_markdown_blocks("[site](https://example.com)");
        match &blocks[0] {
            MarkdownBlock::Paragraph(markup) => {
                assert!(markup.contains("#3794FF"));
                assert!(markup.contains("underline=\"single\""));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn test_markdown_to_pango_is_always_valid() {
        let samples = [
            "# H1\n## H2\n### H3",
            "> quoted **text**",
            "| a | b |\n|---|---|\n| 1 | 2 |",
            "```rust\nfn main() {}\n```",
            "---",
            "- [x] done\n- [ ] todo",
            "[link](https://example.com) and ![img](x.png)",
            "Text with <html> & entities",
        ];
        for s in samples {
            let pango = format_markdown_to_pango(s);
            assert!(
                pango::parse_markup(&pango, '\0').is_ok(),
                "invalid pango for {s:?}: {pango}"
            );
        }
    }

    #[test]
    fn test_completed_blocks_stay_identical_while_streaming() {
        // The incremental renderer's invariant: finished blocks must parse to
        // exactly the same value as more text is appended.
        let a = "First paragraph.\n\nSecond para";
        let b = "First paragraph.\n\nSecond paragraph continues.";
        let blocks_a = parse_markdown_blocks(a);
        let blocks_b = parse_markdown_blocks(b);
        let stable = blocks_a
            .iter()
            .zip(blocks_b.iter())
            .take_while(|(x, y)| x == y)
            .count();
        assert_eq!(stable, 1, "first paragraph must stay byte-identical");
    }

    #[test]
    fn test_unclosed_code_fence_keeps_prefix_stable() {
        let a = "Before.\n\n```rust\nlet x = 1;";
        let b = "Before.\n\n```rust\nlet x = 1;\nlet y = 2;";
        let blocks_a = parse_markdown_blocks(a);
        let blocks_b = parse_markdown_blocks(b);
        let stable = blocks_a
            .iter()
            .zip(blocks_b.iter())
            .take_while(|(x, y)| x == y)
            .count();
        assert_eq!(stable, 1, "paragraph before the fence must not be rebuilt");
    }

    #[test]
    fn test_syntax_highlight_integration_is_valid_pango() {
        let blocks = parse_markdown_blocks("```rs\nlet x = 1 < 2;\n```");
        match &blocks[0] {
            MarkdownBlock::Code { lang, code } => {
                let highlighted = syntax::highlight_to_pango(code, lang);
                assert!(pango::parse_markup(&highlighted, '\0').is_ok());
            }
            other => panic!("expected code, got {other:?}"),
        }
    }
}
