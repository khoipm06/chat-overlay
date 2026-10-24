use gtk4::glib;

pub fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.contains('|')
}

pub fn render_markdown_table_to_pango(table_lines: &[&str]) -> String {
    if table_lines.len() < 2 {
        return table_lines.join("\n");
    }

    let mut rows: Vec<Vec<String>> = Vec::new();

    for line in table_lines.iter() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
            continue;
        }
        let cells: Vec<String> = trimmed[1..trimmed.len() - 1]
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();

        if !is_separator_row(&cells) {
            rows.push(cells);
        }
    }

    if rows.is_empty() {
        return table_lines.join("\n");
    }

    // Determine max columns across all rows to support ragged/uneven tables
    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if num_cols == 0 {
        return table_lines.join("\n");
    }

    for r in &mut rows {
        while r.len() < num_cols {
            r.push(String::new());
        }
    }

    let mut col_widths = vec![0usize; num_cols];
    for r in &rows {
        for (c_idx, cell) in r.iter().enumerate() {
            col_widths[c_idx] = col_widths[c_idx].max(cell.chars().count());
        }
    }

    for w in &mut col_widths {
        *w = (*w).max(3);
    }

    let mut out = String::new();
    out.push_str("<span font_family=\"monospace\" size=\"smaller\" foreground=\"#a1a1aa\">");

    // Top border
    out.push_str("┌─");
    out.push_str(
        &col_widths
            .iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join("─┬─"),
    );
    out.push_str("─┐\n");

    for (row_idx, row) in rows.iter().enumerate() {
        out.push_str("│ ");
        for (c_idx, cell) in row.iter().enumerate() {
            let width = col_widths[c_idx];
            let cell_clean = glib::markup_escape_text(cell);
            let pad = " ".repeat(width.saturating_sub(cell.chars().count()));

            if row_idx == 0 {
                out.push_str(&format!(
                    "<span weight=\"bold\" foreground=\"#eeeef0\">{}</span>{}",
                    cell_clean, pad
                ));
            } else {
                out.push_str(&format!("{}{}", cell_clean, pad));
            }
            if c_idx + 1 < num_cols {
                out.push_str(" │ ");
            }
        }
        out.push_str(" │\n");

        if row_idx == 0 {
            out.push_str("├─");
            out.push_str(
                &col_widths
                    .iter()
                    .map(|w| "─".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("─┼─"),
            );
            out.push_str("─┤\n");
        }
    }

    // Bottom border
    out.push_str("└─");
    out.push_str(
        &col_widths
            .iter()
            .map(|w| "─".repeat(*w))
            .collect::<Vec<_>>()
            .join("─┴─"),
    );
    out.push_str("─┘</span>");

    out
}

fn is_separator_row(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|c| {
            let t = c.trim_matches(':');
            !t.is_empty() && t.chars().all(|ch| ch == '-')
        })
}
