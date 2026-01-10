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
        repl("model", "Switch model (opens picker)"),
    ]
}

// =============================================================================
// Session fixtures
// =============================================================================

/// Helper to create a session item
pub fn session(id: impl Into<String>, description: impl Into<String>) -> PopupItem {
    PopupItem::session(id.into()).desc(description)
}

/// Helper to create a session with message count
pub fn session_with_count(
    id: impl Into<String>,
    description: impl Into<String>,
    count: u32,
) -> PopupItem {
    PopupItem::session(id.into())
        .desc(description)
        .with_message_count(count)
}

/// Test sessions for /resume popup
pub fn test_sessions() -> Vec<PopupItem> {
    vec![
        session_with_count("chat-2025-01-04-abc123", "Chat session from yesterday", 15),
        session_with_count("chat-2025-01-03-def456", "Earlier chat session", 8),
        session_with_count("chat-2025-01-02-ghi789", "Older chat session", 42),
    ]
}

/// Many sessions for scroll/filter testing
pub fn many_sessions() -> Vec<PopupItem> {
    (0..20)
        .map(|i| {
            session_with_count(
                format!("chat-2025-01-{:02}-sess{:03}", 20 - (i / 5), i),
                format!("Session from day {}", 20 - (i / 5)),
                (10 + i * 3) as u32,
            )
        })
        .collect()
}

// =============================================================================
// Model fixtures
// =============================================================================

/// Helper to create a model item
pub fn model(spec: impl Into<String>, description: impl Into<String>) -> PopupItem {
    PopupItem::model(spec.into()).desc(description)
}

/// Helper to create a model item marked as current
pub fn model_current(spec: impl Into<String>, description: impl Into<String>) -> PopupItem {
    PopupItem::model(spec.into())
        .desc(description)
        .with_current(true)
        .with_score(1000) // Current model sorts to top
}

/// Test models for :model popup
pub fn test_models() -> Vec<PopupItem> {
    vec![
        model_current("ollama/llama3.2", "Ollama - Llama 3.2"),
        model("ollama/qwen2.5-coder:32b", "Ollama - Qwen 2.5 Coder 32B"),
        model("openai/gpt-4o", "OpenAI - GPT-4o"),
        model("anthropic/claude-sonnet-4", "Anthropic - Claude Sonnet 4"),
    ]
}

/// Many models for scroll testing
pub fn many_models() -> Vec<PopupItem> {
    vec![
        model_current("ollama/llama3.2", "Ollama - Llama 3.2"),
        model("ollama/llama3.1", "Ollama - Llama 3.1"),
        model("ollama/qwen2.5-coder:32b", "Ollama - Qwen 2.5 Coder 32B"),
        model("ollama/qwen2.5-coder:14b", "Ollama - Qwen 2.5 Coder 14B"),
        model("ollama/deepseek-r1:32b", "Ollama - DeepSeek R1 32B"),
        model("ollama/codellama:34b", "Ollama - CodeLlama 34B"),
        model("openai/gpt-4o", "OpenAI - GPT-4o"),
        model("openai/gpt-4o-mini", "OpenAI - GPT-4o Mini"),
        model("openai/gpt-4-turbo", "OpenAI - GPT-4 Turbo"),
        model("anthropic/claude-sonnet-4", "Anthropic - Claude Sonnet 4"),
        model("anthropic/claude-opus-4", "Anthropic - Claude Opus 4"),
        model("anthropic/claude-haiku-3.5", "Anthropic - Claude Haiku 3.5"),
        model("acp/opencode", "ACP - OpenCode Agent"),
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

    #[test]
    fn test_sessions_not_empty() {
        assert!(!test_sessions().is_empty());
    }

    #[test]
    fn session_tokens_are_session_ids() {
        // Session tokens are the raw session ID (used for resumption, not insertion)
        for s in test_sessions() {
            assert!(
                s.token().starts_with("chat-"),
                "Session token should be session ID starting with 'chat-', got: {}",
                s.token()
            );
        }
    }

    #[test]
    fn session_has_message_count() {
        let s = session_with_count("test-123", "Test session", 42);
        if let PopupItem::Session { message_count, .. } = s {
            assert_eq!(message_count, 42);
        } else {
            panic!("Expected Session variant");
        }
    }

    #[test]
    fn many_sessions_has_expected_count() {
        assert_eq!(many_sessions().len(), 20);
    }

    #[test]
    fn test_models_not_empty() {
        assert!(!test_models().is_empty());
    }

    #[test]
    fn model_tokens_are_specs() {
        for m in test_models() {
            // Model tokens are the provider/model spec
            assert!(
                m.token().contains('/'),
                "Model token should be provider/model format, got: {}",
                m.token()
            );
        }
    }

    #[test]
    fn model_current_is_marked() {
        let m = model_current("ollama/test", "Test model");
        if let PopupItem::Model { current, .. } = m {
            assert!(current);
        } else {
            panic!("Expected Model variant");
        }
    }

    #[test]
    fn many_models_has_expected_count() {
        assert_eq!(many_models().len(), 13);
    }
}
