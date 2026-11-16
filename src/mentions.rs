//! `@` / `#` mention and `/` command autocomplete support.
//!
//! The popover needs three things: a workspace file index, a set of built-in
//! context entries, and a trigger extractor that maps the text before the caret
//! onto the token being typed.

use std::path::Path;

/// Cap on indexed files. The index is for autocomplete, not for completeness;
/// a huge repo must not stall the composer.
pub const MAX_INDEXED_FILES: usize = 2500;

/// Directories never worth indexing even when not gitignored.
const SKIP_DIRS: [&str; 8] = [
    ".git", "target", "node_modules", ".venv", "dist", "build", ".cache", "__pycache__",
];

/// What kind of token the user is typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    /// `@` — files and context entries.
    At,
    /// `#` — files and context entries.
    Hash,
    /// `/` — slash commands, only at the very start of the input.
    Slash,
}

impl TriggerKind {
}

/// A live token found before the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTrigger {
    pub kind: TriggerKind,
    /// Byte offset of the trigger character in the input.
    pub start: usize,
    /// Text typed after the trigger character.
    pub query: String,
}

/// One autocomplete entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionItem {
    /// Text inserted into the composer, e.g. `@src/main.rs`.
    pub insert: String,
    /// Primary line in the popover.
    pub label: String,
    /// Secondary line, usually a path or description.
    pub detail: String,
    /// True for built-in context entries rather than workspace files.
    pub is_context: bool,
}

impl MentionItem {
    fn context(insert: &str, label: &str, detail: &str) -> Self {
        Self {
            insert: insert.to_string(),
            label: label.to_string(),
            detail: detail.to_string(),
            is_context: true,
        }
    }
}

/// Built-in context entries offered alongside workspace files.
pub fn context_mentions() -> Vec<MentionItem> {
    vec![
        MentionItem::context(
            "@screen",
            "Screen",
            "Capture the live screen with this prompt",
        ),
        MentionItem::context(
            "@markup",
            "Markup",
            "Capture the screen and annotate it before sending",
        ),
        MentionItem::context("@region", "Region", "Capture a region you select"),
        MentionItem::context("@window", "Active window", "Capture the focused window"),
        MentionItem::context("@clipboard", "Clipboard", "Attach clipboard contents"),
    ]
}

/// Detects the `@` / `#` / `/` token immediately before the caret.
///
/// Returns `None` unless the token is still being typed: a `@` followed by a
/// space ends the mention and closes the popover.
pub fn extract_active_trigger(text_before_cursor: &str) -> Option<ActiveTrigger> {
    // Slash commands only apply to a line that starts with `/`.
    if let Some(rest) = text_before_cursor.strip_prefix('/') {
        if !rest.contains(char::is_whitespace) && !rest.contains('/') {
            return Some(ActiveTrigger {
                kind: TriggerKind::Slash,
                start: 0,
                query: rest.to_string(),
            });
        }
    }

    // Scan back for the nearest trigger character on the current line.
    let line_start = text_before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = &text_before_cursor[line_start..];

    let mut found: Option<(usize, char)> = None;
    for (idx, ch) in line.char_indices() {
        match ch {
            '@' | '#' => {
                // A trigger must start a word: `a@b` is an email, not a mention.
                let preceded_by_word = idx > 0
                    && line[..idx]
                        .chars()
                        .next_back()
                        .is_some_and(|p| p.is_alphanumeric() || p == '_');
                found = if preceded_by_word { None } else { Some((idx, ch)) };
            }
            c if c.is_whitespace() => found = None,
            _ => {}
        }
    }

    let (idx, symbol) = found?;
    let query = &line[idx + symbol.len_utf8()..];
    if query.chars().any(|c| c.is_whitespace()) {
        return None;
    }

    Some(ActiveTrigger {
        kind: if symbol == '@' {
            TriggerKind::At
        } else {
            TriggerKind::Hash
        },
        start: line_start + idx,
        query: query.to_string(),
    })
}

/// Index of workspace files, built once and searched on each keystroke.
#[derive(Debug, Default)]
pub struct WorkspaceIndex {
    /// Paths relative to the walk root, using `/` separators.
    files: Vec<String>,
}

