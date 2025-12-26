//! Registry fixtures for popup/completion testing
//!
//! Pre-built command and agent lists for testing popup behavior.

use crate::tui::state::{PopupItem, PopupItemKind};

/// Helper to create a command item
pub fn command(name: impl Into<String>, description: impl Into<String>) -> PopupItem {
    let name = name.into();
    PopupItem {
        kind: PopupItemKind::Command,
        title: name.clone(),
        subtitle: description.into(),
        token: format!("/{name}"),
        score: 0,
        available: true,
    }
}

/// Helper to create an agent item
pub fn agent(name: impl Into<String>, description: impl Into<String>) -> PopupItem {
    let name = name.into();
    PopupItem {
        kind: PopupItemKind::Agent,
        title: name.clone(),
        subtitle: description.into(),
        token: format!("@{name}"),
        score: 0,
        available: true,
    }
}

/// Helper to create a note item
pub fn note(name: impl Into<String>) -> PopupItem {
    let name = name.into();
    PopupItem {
        kind: PopupItemKind::Note,
        title: name.clone(),
        subtitle: String::new(),
        token: format!("@{name}"),
        score: 0,
        available: true,
    }
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
    vec![command("search", "Search notes"), command("help", "Show help")]
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
        .map(|i| command(format!("command{i}"), format!("Description for command {i}")))
        .collect()
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
            assert!(!cmd.subtitle.is_empty());
        }
    }

    #[test]
    fn many_commands_has_expected_count() {
        assert_eq!(many_commands().len(), 20);
    }

    #[test]
    fn command_tokens_have_slash() {
        for cmd in minimal_commands() {
            assert!(cmd.token.starts_with('/'));
        }
    }
}
