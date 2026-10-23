/// Built-in Slash Commands matching Raycast & Open-WebUI standards
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub template: &'static str,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        name: "/explain",
        description: "Explain this code or concept step by step",
        template: "Please explain the following clearly and concisely:\n\n",
    },
    SlashCommand {
        name: "/refactor",
        description: "Refactor code for performance, readability & safety",
        template: "Refactor the following code following clean code standards:\n\n",
    },
    SlashCommand {
        name: "/test",
        description: "Write comprehensive unit tests with edge cases",
        template: "Generate comprehensive unit tests covering edge cases for:\n\n",
    },
    SlashCommand {
        name: "/fix",
        description: "Analyze, find bugs and produce minimal diff fixes",
        template: "Find bugs or edge cases in this code and provide minimal fixes:\n\n",
    },
    SlashCommand {
        name: "/review",
        description: "Review git changes, commits, or code architecture",
        template: "Perform a thorough code review checking security and design:\n\n",
    },
    SlashCommand {
        name: "/doc",
        description: "Generate documentation, rustdoc or markdown specs",
        template: "Write clean, accurate documentation for:\n\n",
    },
];

pub fn match_commands(query: &str) -> Vec<&'static SlashCommand> {
    let q = query.trim_start().to_lowercase();
    if q.is_empty() {
        return SLASH_COMMANDS.iter().collect();
    }
    SLASH_COMMANDS
        .iter()
        .filter(|cmd| cmd.name.contains(&q) || cmd.description.to_lowercase().contains(&q))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_commands_matching() {
        let all = match_commands("");
        assert_eq!(all.len(), SLASH_COMMANDS.len());

        let explain = match_commands("/exp");
        assert_eq!(explain.len(), 1);
        assert_eq!(explain[0].name, "/explain");

        let tests = match_commands("test");
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].name, "/test");
    }
}
