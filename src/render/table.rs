use gtk4::glib;
use gtk4::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnAlign {
    Left,
    Center,
    Right,
}

impl ColumnAlign {
    pub fn to_gtk_align(self) -> gtk4::Align {
        match self {
            ColumnAlign::Left => gtk4::Align::Start,
            ColumnAlign::Center => gtk4::Align::Center,
            ColumnAlign::Right => gtk4::Align::End,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableData {
    pub headers: Vec<String>,
    pub alignments: Vec<ColumnAlign>,
    pub rows: Vec<Vec<String>>,
}

/// Checks if a line matches markdown table syntax
pub fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 2 {
        return true;
    }
    // GFM allows tables without leading/trailing pipes
    if trimmed.contains('|') && !trimmed.starts_with("```") {
        return true;
    }
    false
}

fn split_cells(trimmed: &str) -> Vec<String> {
    let s = if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() >= 2 {
        &trimmed[1..trimmed.len() - 1]
    } else if let Some(stripped) = trimmed.strip_prefix('|') {
        stripped
    } else if let Some(stripped) = trimmed.strip_suffix('|') {
        stripped
    } else {
        trimmed
    };
    s.split('|')
        .map(|c| c.trim().to_string())
        .collect()
}

pub fn parse_table_data(table_lines: &[&str]) -> Option<TableData> {
    if table_lines.len() < 2 {
        return None;
    }

    let mut raw_rows: Vec<Vec<String>> = Vec::new();
    let mut separator_idx: Option<usize> = None;
    let mut alignments: Vec<ColumnAlign> = Vec::new();

    for line in table_lines {
        let trimmed = line.trim();
        if !is_table_row(trimmed) {
            continue;
        }
        let cells = split_cells(trimmed);
        if is_separator_row(&cells) {
            if separator_idx.is_none() {
                separator_idx = Some(raw_rows.len());
                alignments = cells
                    .iter()
                    .map(|c| {
                        let t = c.trim();
                        let starts = t.starts_with(':');
                        let ends = t.ends_with(':');
                        if starts && ends {
                            ColumnAlign::Center
                        } else if ends {
                            ColumnAlign::Right
                        } else {
                            ColumnAlign::Left
                        }
                    })
                    .collect();
            }
        } else {
            raw_rows.push(cells);
        }
    }

    if raw_rows.is_empty() {
        return None;
    }

    let num_cols = raw_rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols < 2 {
        return None;
    }

    // Default alignments if no separator row found
    while alignments.len() < num_cols {
        alignments.push(ColumnAlign::Left);
    }

    for r in &mut raw_rows {
        while r.len() < num_cols {
            r.push(String::new());
        }
    }

    let headers = raw_rows.remove(0);
    Some(TableData {
        headers,
        alignments,
        rows: raw_rows,
    })
}

/// Builds a real, responsive GTK4 Grid table widget with horizontal scrolling.
/// Modeled after VS Code's `chatMarkdownTableScrolling.ts`:
/// Measures maximum character length per column to enforce minimum column widths,
/// preventing columns from squeezing, while supporting horizontal scrolling in a card.
pub fn build_table_widget(table: &TableData) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("markdown-table-card");
    card.set_hexpand(true);

    let scrolled = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Automatic)
        .vscrollbar_policy(gtk4::PolicyType::Never)
        .propagate_natural_width(false)
        .propagate_natural_height(true)
        .css_classes(["markdown-table-scrolled"])
        .build();

    let grid = gtk4::Grid::builder()
        .column_spacing(0)
        .row_spacing(0)
        .css_classes(["markdown-table-grid"])
        .build();

    let num_cols = table.headers.len();

    // Calculate maximum characters per column (in ch units, like VS Code table scrolling)
    let mut col_char_widths = vec![4usize; num_cols];
    for (c, h) in table.headers.iter().enumerate() {
        col_char_widths[c] = col_char_widths[c].max(h.chars().count());
    }
    for r in &table.rows {
        for (c, cell) in r.iter().enumerate() {
            if c < num_cols {
                col_char_widths[c] = col_char_widths[c].max(cell.chars().count());
            }
        }
    }

