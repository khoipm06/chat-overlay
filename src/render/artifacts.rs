//! Structured deliverable (artifact) detection: turns bulky structured outputs in the
//! assistant markdown — unified diffs, task lists, test results, file exports, implementation
//! plans — into summary cards with expandable detail (MASTER_SPEC §1.2, layer 3).

/// Kind of structured deliverable detected in the assistant output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverableKind {
    Plan,
    TaskList,
    Diff,
    TestResults,
    FileExport,
}

impl DeliverableKind {
    pub fn title(&self) -> &'static str {
        match self {
            DeliverableKind::Plan => "Implementation plan",
            DeliverableKind::TaskList => "Task list",
            DeliverableKind::Diff => "Unified diff",
            DeliverableKind::TestResults => "Test results",
            DeliverableKind::FileExport => "File export",
        }
    }
}

/// One structured deliverable promoted to a summary card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deliverable {
    pub kind: DeliverableKind,
    pub summary: String,
    /// Raw markdown/code content shown when the card is expanded.
    pub content: String,
    /// Language hint for syntax highlighting (empty for prose kinds).
    pub lang: String,
    /// Exact source text the card replaces in the conversation label.
    pub source: String,
}

/// Summary label shown on the collapsed card (the GTK Expander adds its own arrow).
pub fn deliverable_card_label(d: &Deliverable) -> String {
    format!("{} · {} ▾", d.kind.title(), d.summary)
}

/// Pango markup for the expanded card detail.
pub fn render_deliverable_detail(d: &Deliverable) -> String {
    match d.kind {
        DeliverableKind::Diff | DeliverableKind::TestResults | DeliverableKind::FileExport => {
            let highlighted = crate::render::syntax::highlight_code_to_pango(&d.lang, &d.content);
            format!(
                "<span font_family=\"monospace\" size=\"small\" background=\"#141517\" foreground=\"#e8e9ed\">{}</span>",
                highlighted
            )
        }
        DeliverableKind::TaskList | DeliverableKind::Plan => {
            crate::render::format_markdown_to_pango(&d.content)
        }
    }
}

/// Extract structured deliverables, in document order, from raw assistant markdown.
pub fn extract_deliverables(markdown: &str) -> Vec<Deliverable> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();

        // 1. Fenced code block -> unified diff / test results / file export
        if trimmed.starts_with("```") {
            let lang = trimmed.trim_start_matches('`').trim();
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                j += 1;
            }
            let end = j.min(lines.len() - 1); // unclosed fence: run to EOF
            let has_closing_fence = j < lines.len();
            let content_end = if has_closing_fence { j } else { end + 1 };
            let content = if i + 1 < content_end {
                lines[i + 1..content_end].join("\n")
            } else {
                String::new()
            };
            let source = lines[i..=end].join("\n");
            if let Some((kind, summary)) = classify_fenced(lang, &content) {
                out.push(Deliverable {
                    kind,
                    summary,
                    content,
                    lang: lang.to_string(),
                    source,
                });
            }
            i = end + 1;
            continue;
        }

        // 2. Prose task-list run (>= 3 checkbox lines)
        if is_task_line(trimmed) {
            let mut j = i;
            while j < lines.len() && is_task_line(lines[j].trim_start()) {
                j += 1;
            }
            if j - i >= 3 {
                let content = lines[i..j].join("\n");
                let done = content
                    .lines()
                    .filter(|l| is_done_task(l.trim_start()))
                    .count();
                let source = content.clone();
                out.push(Deliverable {
                    kind: DeliverableKind::TaskList,
                    summary: format!("{} of {} done", done, j - i),
                    content,
                    lang: String::new(),
                    source,
                });
            }
            i = j;
            continue;
        }

        // 3. Plan heading followed by >= 2 numbered steps
        if is_plan_heading(trimmed) {
            let mut j = i + 1;
            let mut steps = 0usize;
            while j < lines.len() {
                let t = lines[j].trim_start();
                if t.starts_with('#') {
                    break;
                }
                if is_numbered_step(t) {
                    steps += 1;
                }
                j += 1;
            }
            if steps >= 2 {
                let content = lines[i..j].join("\n");
                let source = content.clone();
                out.push(Deliverable {
                    kind: DeliverableKind::Plan,
                    summary: format!("{} steps", steps),
                    content,
                    lang: String::new(),
                    source,
                });
            }
            i = j;
            continue;
        }

        i += 1;
    }
    out
}

