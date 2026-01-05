//! REPL command registry for the `:` prefix
//!
//! These are vim-style meta commands for controlling the application itself,
//! distinct from `/` commands which affect the conversation/agent.

/// A REPL command definition
#[derive(Debug, Clone)]
pub struct ReplCommand {
    /// Primary name (e.g., "quit")
    pub name: &'static str,
    /// Short aliases (e.g., ["q"])
    pub aliases: &'static [&'static str],
    /// Description shown in popup
    pub description: &'static str,
}

impl ReplCommand {
    /// Check if a query matches this command (name or aliases)
    pub fn matches(&self, query: &str) -> bool {
        self.name.starts_with(query) || self.aliases.iter().any(|a| a.starts_with(query))
    }

    /// Get all matchable names (primary + aliases)
    pub fn all_names(&self) -> impl Iterator<Item = &'static str> {
        std::iter::once(self.name).chain(self.aliases.iter().copied())
    }
}

/// All available REPL commands
pub const REPL_COMMANDS: &[ReplCommand] = &[
    ReplCommand {
        name: "quit",
        aliases: &["q"],
        description: "Exit the application",
    },
    ReplCommand {
        name: "help",
        aliases: &["h"],
        description: "Show keybindings and commands",
    },
    ReplCommand {
        name: "mode",
        aliases: &["m"],
        description: "Cycle session mode (Plan/Act/Auto)",
    },
    ReplCommand {
        name: "agent",
        aliases: &["a"],
        description: "Switch agent backend",
    },
    ReplCommand {
        name: "models",
        aliases: &[],
        description: "List available models",
    },
    ReplCommand {
        name: "config",
        aliases: &["cfg"],
        description: "Show current configuration",
    },
    ReplCommand {
        name: "messages",
        aliases: &["mes"],
        description: "Show message history (notifications)",
    },
    ReplCommand {
        name: "edit",
        aliases: &["e", "view"],
        description: "Open session in $EDITOR",
    },
];

/// Find REPL commands matching a query (fuzzy prefix match)
pub fn find_matching(query: &str) -> Vec<&'static ReplCommand> {
    if query.is_empty() {
        return REPL_COMMANDS.iter().collect();
    }

    let query_lower = query.to_lowercase();
    REPL_COMMANDS
        .iter()
        .filter(|cmd| cmd.matches(&query_lower))
        .collect()
}

/// Look up a command by exact name or alias
pub fn lookup(name: &str) -> Option<&'static ReplCommand> {
    let name_lower = name.to_lowercase();
    REPL_COMMANDS
        .iter()
        .find(|cmd| cmd.name == name_lower || cmd.aliases.contains(&name_lower.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_matching_empty_query() {
        let matches = find_matching("");
        assert_eq!(matches.len(), REPL_COMMANDS.len());
        // Verify expected count (quit, help, mode, agent, models, config, messages, edit)
        assert_eq!(REPL_COMMANDS.len(), 8);
    }

    #[test]
    fn test_find_matching_prefix() {
        let matches = find_matching("q");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "quit");
    }

    #[test]
    fn test_find_matching_alias() {
        let matches = find_matching("h");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "help");
    }

    #[test]
    fn test_find_matching_multiple() {
        // "m" matches "mode", "models", and "messages"
        let matches = find_matching("m");
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_lookup_by_name() {
        let cmd = lookup("quit").unwrap();
        assert_eq!(cmd.name, "quit");
    }

    #[test]
    fn test_lookup_by_alias() {
        let cmd = lookup("q").unwrap();
        assert_eq!(cmd.name, "quit");
    }

    #[test]
    fn test_lookup_case_insensitive() {
        let cmd = lookup("QUIT").unwrap();
        assert_eq!(cmd.name, "quit");
    }

    #[test]
    fn test_lookup_not_found() {
        assert!(lookup("nonexistent").is_none());
    }

    #[test]
    fn test_command_matches() {
        let cmd = &REPL_COMMANDS[0]; // quit
        assert!(cmd.matches("q"));
        assert!(cmd.matches("qu"));
        assert!(cmd.matches("quit"));
        assert!(!cmd.matches("x"));
    }
}
