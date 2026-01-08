//! REPL command registry
//!
//! Handles vim-style commands like `:quit`, `:help`, `:mode`.

use crate::tui::popup::PopupProvider;
use crate::tui::state::types::{PopupItem, PopupKind};
use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Matcher, Utf32String};

/// Registry for REPL commands
#[derive(Clone, Default)]
pub struct ReplCommandRegistry {
    commands: Vec<ReplCommandEntry>,
}

/// A REPL command entry
#[derive(Clone, Debug)]
pub struct ReplCommandEntry {
    pub name: String,
    pub description: String,
}

impl ReplCommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a command to the registry
    pub fn add_command(&mut self, name: &str, description: &str) {
        self.commands.push(ReplCommandEntry {
            name: name.to_string(),
            description: description.to_string(),
        });
    }

    /// Set all commands at once (replaces existing)
    pub fn set_commands(&mut self, commands: Vec<(String, String)>) {
        self.commands = commands
            .into_iter()
            .map(|(name, description)| ReplCommandEntry { name, description })
            .collect();
    }

    /// Initialize from the static REPL_COMMANDS registry
    pub fn init_from_static(&mut self) {
        use crate::tui::repl_commands::REPL_COMMANDS;
        self.commands = REPL_COMMANDS
            .iter()
            .map(|cmd| ReplCommandEntry {
                name: cmd.name.to_string(),
                description: cmd.description.to_string(),
            })
            .collect();
    }

    /// Get command count
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }
}

impl PopupProvider for ReplCommandRegistry {
    fn provide(&self, _kind: PopupKind, query: &str) -> Vec<PopupItem> {
        let query = query.split_whitespace().next().unwrap_or("").trim();
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut pattern = nucleo::pattern::MultiPattern::new(1);
        pattern.reparse(0, query, CaseMatching::Ignore, Normalization::Smart, false);

        let mut out = Vec::new();

        for cmd in &self.commands {
            let match_col = Utf32String::from(cmd.name.as_str());
            let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
            if let Some(score) = score {
                out.push(
                    PopupItem::repl(&cmd.name)
                        .desc(&cmd.description)
                        .with_score(score.min(i32::MAX as u32) as i32),
                );
            }
        }

        // Sort by score descending, truncate
        out.sort_by_key(|item| std::cmp::Reverse(item.score()));
        out.truncate(20);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repl_registry_matches_commands() {
        let mut registry = ReplCommandRegistry::new();
        registry.add_command("quit", "Exit the application");
        registry.add_command("help", "Show help");
        registry.add_command("mode", "Change mode");

        let items = registry.provide(PopupKind::ReplCommand, "quit");
        assert_eq!(items.len(), 1);
        assert!(items[0].is_repl_command());
        assert_eq!(items[0].title(), ":quit");
    }

    #[test]
    fn test_repl_registry_fuzzy_match() {
        let mut registry = ReplCommandRegistry::new();
        registry.add_command("messages", "Show notification history");

        // Fuzzy match
        let items = registry.provide(PopupKind::ReplCommand, "msg");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_repl_registry_set_bulk() {
        let mut registry = ReplCommandRegistry::new();
        registry.set_commands(vec![
            ("a".into(), "Command A".into()),
            ("b".into(), "Command B".into()),
            ("c".into(), "Command C".into()),
        ]);
        assert_eq!(registry.command_count(), 3);
    }

    #[test]
    fn test_repl_registry_empty_query_matches_all() {
        let mut registry = ReplCommandRegistry::new();
        registry.add_command("quit", "Exit");
        registry.add_command("help", "Help");

        let items = registry.provide(PopupKind::ReplCommand, "");
        assert_eq!(items.len(), 2);
    }
}
