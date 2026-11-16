/// Commands the client handles itself, without a round trip to the harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalCommand {
    NewSession,
    ClearConversation,
    CaptureScreen,
    OpenMarkup,
    CaptureRegion,
    CaptureWindow,
    ShowHelp,
}

impl LocalCommand {
    #[cfg(test)]
    pub fn label(self) -> &'static str {
        match self {
            LocalCommand::NewSession => "New session",
            LocalCommand::ClearConversation => "Clear the conversation",
            LocalCommand::CaptureScreen => "Capture the screen",
            LocalCommand::OpenMarkup => "Annotate the screen",
            LocalCommand::CaptureRegion => "Capture a region",
            LocalCommand::CaptureWindow => "Capture the active window",
            LocalCommand::ShowHelp => "Show help",
        }
    }
}

/// Maps a `/name` to the client-side action it triggers.
///
/// Returns `None` for unknown names and for prompt-template commands, which are
/// expanded by [`expand_slash_command`] and sent to the harness instead.
pub fn parse_local_command(name: &str) -> Option<LocalCommand> {
    let name = name.trim().trim_start_matches('/');
    match name {
        "new" => Some(LocalCommand::NewSession),
        "clear" => Some(LocalCommand::ClearConversation),
        "screen" => Some(LocalCommand::CaptureScreen),
        "markup" => Some(LocalCommand::OpenMarkup),
        "region" => Some(LocalCommand::CaptureRegion),
        "window" => Some(LocalCommand::CaptureWindow),
        "help" => Some(LocalCommand::ShowHelp),
        _ => None,
    }
}

/// Built-in Slash Commands matching Raycast & Open-WebUI standards
pub struct SlashCommand {
    pub name: &'static str,
    pub template: &'static str,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "/explain",
        template: "Please explain the following clearly and concisely:\n\n",
    },
    SlashCommand {
        name: "/refactor",
        template: "Refactor the following code following clean code standards:\n\n",
    },
    SlashCommand {
        name: "/test",
        template: "Generate comprehensive unit tests covering edge cases for:\n\n",
    },
    SlashCommand {
        name: "/fix",
        template: "Find bugs or edge cases in this code and provide minimal fixes:\n\n",
    },
    SlashCommand {
        name: "/review",
        template: "Perform a thorough code review checking security and design:\n\n",
    },
    SlashCommand {
        name: "/doc",
        template: "Write clean, accurate documentation for:\n\n",
    },
];

pub fn expand_slash_command(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    // A client-handled command must never be rewritten into prompt text.
    let name = trimmed[1..]
        .split_whitespace()
        .next()
        .unwrap_or_default();
    if parse_local_command(name).is_some() {
        return None;
    }
    for cmd in SLASH_COMMANDS {
        if let Some(stripped) = trimmed.strip_prefix(cmd.name) {
            // Require a boundary so `/testing` does not match `/test`.
            if !stripped.is_empty() && !stripped.starts_with(char::is_whitespace) {
                continue;
            }
            let remainder = stripped.trim_start();
            return Some(format!("{}{}", cmd.template, remainder));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_slash_command() {
        assert_eq!(expand_slash_command("regular text"), None);
        assert_eq!(
            expand_slash_command("/explain how rust lifetimes work"),
            Some("Please explain the following clearly and concisely:\n\nhow rust lifetimes work".to_string())
        );
        assert_eq!(
            expand_slash_command("/fix null pointer bug"),
            Some("Find bugs or edge cases in this code and provide minimal fixes:\n\nnull pointer bug".to_string())
        );
    }

    #[test]
    fn test_expand_requires_word_boundary() {
        // `/testing` must not be treated as `/test`.
        assert_eq!(expand_slash_command("/testing something"), None);
    }

    #[test]
    fn test_local_commands_are_not_expanded_as_prompts() {
        for name in ["new", "clear", "screen", "markup", "region", "window", "help"] {
            let input = format!("/{name} extra");
            assert_eq!(
                expand_slash_command(&input),
                None,
                "/{name} is client-handled and must not be rewritten"
            );
        }
    }

    #[test]
    fn test_parse_local_command_recognises_each_name() {
        assert_eq!(parse_local_command("/new"), Some(LocalCommand::NewSession));
        assert_eq!(parse_local_command("clear"), Some(LocalCommand::ClearConversation));
        assert_eq!(parse_local_command("/screen"), Some(LocalCommand::CaptureScreen));
        assert_eq!(parse_local_command("/markup"), Some(LocalCommand::OpenMarkup));
        assert_eq!(parse_local_command("/region"), Some(LocalCommand::CaptureRegion));
        assert_eq!(parse_local_command("/window"), Some(LocalCommand::CaptureWindow));
        assert_eq!(parse_local_command("/help"), Some(LocalCommand::ShowHelp));
    }

    #[test]
    fn test_parse_local_command_rejects_unknown_and_prompt_templates() {
        assert_eq!(parse_local_command("/nope"), None);
        assert_eq!(parse_local_command("/explain"), None);
        assert_eq!(parse_local_command(""), None);
    }

    #[test]
    fn test_local_command_labels_are_present() {
        for cmd in [
            LocalCommand::NewSession,
            LocalCommand::ClearConversation,
            LocalCommand::CaptureScreen,
            LocalCommand::OpenMarkup,
            LocalCommand::CaptureRegion,
            LocalCommand::CaptureWindow,
            LocalCommand::ShowHelp,
        ] {
            assert!(!cmd.label().is_empty());
        }
    }

    #[test]
    fn test_local_commands_align_with_mention_builtins() {
        // Every command the `/` popover calls built-in must actually be handled
        // by the client; anything else has to come from the harness.
        let builtins = crate::mentions::builtin_commands();
        assert!(!builtins.is_empty());
        for cmd in &builtins {
            assert!(!cmd.from_agent, "{} must be client-side", cmd.name);
            assert!(
                parse_local_command(&cmd.name).is_some(),
                "/{} is offered but not handled",
                cmd.name
            );
        }
    }

    #[test]
    fn test_harness_owned_commands_are_not_client_builtins() {
        // Compaction is performed by the agent, so the client must neither
        // claim it as built-in nor rewrite it into prompt text.
        let commands = crate::mentions::builtin_commands();
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&"compact"));
        assert_eq!(parse_local_command("/compact"), None);
        assert_eq!(expand_slash_command("/compact"), None);
    }
}