    // 1. Render Header Cells
    for (c_idx, header_text) in table.headers.iter().enumerate() {
        let align = table.alignments.get(c_idx).copied().unwrap_or(ColumnAlign::Left);
        let col_width = ((col_char_widths[c_idx] as i32) * 8 + 24).clamp(64, 360);

        let cell_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        cell_box.add_css_class("table-cell-header");
        cell_box.set_size_request(col_width, -1);
        cell_box.set_hexpand(true);
        cell_box.set_halign(gtk4::Align::Fill);

        let label = gtk4::Label::builder()
            .selectable(true)
            .css_classes(["table-cell-text", "table-header-text"])
            .halign(align.to_gtk_align())
            .hexpand(true)
            .build();

        let inline_markup = crate::render::inline::format_inline_elements(header_text);
        let pango = format!("<span weight=\"bold\" foreground=\"#f4f4f5\">{}</span>", inline_markup);
        if gtk4::pango::parse_markup(&pango, '\0').is_ok() {
            label.set_markup(&pango);
        } else {
            label.set_text(header_text);
        }

        cell_box.append(&label);
        grid.attach(&cell_box, c_idx as i32, 0, 1, 1);
    }

    // 2. Render Body Rows
    for (r_idx, row) in table.rows.iter().enumerate() {
        let is_even = r_idx % 2 == 0;
        let row_class = if is_even { "table-row-even" } else { "table-row-odd" };

        for (c_idx, cell_text) in row.iter().enumerate() {
            let align = table.alignments.get(c_idx).copied().unwrap_or(ColumnAlign::Left);
            let col_width = ((col_char_widths[c_idx] as i32) * 8 + 24).clamp(64, 360);

            let cell_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            cell_box.add_css_class("table-cell");
            cell_box.add_css_class(row_class);
            cell_box.set_size_request(col_width, -1);
            cell_box.set_hexpand(true);
            cell_box.set_halign(gtk4::Align::Fill);

            let label = gtk4::Label::builder()
                .selectable(true)
                .css_classes(["table-cell-text"])
                .halign(align.to_gtk_align())
                .hexpand(true)
                .build();

            let inline_markup = crate::render::inline::format_inline_elements(cell_text);
            if gtk4::pango::parse_markup(&inline_markup, '\0').is_ok() {
                label.set_markup(&inline_markup);
            } else {
                label.set_text(cell_text);
            }

            cell_box.append(&label);
            grid.attach(&cell_box, c_idx as i32, (r_idx + 1) as i32, 1, 1);
        }
    }

    let _ = num_cols;
    scrolled.set_child(Some(&grid));
    card.append(&scrolled);
    card
}