impl WorkspaceIndex {
    /// Walks `root`, honouring `.gitignore` and skipping junk directories.
    pub fn build(root: &Path) -> Self {
        if !root.is_dir() {
            return Self::default();
        }

        let mut files = Vec::new();
        let walker = ignore::WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .max_depth(Some(8))
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !(entry.file_type().is_some_and(|t| t.is_dir())
                    && SKIP_DIRS.contains(&name.as_ref()))
            })
            .build();

        for entry in walker.flatten() {
            if files.len() >= MAX_INDEXED_FILES {
                break;
            }
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            if let Ok(rel) = entry.path().strip_prefix(root) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }

        files.sort();
        Self { files }
    }

    /// Number of indexed files.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Ranks files matching `query`, best first.
    ///
    /// Basename matches beat directory matches, exact prefixes beat scattered
    /// subsequences, and shorter paths win ties.
    pub fn search(&self, query: &str, limit: usize) -> Vec<MentionItem> {
        let query = query.trim().to_lowercase();
        let mut scored: Vec<(i32, &String)> = Vec::new();

        for path in &self.files {
            let lower = path.to_lowercase();
            let base = lower.rsplit('/').next().unwrap_or(&lower);

            let score = if query.is_empty() {
                // Empty query: shallow files first, so the list is useful.
                -((path.matches('/').count() as i32) * 10) - path.len() as i32
            } else if base.starts_with(&query) {
                1000 - lower.len() as i32
            } else if base.contains(&query) {
                750 - lower.len() as i32
            } else if lower.starts_with(&query) {
                600 - lower.len() as i32
            } else if lower.contains(&query) {
                400 - lower.len() as i32
            } else if is_subsequence(&query, &lower) {
                200 - lower.len() as i32
            } else {
                continue;
            };

            scored.push((score, path));
        }

        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));

        scored
            .into_iter()
            .take(limit)
            .map(|(_, path)| MentionItem {
                insert: format!("@{path}"),
                label: path.rsplit('/').next().unwrap_or(path).to_string(),
                detail: path.clone(),
                is_context: false,
            })
            .collect()
    }

    /// Context entries first, then ranked files, capped at `limit`.
    pub fn suggest(&self, query: &str, limit: usize) -> Vec<MentionItem> {
        let query_lower = query.trim().to_lowercase();
        let mut items: Vec<MentionItem> = context_mentions()
            .into_iter()
            .filter(|item| {
                query_lower.is_empty()
                    || item.label.to_lowercase().contains(&query_lower)
                    || item.insert.to_lowercase().contains(&query_lower)
            })
            .collect();

        let remaining = limit.saturating_sub(items.len());
        if remaining > 0 {
            items.extend(self.search(query, remaining));
        }
        items
    }
}

/// True when all of `needle`'s characters appear in `haystack` in order.
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|n| chars.any(|h| h == n))
}

/// A slash command offered by the composer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    /// True when the command came from the harness rather than being built in.
    pub from_agent: bool,
}

/// Built-in commands the client handles itself.
///
/// Anything the harness owns (compaction, goals, …) is not listed here; it
/// arrives through [`merge_commands`] from `available_commands_update`.
pub fn builtin_commands() -> Vec<SlashCommand> {
    [
        ("new", "Start a new session"),
        ("clear", "Clear the conversation"),
        ("screen", "Capture the screen"),
        ("markup", "Annotate the screen"),
        ("region", "Capture a region"),
        ("window", "Capture the active window"),
        ("help", "Show help"),
    ]
    .into_iter()
    .map(|(name, description)| SlashCommand {
        name: name.to_string(),
        description: description.to_string(),
        from_agent: false,
    })
    .collect()
}

/// Merges built-in commands with the harness's own, built-ins first.
///
/// Names reported by the harness win on collision, since its behaviour is the
/// authoritative one, but the built-in order is preserved.
pub fn merge_commands(agent_commands: &[(String, String)]) -> Vec<SlashCommand> {
    let mut out = builtin_commands();
    for (name, description) in agent_commands {
        let existing = out.iter_mut().find(|c| &c.name == name);
        match existing {
            Some(cmd) => {
                cmd.description = description.clone();
                cmd.from_agent = true;
            }
            None => out.push(SlashCommand {
                name: name.clone(),
                description: description.clone(),
                from_agent: true,
            }),
        }
    }
    out
}

