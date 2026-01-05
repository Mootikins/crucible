//! Command and skill registry
//!
//! Handles slash commands (`/search`, `/clear`) and skills (`skill:commit`).

use crate::tui::popup::PopupProvider;
use crate::tui::state::{PopupItem, PopupKind};
use crucible_core::traits::chat::CommandDescriptor;
use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config, Matcher, Utf32String};

/// Registry for slash commands and skills
#[derive(Clone, Default)]
pub struct CommandRegistry {
    commands: Vec<CommandDescriptor>,
    skills: Vec<SkillEntry>,
}

/// A skill entry with metadata
#[derive(Clone, Debug)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub scope: String,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a command to the registry
    pub fn add_command(&mut self, cmd: CommandDescriptor) {
        self.commands.push(cmd);
    }

    /// Set all commands at once (replaces existing)
    pub fn set_commands(&mut self, commands: Vec<CommandDescriptor>) {
        self.commands = commands;
    }

    /// Add a skill to the registry
    pub fn add_skill(&mut self, name: &str, description: &str, scope: &str) {
        self.skills.push(SkillEntry {
            name: name.to_string(),
            description: description.to_string(),
            scope: scope.to_string(),
        });
    }

    /// Set all skills at once (replaces existing)
    pub fn set_skills(&mut self, skills: Vec<(String, String, String)>) {
        self.skills = skills
            .into_iter()
            .map(|(name, description, scope)| SkillEntry {
                name,
                description,
                scope,
            })
            .collect();
    }

    /// Get command count
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// Get skill count
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }
}

impl PopupProvider for CommandRegistry {
    fn provide(&self, _kind: PopupKind, query: &str) -> Vec<PopupItem> {
        let query = query.split_whitespace().next().unwrap_or("").trim();
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut pattern = nucleo::pattern::MultiPattern::new(1);
        pattern.reparse(0, query, CaseMatching::Ignore, Normalization::Smart, false);

        let mut out = Vec::new();

        // Match commands
        for cmd in &self.commands {
            let description = cmd
                .input_hint
                .as_ref()
                .map(|h| format!("{} — {}", cmd.description, h))
                .unwrap_or_else(|| cmd.description.clone());
            let match_col = Utf32String::from(cmd.name.as_str());
            let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
            if let Some(score) = score {
                let mut item = PopupItem::cmd(&cmd.name)
                    .desc(&description)
                    .with_score(score.min(i32::MAX as u32) as i32);
                if let Some(hint) = &cmd.input_hint {
                    item = item.hint(hint);
                }
                out.push(item);
            }
        }

        // Match skills
        for skill in &self.skills {
            let full_desc = format!("{} ({})", skill.description, skill.scope);
            let match_col = Utf32String::from(skill.name.as_str());
            let score = pattern.score(std::slice::from_ref(&match_col), &mut matcher);
            if let Some(score) = score {
                out.push(
                    PopupItem::skill(&skill.name)
                        .desc(&full_desc)
                        .with_scope(&skill.scope)
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
    fn test_command_registry_matches_commands() {
        let mut registry = CommandRegistry::new();
        registry.add_command(CommandDescriptor {
            name: "search".into(),
            description: "Search notes".into(),
            input_hint: Some("query".into()),
            secondary_options: vec![],
        });
        registry.add_command(CommandDescriptor {
            name: "exit".into(),
            description: "Exit".into(),
            input_hint: None,
            secondary_options: vec![],
        });

        let items = registry.provide(PopupKind::Command, "sea");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title(), "/search");
    }

    #[test]
    fn test_command_registry_matches_skills() {
        let mut registry = CommandRegistry::new();
        registry.add_skill("commit", "Create git commits", "user");
        registry.add_skill("review", "Code review", "user");

        let items = registry.provide(PopupKind::Command, "com");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title(), "skill:commit");
    }

    #[test]
    fn test_command_registry_fuzzy_match() {
        let mut registry = CommandRegistry::new();
        registry.add_command(CommandDescriptor {
            name: "search".into(),
            description: "Search".into(),
            input_hint: None,
            secondary_options: vec![],
        });

        // Fuzzy subsequence match
        let items = registry.provide(PopupKind::Command, "srch");
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn test_command_registry_set_bulk() {
        let mut registry = CommandRegistry::new();
        registry.set_commands(vec![
            CommandDescriptor {
                name: "a".into(),
                description: "A".into(),
                input_hint: None,
                secondary_options: vec![],
            },
            CommandDescriptor {
                name: "b".into(),
                description: "B".into(),
                input_hint: None,
                secondary_options: vec![],
            },
        ]);
        assert_eq!(registry.command_count(), 2);

        registry.set_skills(vec![
            ("x".into(), "X".into(), "user".into()),
            ("y".into(), "Y".into(), "project".into()),
        ]);
        assert_eq!(registry.skill_count(), 2);
    }
}
