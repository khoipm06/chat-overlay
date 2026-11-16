//! Markdown table widget for the assistant response stream.
//!
//! Cell text arrives already converted to Pango inline markup by
//! [`crate::render`] (via `pulldown-cmark`), so this module only lays widgets out.

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableData {
    /// Pango inline markup per header cell.
    pub headers: Vec<String>,
    pub alignments: Vec<ColumnAlign>,
    /// Pango inline markup per body cell.
    pub rows: Vec<Vec<String>>,
}

/// Applies pre-formatted Pango markup to a label, falling back to plain text.
fn set_cell_markup(label: &gtk4::Label, markup: &str) {
    if gtk4::pango::parse_markup(markup, '\0').is_ok() {
        label.set_markup(markup);
    } else {
        label.set_text(markup);
    }
}

impl TableData {
    /// Minimum column width in `ch` units, derived from the widest cell.
    fn col_char_widths(&self) -> Vec<usize> {
        let n = self.headers.len();
        let mut widths = vec![4usize; n];
        for (c, h) in self.headers.iter().enumerate() {
            widths[c] = widths[c].max(plain_len(h));
        }
        for row in &self.rows {
            for (c, cell) in row.iter().enumerate() {
                if c < n {
                    widths[c] = widths[c].max(plain_len(cell));
                }
            }
        }
        widths
    }
}

/// Rough visible length of a Pango-markup string (tags stripped, entities kept).
fn plain_len(markup: &str) -> usize {
    let mut len = 0usize;
    let mut in_tag = false;
    for ch in markup.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => len += 1,
            _ => {}
        }
    }
    len
}

/// Builds a scrollable grid table card. Columns keep their natural width and the
/// whole grid scrolls horizontally, matching VS Code's markdown table behaviour.
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

    let widths = table.col_char_widths();

    for (c_idx, header_markup) in table.headers.iter().enumerate() {
        let align = table
            .alignments
            .get(c_idx)
            .copied()
            .unwrap_or(ColumnAlign::Left);
        let min_width = (widths[c_idx] as i32 * 8 + 24).clamp(64, 360);

        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        cell.add_css_class("table-cell-header");
        cell.set_size_request(min_width, -1);
        cell.set_hexpand(true);

        let label = gtk4::Label::builder()
            .selectable(true)
            .wrap(true)
            .wrap_mode(gtk4::pango::WrapMode::WordChar)
            .xalign(0.0)
            .halign(align.to_gtk_align())
            .hexpand(true)
            .css_classes(["table-cell-text", "table-header-text"])
            .build();
        set_cell_markup(
            &label,
            &format!("<span weight=\"bold\">{}</span>", header_markup),
        );

        cell.append(&label);
        grid.attach(&cell, c_idx as i32, 0, 1, 1);
    }

    for (r_idx, row) in table.rows.iter().enumerate() {
        let row_class = if r_idx % 2 == 0 {
            "table-row-even"
        } else {
            "table-row-odd"
        };

        for (c_idx, cell_markup) in row.iter().enumerate() {
            if c_idx >= table.headers.len() {
                break;
            }
            let align = table
                .alignments
                .get(c_idx)
                .copied()
                .unwrap_or(ColumnAlign::Left);
            let min_width = (widths[c_idx] as i32 * 8 + 24).clamp(64, 360);

            let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            cell.add_css_class("table-cell");
            cell.add_css_class(row_class);
            cell.set_size_request(min_width, -1);
            cell.set_hexpand(true);

            let label = gtk4::Label::builder()
                .selectable(true)
                .wrap(true)
                .wrap_mode(gtk4::pango::WrapMode::WordChar)
                .xalign(0.0)
                .halign(align.to_gtk_align())
                .hexpand(true)
                .css_classes(["table-cell-text"])
                .build();
            set_cell_markup(&label, cell_markup);

            cell.append(&label);
            grid.attach(&cell, c_idx as i32, (r_idx + 1) as i32, 1, 1);
        }
    }

    scrolled.set_child(Some(&grid));
    card.append(&scrolled);
    card
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TableData {
        TableData {
            headers: vec!["Command".into(), "Align".into(), "Description".into()],
            alignments: vec![
                ColumnAlign::Left,
                ColumnAlign::Center,
                ColumnAlign::Right,
            ],
            rows: vec![
                vec!["cargo build".into(), "Left".into(), "Builds release".into()],
                vec!["cargo test".into(), "Center".into(), "Runs tests".into()],
            ],
        }
    }

    #[test]
    fn test_plain_len_strips_markup() {
        assert_eq!(plain_len("<b>hi</b>"), 2);
        assert_eq!(plain_len("plain"), 5);
    }

    #[test]
    fn test_column_widths_use_widest_cell() {
        let widths = sample().col_char_widths();
        // "Command"=7, "cargo build"=11 → 11; "Align"=5, "Center"=6 → 6;
        // "Description"=11, "Builds release"=14 → 14.
        assert_eq!(widths, vec![11, 6, 14]);
    }

    #[test]
    fn test_alignments_map_to_gtk() {
        assert_eq!(ColumnAlign::Left.to_gtk_align(), gtk4::Align::Start);
        assert_eq!(ColumnAlign::Right.to_gtk_align(), gtk4::Align::End);
    }

    #[test]
    fn test_table_data_equality() {
        assert_eq!(sample(), sample());
        let mut other = sample();
        other.rows.pop();
        assert_ne!(sample(), other);
    }
}
