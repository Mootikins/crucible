//! Command parsing for chat interface
//!
//! Parses slash commands and special keybindings into Command enum.

/// Chat command variants
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Exit the chat (/exit or /quit)
    Exit,
    /// Search the knowledge base (/search <query>)
    Search(String),
    /// Switch to Plan mode (/plan)
    Plan,
    /// Switch to Act mode (/act)
    Act,
    /// Switch to AutoApprove mode (/auto)
    Auto,
    /// Cycle to next mode (/mode)
    Mode,
    /// Silent mode cycle (Shift+Tab keybinding)
    SilentMode,
}

/// Command parser
pub struct CommandParser;

impl CommandParser {
    /// Parse a string into a Command
    ///
    /// Returns None if the input is not a recognized command.
    pub fn parse(input: &str) -> Option<Command> {
        // Handle silent mode keybinding (Shift+Tab)
        if input == "\x00mode" {
            return Some(Command::SilentMode);
        }

        // Handle exit commands
        if input == "/exit" || input == "/quit" {
            return Some(Command::Exit);
        }

        // Handle mode switching commands
        if input == "/plan" {
            return Some(Command::Plan);
        }
        if input == "/act" {
            return Some(Command::Act);
        }
        if input == "/auto" {
            return Some(Command::Auto);
        }
        if input == "/mode" {
            return Some(Command::Mode);
        }

        // Handle search command
        if input.starts_with("/search ") {
            let query = input[8..].trim();
            if query.is_empty() {
                return None; // Empty search query is invalid
            }
            return Some(Command::Search(query.to_string()));
        }

        // Empty /search is invalid
        if input == "/search" {
            return None;
        }

        // Not a recognized command
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exit command tests
    #[test]
    fn test_parse_exit() {
        assert_eq!(CommandParser::parse("/exit"), Some(Command::Exit));
    }

    #[test]
    fn test_parse_quit() {
        assert_eq!(CommandParser::parse("/quit"), Some(Command::Exit));
    }

    #[test]
    fn test_parse_exit_case_sensitive() {
        // Commands should be case-sensitive
        assert_eq!(CommandParser::parse("/EXIT"), None);
        assert_eq!(CommandParser::parse("/Quit"), None);
    }

    // Mode switching tests
    #[test]
    fn test_parse_plan() {
        assert_eq!(CommandParser::parse("/plan"), Some(Command::Plan));
    }

    #[test]
    fn test_parse_act() {
        assert_eq!(CommandParser::parse("/act"), Some(Command::Act));
    }

    #[test]
    fn test_parse_auto() {
        assert_eq!(CommandParser::parse("/auto"), Some(Command::Auto));
    }

    #[test]
    fn test_parse_mode() {
        assert_eq!(CommandParser::parse("/mode"), Some(Command::Mode));
    }

    // Silent mode test
    #[test]
    fn test_parse_silent_mode() {
        // Shift+Tab generates "\x00mode" keybinding
        assert_eq!(CommandParser::parse("\x00mode"), Some(Command::SilentMode));
    }

    // Search command tests
    #[test]
    fn test_parse_search_with_query() {
        assert_eq!(
            CommandParser::parse("/search hello world"),
            Some(Command::Search("hello world".to_string()))
        );
    }

    #[test]
    fn test_parse_search_with_leading_whitespace() {
        assert_eq!(
            CommandParser::parse("/search   leading spaces"),
            Some(Command::Search("leading spaces".to_string()))
        );
    }

    #[test]
    fn test_parse_search_with_trailing_whitespace() {
        assert_eq!(
            CommandParser::parse("/search trailing   "),
            Some(Command::Search("trailing".to_string()))
        );
    }

    #[test]
    fn test_parse_search_empty_query() {
        // Empty search should return None (invalid)
        assert_eq!(CommandParser::parse("/search"), None);
        assert_eq!(CommandParser::parse("/search   "), None);
    }

    #[test]
    fn test_parse_search_preserves_internal_whitespace() {
        assert_eq!(
            CommandParser::parse("/search multiple   spaces   inside"),
            Some(Command::Search("multiple   spaces   inside".to_string()))
        );
    }

    // Non-command tests
    #[test]
    fn test_parse_regular_message() {
        assert_eq!(CommandParser::parse("regular message"), None);
    }

    #[test]
    fn test_parse_message_starting_with_slash() {
        // Unknown slash command should return None
        assert_eq!(CommandParser::parse("/unknown"), None);
    }

    #[test]
    fn test_parse_empty_string() {
        assert_eq!(CommandParser::parse(""), None);
    }

    #[test]
    fn test_parse_whitespace_only() {
        assert_eq!(CommandParser::parse("   "), None);
    }

    #[test]
    fn test_parse_command_with_extra_text() {
        // Commands shouldn't have extra arguments (except search)
        assert_eq!(CommandParser::parse("/exit now"), None);
        assert_eq!(CommandParser::parse("/plan please"), None);
        assert_eq!(CommandParser::parse("/act now"), None);
    }

    #[test]
    fn test_parse_slash_only() {
        assert_eq!(CommandParser::parse("/"), None);
    }

    // Command enum tests
    #[test]
    fn test_command_clone() {
        let cmd = Command::Search("test".to_string());
        let cloned = cmd.clone();
        assert_eq!(cmd, cloned);
    }

    #[test]
    fn test_command_debug() {
        let cmd = Command::Exit;
        let debug_str = format!("{:?}", cmd);
        assert!(debug_str.contains("Exit"));
    }

    #[test]
    fn test_command_equality() {
        assert_eq!(Command::Exit, Command::Exit);
        assert_eq!(Command::Plan, Command::Plan);
        assert_eq!(
            Command::Search("test".to_string()),
            Command::Search("test".to_string())
        );

        assert_ne!(Command::Exit, Command::Plan);
        assert_ne!(
            Command::Search("a".to_string()),
            Command::Search("b".to_string())
        );
    }
}
