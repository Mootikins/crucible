// Error types for REPL operations

use colored::Colorize;
use thiserror::Error;

use super::command::CommandParseError;

/// REPL error types
#[derive(Debug, Error)]
pub enum ReplError {
    #[error("Command parse error: {0}")]
    CommandParse(#[from] CommandParseError),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Tool execution error: {0}")]
    Tool(String),

    #[error("Rune script error: {0}")]
    Rune(String),

    #[error("Formatting error: {0}")]
    Formatting(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(String),
}

impl ReplError {
    /// Format error with color and helpful context
    pub fn display_pretty(&self) -> String {
        match self {
            ReplError::CommandParse(e) => {
                format!(
                    "{} Command Error: {}\n{} Type {} for available commands",
                    "❌".red(),
                    e.to_string().red(),
                    "💡".cyan(),
                    ":help".green(),
                )
            }

            ReplError::Query(msg) => {
                // Try to parse query error for better display
                if let Some((line, col, err_msg)) = Self::parse_query_error(msg) {
                    format!(
                        "{} Query Error (line {}, column {}):\n  {}\n  {}{}",
                        "❌".red(),
                        line,
                        col,
                        " ".repeat(col.saturating_sub(1)) + "^",
                        err_msg.red(),
                        "\n💡 Check SurrealDB syntax: https://surrealdb.com/docs/surrealql".cyan()
                    )
                } else {
                    format!(
                        "{} Query Error: {}\n{} Check SurrealDB syntax: https://surrealdb.com/docs/surrealql",
                        "❌".red(),
                        msg.red(),
                        "💡".cyan()
                    )
                }
            }

            ReplError::Tool(msg) => {
                format!(
                    "{} Tool Execution Failed: {}\n{} Use {} to list available tools",
                    "❌".red(),
                    msg.red(),
                    "💡".cyan(),
                    ":tools".green(),
                )
            }

            ReplError::Rune(msg) => {
                format!(
                    "{} Rune Script Error: {}\n{} Check script syntax and file path",
                    "❌".red(),
                    msg.red(),
                    "💡".cyan(),
                )
            }

            ReplError::Formatting(msg) => {
                format!("{} Output Formatting Error: {}", "❌".red(), msg.red())
            }

            ReplError::Database(msg) => {
                format!(
                    "{} Database Error: {}\n{} Check database connection and configuration",
                    "❌".red(),
                    msg.red(),
                    "💡".cyan(),
                )
            }

            ReplError::Config(msg) => {
                format!(
                    "{} Configuration Error: {}\n{} Check ~/.crucible/config.yaml",
                    "❌".red(),
                    msg.red(),
                    "💡".cyan(),
                )
            }

            ReplError::Io(e) => {
                format!("{} IO Error: {}", "❌".red(), e.to_string().red())
            }
        }
    }

    /// Attempt to parse query error message for line/column info
    /// Returns (line, column, message) if parseable
    fn parse_query_error(msg: &str) -> Option<(usize, usize, String)> {
        // Common error patterns:
        // "Parse error at line 2, column 15: ..."
        // "Syntax error (line 3, col 8): ..."

        use regex::Regex;

        let patterns = [
            Regex::new(r"line (\d+),? column (\d+):? (.*)").ok(),
            Regex::new(r"\(line (\d+),? col (\d+)\):? (.*)").ok(),
            Regex::new(r"at (\d+):(\d+):? (.*)").ok(),
        ];

        for pattern in patterns.iter().filter_map(|p| p.as_ref()) {
            if let Some(caps) = pattern.captures(msg) {
                let line = caps.get(1)?.as_str().parse().ok()?;
                let col = caps.get(2)?.as_str().parse().ok()?;
                let error_msg = caps.get(3)?.as_str().to_string();
                return Some((line, col, error_msg));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = ReplError::Query("Something went wrong".to_string());
        let display = error.display_pretty();
        assert!(display.contains("Query Error"));
        assert!(display.contains("Something went wrong"));
    }

    #[test]
    fn test_query_error_parsing() {
        let msg = "Parse error at line 2, column 15: unexpected token";
        let parsed = ReplError::parse_query_error(msg);
        assert!(parsed.is_some());

        let (line, col, err_msg) = parsed.unwrap();
        assert_eq!(line, 2);
        assert_eq!(col, 15);
        assert!(err_msg.contains("unexpected token"));
    }

    #[test]
    fn test_command_parse_error_display() {
        let error =
            ReplError::CommandParse(CommandParseError::UnknownCommand("invalid".to_string()));
        let display = error.display_pretty();
        assert!(display.contains("Command Error"));
        assert!(display.contains(":help"));
    }

    #[test]
    fn test_tool_error_display() {
        let error = ReplError::Tool("Tool not found".to_string());
        let display = error.display_pretty();
        assert!(display.contains("Tool Execution Failed"));
        assert!(display.contains(":tools"));
    }
}