/// Remove the exact source blocks that became cards from the conversation markdown.
pub fn strip_deliverables(markdown: &str, deliverables: &[Deliverable]) -> String {
    let mut text = markdown.to_string();
    for d in deliverables {
        if let Some(pos) = text.find(&d.source) {
            text.replace_range(pos..pos + d.source.len(), "");
        }
    }
    text
}

fn classify_fenced(lang: &str, content: &str) -> Option<(DeliverableKind, String)> {
    let lower = lang.to_lowercase();
    let base = lower.split_whitespace().next().unwrap_or("");
    let path_token = lower
        .split_whitespace()
        .skip(1)
        .find(|t| t.contains('/') && t.contains('.'));

    if base == "diff" || base == "patch" || content.contains("diff --git") {
        let files = content
            .lines()
            .filter(|l| l.starts_with("diff --git"))
            .count()
            .max(1);
        let ins = content
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .count();
        let del = content
            .lines()
            .filter(|l| l.starts_with('-') && !l.starts_with("---"))
            .count();
        let unit = if files == 1 { "file" } else { "files" };
        return Some((
            DeliverableKind::Diff,
            format!("{} {} · +{} −{}", files, unit, ins, del),
        ));
    }

    let passed = extract_number_before(content, "passed");
    let failed = extract_number_before(content, "failed");
    let looks_like_tests = content.contains("test result:")
        || content.contains("FAILED")
        || (passed.is_some() && failed.is_some());
    if looks_like_tests {
        let summary = match (passed, failed) {
            (Some(p), Some(f)) => format!("{} passed · {} failed", p, f),
            (Some(p), None) => format!("{} passed", p),
            _ => "failure details".to_string(),
        };
        return Some((DeliverableKind::TestResults, summary));
    }

    if let Some(path) = path_token {
        let count = content.lines().count();
        let unit = if count == 1 { "line" } else { "lines" };
        return Some((
            DeliverableKind::FileExport,
            format!("{} · {} {}", path, count, unit),
        ));
    }
    None
}

/// Digits immediately before `word` (e.g. "141 passed" -> 141).
fn extract_number_before(haystack: &str, word: &str) -> Option<u64> {
    let idx = haystack.find(word)?;
    let digits: String = haystack[..idx]
        .chars()
        .rev()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<u64>().ok()
    }
}

fn is_task_line(t: &str) -> bool {
    (t.starts_with("- [") || t.starts_with("* [") || t.starts_with("+ [")) && t.len() >= 4
}

fn is_done_task(t: &str) -> bool {
    t.starts_with("- [x]")
        || t.starts_with("- [X]")
        || t.starts_with("* [x]")
        || t.starts_with("* [X]")
        || t.starts_with("+ [x]")
        || t.starts_with("+ [X]")
}

fn is_plan_heading(t: &str) -> bool {
    if !t.starts_with('#') {
        return false;
    }
    let rest = t.trim_start_matches('#').trim().to_lowercase();
    rest.starts_with("plan") || rest.starts_with("implementation")
}

