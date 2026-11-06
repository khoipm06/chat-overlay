use gtk4::glib;

/// Formats callouts and alerts (GitHub style: [!NOTE], [!TIP], [!WARNING], [!IMPORTANT], [!CAUTION])
pub fn format_alerts_and_quotes(line: &str) -> Option<String> {
    let trimmed = line.trim();

    if trimmed.starts_with("> [!NOTE]") {
        return Some("<span foreground=\"#60a5fa\" weight=\"bold\">ℹ Note</span>".to_string());
    } else if trimmed.starts_with("> [!TIP]") {
        return Some("<span foreground=\"#4ade80\" weight=\"bold\">💡 Tip</span>".to_string());
    } else if trimmed.starts_with("> [!WARNING]") {
        return Some("<span foreground=\"#facc15\" weight=\"bold\">⚠ Warning</span>".to_string());
    } else if trimmed.starts_with("> [!CAUTION]") {
        return Some("<span foreground=\"#f87171\" weight=\"bold\">🛑 Caution</span>".to_string());
    } else if trimmed.starts_with("> [!IMPORTANT]") {
        return Some("<span foreground=\"#c084fc\" weight=\"bold\">❗ Important</span>".to_string());
    }

    if let Some(rest) = line.strip_prefix("> > ") {
        let escaped = glib::markup_escape_text(rest);
        return Some(format!(
            "<span style=\"italic\" foreground=\"#71717a\">    │ │ {}</span>",
            escaped
        ));
    }

    if let Some(rest) = line.strip_prefix("> ") {
        let escaped = glib::markup_escape_text(rest);
        return Some(format!(
            "<span style=\"italic\" foreground=\"#a1a1aa\">  │ {}</span>",
            escaped
        ));
    }

    if trimmed.starts_with("$$") && trimmed.ends_with("$$") && trimmed.len() > 4 {
        let eq = &trimmed[2..trimmed.len() - 2].trim();
        let escaped = glib::markup_escape_text(eq);
        return Some(format!(
            "<span font_family=\"serif\" size=\"large\" style=\"italic\" foreground=\"#a5b4fc\">    {}</span>",
            escaped
        ));
    }

    None
}