/// Fallback text renderer for non-widget contexts (logs, tooltips, tests)
pub fn render_markdown_table(table_lines: &[&str]) -> String {
    if let Some(table) = parse_table_data(table_lines) {
        let num_cols = table.headers.len();
        if num_cols == 0 {
            return table_lines.join("\n");
        }

        let mut all_rows = Vec::new();
        all_rows.push(table.headers.clone());
        all_rows.extend(table.rows.clone());

        let mut col_widths = vec![3usize; num_cols];
        for r in &all_rows {
            for (c_idx, cell) in r.iter().enumerate() {
                col_widths[c_idx] = col_widths[c_idx].max(cell.chars().count().max(3));
            }
        }

        let mut out = String::new();
        out.push_str("<span font_family=\"monospace\" size=\"small\" foreground=\"#a1a1aa\">");

        for (row_idx, row) in all_rows.iter().enumerate() {
            out.push_str("  ");
            for (c_idx, cell) in row.iter().enumerate() {
                let width = col_widths[c_idx];
                let cell_clean = glib::markup_escape_text(cell);
                let pad = " ".repeat(width.saturating_sub(cell.chars().count()));

                if row_idx == 0 {
                    out.push_str(&format!(
                        "<span weight=\"bold\" foreground=\"#f4f4f5\">{}</span>{}",
                        cell_clean, pad
                    ));
                } else {
                    out.push_str(&format!("{}{}", cell_clean, pad));
                }
                if c_idx + 1 < num_cols {
                    out.push_str("  <span foreground=\"#3f3f46\">│</span>  ");
                }
            }
            out.push('\n');

            if row_idx == 0 {
                out.push_str("  ");
                let sep_line: Vec<String> = col_widths.iter().map(|w| "─".repeat(*w)).collect();
                out.push_str(&format!(
                    "<span foreground=\"#3f3f46\">{}</span>\n",
                    sep_line.join("──┼──")
                ));
            }
        }

        out.push_str("</span>");
        out
    } else {
        table_lines.join("\n")
    }
}

fn is_separator_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let t = c.trim_matches(':');
            !t.is_empty() && t.chars().all(|ch| ch == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_table_data() {
        let lines = vec![
            "| Command | Alignment | Description |",
            "|:---|:---:|---:|",
            "| `cargo build` | Left | Builds release |",
            "| `cargo test` | Center | Runs test suite |",
        ];
        let data = parse_table_data(&lines).expect("Should parse table data");
        assert_eq!(data.headers, vec!["Command", "Alignment", "Description"]);
        assert_eq!(
            data.alignments,
            vec![ColumnAlign::Left, ColumnAlign::Center, ColumnAlign::Right]
        );
        assert_eq!(data.rows.len(), 2);
        assert_eq!(data.rows[0][0], "`cargo build`");
        assert_eq!(data.rows[1][2], "Runs test suite");
    }

    #[test]
    fn test_render_table_fallback() {
        let lines = vec![
            "| Command | Description |",
            "|---|---|",
            "| `cargo build` | Builds release |",
            "| `cargo test` | Runs test suite |",
        ];
        let rendered = render_markdown_table(&lines);
        assert!(rendered.contains("Command"));
        assert!(rendered.contains("Builds release"));
        assert!(rendered.contains("──┼──"));
    }

    #[test]
    fn test_parse_table_data_edge_cases() {
        assert!(parse_table_data(&["| single |"]).is_none());

        let lines = vec![
            "| Header 1 | Header 2 |",
            "|---|---|",
            "| Val 1 |",
        ];
        let data = parse_table_data(&lines).expect("Should pad uneven row");
        assert_eq!(data.rows.len(), 1);
        assert_eq!(data.rows[0].len(), 2);
        assert_eq!(data.rows[0][0], "Val 1");
        assert_eq!(data.rows[0][1], "");

        let lines_no_sep = vec![
            "| Header 1 | Header 2 |",
            "| Val 1 | Val 2 |",
        ];
        let data_no_sep = parse_table_data(&lines_no_sep).expect("Should parse even without sep");
        assert_eq!(data_no_sep.headers, vec!["Header 1", "Header 2"]);
        assert_eq!(data_no_sep.rows.len(), 1);

        let lines_no_enclosing = vec![
            "Col A | Col B | Col C",
            "--- | :---: | ---:",
            "Item 1 | Center 2 | Right 3",
        ];
        let data_no_enclosing =
            parse_table_data(&lines_no_enclosing).expect("Should parse compact un-enclosed table");
        assert_eq!(data_no_enclosing.headers, vec!["Col A", "Col B", "Col C"]);
        assert_eq!(
            data_no_enclosing.alignments,
            vec![ColumnAlign::Left, ColumnAlign::Center, ColumnAlign::Right]
        );
        assert_eq!(data_no_enclosing.rows[0], vec!["Item 1", "Center 2", "Right 3"]);
    }
}