fn is_numbered_step(t: &str) -> bool {
    let bytes = t.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    i > 0
        && i < bytes.len()
        && (bytes[i] == b'.' || bytes[i] == b')')
        && i + 1 < bytes.len()
        && bytes[i + 1] == b' '
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_unified_diff_card() {
        let md = "Here is the change:\n\n```diff\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1,3 +1,4 @@\n fn main() {}\n+let x = 1;\n```\n\nDone.";
        let dels = extract_deliverables(md);
        assert_eq!(dels.len(), 1);
        assert_eq!(dels[0].kind, DeliverableKind::Diff);
        assert!(dels[0].summary.contains("1 file"), "{}", dels[0].summary);
        assert!(dels[0].summary.contains("+1"), "{}", dels[0].summary);
        let stripped = strip_deliverables(md, &dels);
        assert!(!stripped.contains("```diff"));
        assert!(stripped.contains("Here is the change:"));
        assert!(stripped.contains("Done."));
    }

    #[test]
    fn test_extract_task_list_card() {
        let md = "## Progress\n- [x] parse args\n- [x] build ui\n- [ ] wire events\n- [ ] polish css\n\nDone.";
        let dels = extract_deliverables(md);
        assert_eq!(dels.len(), 1);
        assert_eq!(dels[0].kind, DeliverableKind::TaskList);
        assert_eq!(dels[0].summary, "2 of 4 done");
        // A short run stays plain prose.
        assert!(extract_deliverables("- [x] one\n- [ ] two").is_empty());
    }

    #[test]
    fn test_extract_test_results_card() {
        let md = "```\nrunning 141 tests\n\ntest result: ok. 141 passed; 0 failed; 0 ignored\n```";
        let dels = extract_deliverables(md);
        assert_eq!(dels.len(), 1);
        assert_eq!(dels[0].kind, DeliverableKind::TestResults);
        assert_eq!(dels[0].summary, "141 passed · 0 failed");
    }

    #[test]
    fn test_extract_file_export_card() {
        let md = "```rust src/lib.rs\npub fn answer() -> u8 { 42 }\n```";
        let dels = extract_deliverables(md);
        assert_eq!(dels.len(), 1);
        assert_eq!(dels[0].kind, DeliverableKind::FileExport);
        assert!(
            dels[0].summary.starts_with("src/lib.rs"),
            "{}",
            dels[0].summary
        );
    }

    #[test]
    fn test_extract_plan_card() {
        let md = "## Implementation Plan\n\n1. Parse the NDJSON stream\n2. Render the timeline\n3. Wire the timers\n\nAll good.";
        let dels = extract_deliverables(md);
        assert_eq!(dels.len(), 1);
        assert_eq!(dels[0].kind, DeliverableKind::Plan);
        assert_eq!(dels[0].summary, "3 steps");
        // Fewer than 2 steps stays prose.
        assert!(extract_deliverables("## Plan\n\n1. Only one step").is_empty());
    }

    #[test]
    fn test_plain_code_block_not_classified() {
        let md = "```rust\nfn main() {}\n```";
        assert!(extract_deliverables(md).is_empty());
    }

    #[test]
    fn test_task_list_inside_code_fence_ignored() {
        let md = "```\n- [x] a\n- [x] b\n- [ ] c\n```";
        assert!(extract_deliverables(md).is_empty());
    }

    #[test]
    fn test_card_label_and_detail_markup() {
        let md = "```diff\n--- a/f\n+++ b/g\n+a\n```";
        let dels = extract_deliverables(md);
        assert_eq!(dels.len(), 1);
        let label = deliverable_card_label(&dels[0]);
        assert!(label.starts_with("Unified diff · 1 file"), "{}", label);
        let detail = render_deliverable_detail(&dels[0]);
        assert!(gtk4::pango::parse_markup(&detail, '\0').is_ok());
        let prose_dels = extract_deliverables("## Plan\n\n1. Step one\n2. Step two");
        let detail_prose = render_deliverable_detail(&prose_dels[0]);
        assert!(gtk4::pango::parse_markup(&detail_prose, '\0').is_ok());
    }

    #[test]
    fn test_unclosed_fence_and_empty_input_no_panic() {
        let dels = extract_deliverables("```diff\n+hello");
        assert_eq!(dels.len(), 1);
        assert!(extract_deliverables("").is_empty());
        assert!(extract_deliverables("```").is_empty());
        assert!(strip_deliverables("plain text", &[]).contains("plain text"));
    }
}
