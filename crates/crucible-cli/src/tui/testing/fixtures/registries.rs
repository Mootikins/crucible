//! Registry fixtures for popup/completion testing
//!
//! Pre-built command and agent lists for testing popup behavior.

use crate::tui::state::PopupItem;

/// Helper to create a command item
pub fn command(name: impl Into<String>, description: impl Into<String>) -> PopupItem {
    PopupItem::cmd(name.into()).desc(description)
}

/// Helper to create an agent item
pub fn agent(name: impl Into<String>, description: impl Into<String>) -> PopupItem {
    PopupItem::agent(name.into()).desc(description)
}

/// Helper to create a note item
pub fn note(name: impl Into<String>) -> PopupItem {
    PopupItem::note(name.into())
}

/// Helper to create a file item
pub fn file(path: impl Into<String>) -> PopupItem {
    PopupItem::file(path.into())
}

/// Helper to create a skill item
pub fn skill(name: impl Into<String>, description: impl Into<String>) -> PopupItem {
    PopupItem::skill(name.into()).desc(description)
}

/// Helper to create a REPL command item
pub fn repl(name: impl Into<String>, description: impl Into<String>) -> PopupItem {
    PopupItem::repl(name.into()).desc(description)
}

/// Standard slash commands
pub fn standard_commands() -> Vec<PopupItem> {
    vec![
        command("search", "Search notes semantically"),
        command("new", "Start new session"),
        command("clear", "Clear conversation context"),
        command("help", "Show available commands"),
        command("mode", "Switch session mode"),
        command("agents", "List available agents"),
    ]
}

/// Minimal command set for focused tests
pub fn minimal_commands() -> Vec<PopupItem> {
    vec![
        command("search", "Search notes"),
        command("help", "Show help"),
    ]
}

/// Test agents
pub fn test_agents() -> Vec<PopupItem> {
    vec![
        agent("researcher", "Deep research and analysis"),
        agent("coder", "Code generation and review"),
        agent("writer", "Writing and editing"),
    ]
}

/// Large list for scroll/filter testing
pub fn many_commands() -> Vec<PopupItem> {
    (0..20)
        .map(|i| {
            command(
                format!("command{i}"),
                format!("Description for command {i}"),
            )
        })
        .collect()
}

/// Test files for @ popup
pub fn test_files() -> Vec<PopupItem> {
    vec![
        file("src/main.rs"),
        file("src/lib.rs"),
        file("Cargo.toml"),
        file("README.md"),
    ]
}

/// Test notes for @ popup
pub fn test_notes() -> Vec<PopupItem> {
    vec![
        note("Projects/Crucible"),
        note("Ideas/Backlog"),
        note("Meta/Roadmap"),
    ]
}

/// Test skills (like commands but invoked differently)
pub fn test_skills() -> Vec<PopupItem> {
    vec![
        skill("commit", "Create git commit"),
        skill("prime", "Load project context"),
        skill("ultra-think", "Deep analysis mode"),
    ]
}

/// Test REPL commands (vim-style system commands)
pub fn test_repl_commands() -> Vec<PopupItem> {
    vec![
        repl("quit", "Exit the application"),
        repl("help", "Show keybindings and commands"),
        repl("mode", "Cycle session mode"),
        repl("agent", "Switch agent backend"),
        repl("models", "List available models"),
    ]
}

/// Mixed items for AgentOrFile popup testing
pub fn mixed_agent_file_items() -> Vec<PopupItem> {
    let mut items = vec![];
    items.extend(test_agents());
    items.extend(test_files());
    items.extend(test_notes());
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_commands_not_empty() {
        assert!(!standard_commands().is_empty());
    }

    #[test]
    fn commands_have_subtitles() {
        for cmd in standard_commands() {
            assert!(!cmd.subtitle().is_empty());
        }
    }

    #[test]
    fn many_commands_has_expected_count() {
        assert_eq!(many_commands().len(), 20);
    }

    #[test]
    fn command_tokens_have_slash() {
        for cmd in minimal_commands() {
            assert!(cmd.token().starts_with('/'));
        }
    }

    #[test]
    fn agent_tokens_have_at() {
        for agent in test_agents() {
            assert!(agent.token().starts_with('@'));
        }
    }

    #[test]
    fn file_tokens_are_paths() {
        // File tokens are just the path (no prefix)
        for f in test_files() {
            assert!(!f.token().is_empty());
            // Should be a file path
            assert!(f.token().contains('.') || f.token().contains('/'));
        }
    }

    #[test]
    fn note_tokens_are_paths() {
        // Note tokens are just the path (no prefix)
        for n in test_notes() {
            assert!(!n.token().is_empty());
        }
    }

    #[test]
    fn skill_tokens_have_skill_prefix() {
        // Skills use "skill:" prefix
        for s in test_skills() {
            assert!(
                s.token().starts_with("skill:"),
                "Skill token should start with 'skill:', got: {}",
                s.token()
            );
        }
    }

    #[test]
    fn repl_tokens_have_colon_prefix() {
        // REPL commands use ":" prefix
        for r in test_repl_commands() {
            assert!(
                r.token().starts_with(':'),
                "REPL token should start with ':', got: {}",
                r.token()
            );
        }
    }

    #[test]
    fn mixed_items_contains_all_types() {
        let items = mixed_agent_file_items();
        assert!(items.iter().any(|i| matches!(i, PopupItem::Agent { .. })));
        assert!(items.iter().any(|i| matches!(i, PopupItem::File { .. })));
        assert!(items.iter().any(|i| matches!(i, PopupItem::Note { .. })));
    }
}
