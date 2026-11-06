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
    for cmd in SLASH_COMMANDS {
        if let Some(stripped) = trimmed.strip_prefix(cmd.name) {
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
}