/// Filters commands by a prefix query.
pub fn filter_commands(commands: &[SlashCommand], query: &str) -> Vec<SlashCommand> {
    let query = query.trim().trim_start_matches('/').to_lowercase();
    commands
        .iter()
        .filter(|c| query.is_empty() || c.name.to_lowercase().starts_with(&query))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_at_trigger() {
        let trigger = extract_active_trigger("look at @src/ma").unwrap();
        assert_eq!(trigger.kind, TriggerKind::At);
        assert_eq!(trigger.query, "src/ma");
        assert_eq!(trigger.start, 8);
    }

    #[test]
    fn test_extract_hash_trigger() {
        let trigger = extract_active_trigger("check #notes").unwrap();
        assert_eq!(trigger.kind, TriggerKind::Hash);
        assert_eq!(trigger.query, "notes");
    }

    #[test]
    fn test_extract_slash_only_at_line_start() {
        let trigger = extract_active_trigger("/comp").unwrap();
        assert_eq!(trigger.kind, TriggerKind::Slash);
        assert_eq!(trigger.query, "comp");

        // Mid-line slash is a path separator, not a command.
        assert!(extract_active_trigger("see src/comp").is_none());
    }

    #[test]
    fn test_slash_closed_by_space() {
        assert!(extract_active_trigger("/comp now").is_none());
    }

    #[test]
    fn test_at_closed_by_space() {
        assert!(extract_active_trigger("@screen please").is_none());
    }

    #[test]
    fn test_email_is_not_a_mention() {
        assert!(extract_active_trigger("mail me at a@b").is_none());
    }

    #[test]
    fn test_trigger_uses_only_current_line() {
        let trigger = extract_active_trigger("first line\n@main").unwrap();
        assert_eq!(trigger.query, "main");
        assert_eq!(trigger.start, "first line\n".len());
    }

    #[test]
    fn test_empty_query_after_trigger_matches() {
        let trigger = extract_active_trigger("@").unwrap();
        assert_eq!(trigger.query, "");
        assert_eq!(trigger.kind, TriggerKind::At);
    }

    #[test]
    fn test_no_trigger_in_plain_text() {
        assert!(extract_active_trigger("just some prose").is_none());
    }

    #[test]
    fn test_context_mentions_include_the_live_screen_entries() {
        let items = context_mentions();
        let inserts: Vec<&str> = items.iter().map(|i| i.insert.as_str()).collect();
        assert!(inserts.contains(&"@screen"));
        assert!(inserts.contains(&"@markup"));
        assert!(inserts.contains(&"@region"));
        assert!(inserts.contains(&"@window"));
        assert!(inserts.contains(&"@clipboard"));
        assert!(items.iter().all(|i| i.is_context));
    }

    #[test]
    fn test_workspace_index_missing_root_is_empty() {
        let index = WorkspaceIndex::build(Path::new("/nonexistent/definitely/not/here"));
        assert!(index.is_empty());
        assert!(index.search("anything", 10).is_empty());
    }

    #[test]
    fn test_workspace_index_finds_files_and_skips_target() {
        let root = std::env::temp_dir().join("pi-overlay-mentions-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(root.join("README.md"), "# hi").unwrap();
        std::fs::write(root.join("target/junk.rs"), "ignored").unwrap();

        let index = WorkspaceIndex::build(&root);
        assert!(index.len() >= 2, "expected indexed files, got {}", index.len());

        let results = index.search("main", 10);
        assert!(
            results.iter().any(|r| r.detail == "src/main.rs"),
            "expected src/main.rs in {results:?}"
        );
        assert!(
            !results.iter().any(|r| r.detail.contains("target/")),
            "target/ must be skipped"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_search_prefers_basename_match() {
        let root = std::env::temp_dir().join("pi-overlay-mentions-rank");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("deep/nested/path")).unwrap();
        std::fs::write(root.join("deep/nested/path/config.rs"), "").unwrap();
        std::fs::write(root.join("config.rs"), "").unwrap();

        let index = WorkspaceIndex::build(&root);
        let results = index.search("config.rs", 5);
        assert_eq!(results[0].detail, "config.rs", "shallow match must rank first");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_suggest_puts_context_before_files() {
        let index = WorkspaceIndex::default();
        let items = index.suggest("", 10);
        assert!(!items.is_empty());
        assert!(items[0].is_context, "context entries come first");
    }

    #[test]
    fn test_suggest_filters_context_by_query() {
        let index = WorkspaceIndex::default();
        let items = index.suggest("mark", 10);
        assert!(items.iter().any(|i| i.insert == "@markup"));
        assert!(!items.iter().any(|i| i.insert == "@clipboard"));
    }

    #[test]
    fn test_is_subsequence() {
        assert!(is_subsequence("mn", "main.rs"));
        assert!(is_subsequence("", "anything"));
        assert!(!is_subsequence("zz", "main.rs"));
    }

    #[test]
    fn test_builtin_commands_are_unique() {
        let commands = builtin_commands();
        let mut names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        names.sort();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate built-in command name");
    }

    #[test]
    fn test_merge_commands_marks_agent_source() {
        let agent = vec![
            ("compact".to_string(), "Shrink the context window".to_string()),
            ("goal".to_string(), "Set a session goal".to_string()),
        ];
        let merged = merge_commands(&agent);

        let compact = merged.iter().find(|c| c.name == "compact").unwrap();
        assert!(compact.from_agent, "agent override must be marked");
        assert_eq!(compact.description, "Shrink the context window");

        let goal = merged.iter().find(|c| c.name == "goal").unwrap();
        assert!(goal.from_agent);

        // Built-ins that the agent did not mention stay client-side.
        let help = merged.iter().find(|c| c.name == "help").unwrap();
        assert!(!help.from_agent);
    }

    #[test]
    fn test_filter_commands_by_prefix() {
        let commands = builtin_commands();
        let filtered = filter_commands(&commands, "sc");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "screen");

        // A leading slash is tolerated, as typed.
        assert_eq!(filter_commands(&commands, "/new")[0].name, "new");
        assert_eq!(filter_commands(&commands, "").len(), commands.len());
    }
}
