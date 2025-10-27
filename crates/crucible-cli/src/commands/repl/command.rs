// Command parsing for REPL built-in commands
//
// All built-in commands start with ':' prefix (e.g., :tools, :run, :quit)

use thiserror::Error;
use tracing::level_filters::LevelFilter;

use super::OutputFormat;

/// Built-in REPL commands
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// :tools - List available tools
    ListTools,

    /// :run <tool> <args...> - Execute a tool
    RunTool {
        tool_name: String,
        args: Vec<String>,
    },

    /// :rune <script> [args...> - Run a Rune script
    RunRune {
        script_path: String,
        args: Vec<String>,
    },

    /// :stats - Show kiln and REPL statistics
    ShowStats,

    /// :config - Display current configuration
    ShowConfig,

    /// :log <level> - Set log level (trace|debug|info|warn|error)
    SetLogLevel(LevelFilter),

    /// :format <fmt> - Set output format (table|json|csv)
    SetFormat(OutputFormat),

    /// :help [command] - Show help information
    Help(Option<String>),

    /// :history [limit] - Show command history
    ShowHistory(Option<usize>),

    /// :clear - Clear screen
    ClearScreen,

    /// :quit - Exit daemon
    Quit,
}

impl Command {
    /// Parse a command string (assumes input starts with ':')
    pub fn parse(input: &str) -> Result<Self, CommandParseError> {
        let input = input.trim();

        if !input.starts_with(':') {
            return Err(CommandParseError::MissingPrefix);
        }

        // Remove ':' prefix and split into parts
        let parts: Vec<&str> = input[1..].split_whitespace().collect();

        if parts.is_empty() {
            return Err(CommandParseError::EmptyCommand);
        }

        let cmd = parts[0];
        let args = &parts[1..];

        match cmd {
            "tools" => {
                Self::no_args_expected(cmd, args)?;
                Ok(Command::ListTools)
            }

            "run" => {
                if args.is_empty() {
                    return Err(CommandParseError::MissingArgument {
                        command: cmd.to_string(),
                        expected: "tool name".to_string(),
                    });
                }
                Ok(Command::RunTool {
                    tool_name: args[0].to_string(),
                    args: args[1..].iter().map(|s| s.to_string()).collect(),
                })
            }

            "rune" => {
                if args.is_empty() {
                    return Err(CommandParseError::MissingArgument {
                        command: cmd.to_string(),
                        expected: "script path".to_string(),
                    });
                }
                Ok(Command::RunRune {
                    script_path: args[0].to_string(),
                    args: args[1..].iter().map(|s| s.to_string()).collect(),
                })
            }

            "stats" => {
                Self::no_args_expected(cmd, args)?;
                Ok(Command::ShowStats)
            }

            "config" => {
                Self::no_args_expected(cmd, args)?;
                Ok(Command::ShowConfig)
            }

            "log" => {
                if args.is_empty() {
                    return Err(CommandParseError::MissingArgument {
                        command: cmd.to_string(),
                        expected: "log level (trace|debug|info|warn|error)".to_string(),
                    });
                }

                let level = Self::parse_log_level(args[0])?;
                Ok(Command::SetLogLevel(level))
            }

            "format" | "fmt" => {
                if args.is_empty() {
                    return Err(CommandParseError::MissingArgument {
                        command: cmd.to_string(),
                        expected: "format (table|json|csv)".to_string(),
                    });
                }

                let format = Self::parse_output_format(args[0])?;
                Ok(Command::SetFormat(format))
            }

            "help" | "h" | "?" => {
                let topic = args.first().map(|s| s.to_string());
                Ok(Command::Help(topic))
            }

            "history" | "hist" => {
                let limit = if args.is_empty() {
                    None
                } else {
                    Some(args[0].parse::<usize>().map_err(|_| {
                        CommandParseError::InvalidArgument {
                            argument: args[0].to_string(),
                            expected_type: "positive integer".to_string(),
                        }
                    })?)
                };
                Ok(Command::ShowHistory(limit))
            }

            "clear" | "cls" => {
                Self::no_args_expected(cmd, args)?;
                Ok(Command::ClearScreen)
            }

            "quit" | "exit" | "q" => {
                Self::no_args_expected(cmd, args)?;
                Ok(Command::Quit)
            }

            _ => Err(CommandParseError::UnknownCommand(cmd.to_string())),
        }
    }

    /// Validate that no arguments were provided
    fn no_args_expected(cmd: &str, args: &[&str]) -> Result<(), CommandParseError> {
        if !args.is_empty() {
            Err(CommandParseError::UnexpectedArguments {
                command: cmd.to_string(),
                args: args.iter().map(|s| s.to_string()).collect(),
            })
        } else {
            Ok(())
        }
    }

    /// Parse log level from string
    fn parse_log_level(s: &str) -> Result<LevelFilter, CommandParseError> {
        match s.to_lowercase().as_str() {
            "trace" => Ok(LevelFilter::TRACE),
            "debug" => Ok(LevelFilter::DEBUG),
            "info" => Ok(LevelFilter::INFO),
            "warn" | "warning" => Ok(LevelFilter::WARN),
            "error" => Ok(LevelFilter::ERROR),
            "off" => Ok(LevelFilter::OFF),
            _ => Err(CommandParseError::InvalidArgument {
                argument: s.to_string(),
                expected_type: "log level (trace|debug|info|warn|error|off)".to_string(),
            }),
        }
    }

    /// Parse output format from string
    fn parse_output_format(s: &str) -> Result<OutputFormat, CommandParseError> {
        match s.to_lowercase().as_str() {
            "table" => Ok(OutputFormat::Table),
            "json" => Ok(OutputFormat::Json),
            "csv" => Ok(OutputFormat::Csv),
            _ => Err(CommandParseError::InvalidArgument {
                argument: s.to_string(),
                expected_type: "format (table|json|csv)".to_string(),
            }),
        }
    }

    /// Get help text for all commands
    pub fn general_help() -> &'static str {
        r#"
╔══════════════════════════════════════════════════════════════════╗
║                    Crucible REPL Commands                        ║
╚══════════════════════════════════════════════════════════════════╝

TOOLS:
  :tools                      List all available tools
  :run <tool> [args...]       Execute a tool with arguments
  :rune <script> [args...]    Run a Rune script
  :help <tool>                Show detailed help for a specific tool
  :h <tool>                   (shorthand)

INFORMATION:
  :stats                      Show kiln and REPL statistics
  :config                     Display current configuration
  :history [limit]            Show command history (default: 20)

CONFIGURATION:
  :log <level>                Set log level (trace|debug|info|warn|error)
  :format <fmt>               Set output format (table|json|csv)

UTILITY:
  :help [command]             Show help (optionally for specific command)
  :clear                      Clear screen
  :quit                       Exit daemon

SURREALQL QUERIES:
  Any input not starting with ':' is treated as a SurrealQL query.

  Examples:
    SELECT * FROM notes;
    SELECT title, tags FROM notes WHERE tags CONTAINS '#project';
    SELECT ->links->note.title FROM notes WHERE path = 'foo.md';

SHORTCUTS:
  Ctrl+C                      Cancel running query (or quit if idle)
  Ctrl+D                      Exit REPL
  Ctrl+R                      Search command history
  Tab                         Autocomplete commands and table names

TIP: Use :help <command> for detailed help on a specific command.
        "#
    }

    /// Get help text for a specific command
    pub fn help_for_command(cmd: &str) -> Option<&'static str> {
        match cmd {
            ":tools" | "tools" => Some(
                r#"
:tools - List Available Tools

USAGE:
  :tools

DESCRIPTION:
  Displays all available tools (both built-in and Rune scripts) with
  their descriptions. These tools can be executed using :run.

EXAMPLES:
  :tools

SEE ALSO:
  :run, :rune
                "#,
            ),

            ":run" | "run" => Some(
                r#"
:run - Execute Tool

USAGE:
  :run <tool_name> [args...]

DESCRIPTION:
  Executes a tool by name with optional arguments. Tools can be:
  - Built-in Rust tools (search, metadata, etc.)
  - Rune scripts loaded from ~/.crucible/scripts/

ARGUMENTS:
  tool_name     Name of the tool to execute (required)
  args          Tool-specific arguments (optional)

EXAMPLES:
  :run search_by_tags project ai
  :run metadata Projects/crucible.md
  :run semantic_search "agent orchestration"

SEE ALSO:
  :tools, :rune
                "#,
            ),

            ":rune" | "rune" => Some(
                r#"
:rune - Run Rune Script

USAGE:
  :rune <script_path> [args...]

DESCRIPTION:
  Executes a Rune script file with optional arguments. Scripts can
  access the database and tool registry.

ARGUMENTS:
  script_path   Path to .rn script file (required)
  args          Script-specific arguments (optional)

EXAMPLES:
  :rune custom_query.rn
  :rune scripts/analytics.rn --format json
  :rune ~/tools/export.rn output.csv

SEE ALSO:
  :run, :tools
                "#,
            ),

            ":log" | "log" => Some(
                r#"
:log - Set Log Level

USAGE:
  :log <level>

DESCRIPTION:
  Changes the logging verbosity. Affects both console output and
  the daemon.log file.

LEVELS:
  trace         Most verbose (all events)
  debug         Debugging information
  info          Informational messages (default)
  warn          Warnings only
  error         Errors only
  off           No logging

EXAMPLES:
  :log debug
  :log info
  :log error

NOTE:
  Log level persists for the current session only.
                "#,
            ),

            ":format" | ":fmt" | "format" | "fmt" => Some(
                r#"
:format - Set Output Format

USAGE:
  :format <format>
  :fmt <format>

DESCRIPTION:
  Changes how query results are displayed. Does not affect
  command output (which is always human-readable).

FORMATS:
  table         Human-readable table (default)
  json          JSON format (for piping to jq)
  csv           CSV format (for export)

EXAMPLES:
  :format table
  :format json
  :fmt csv

NOTE:
  Format persists for the current session.
                "#,
            ),

            ":stats" | "stats" => Some(
                r#"
:stats - Show Statistics

USAGE:
  :stats

DESCRIPTION:
  Displays statistics about the REPL session and kiln:
  - Number of commands/queries executed
  - Average query time
  - History size
  - Number of tools loaded
  - Database connection status

EXAMPLES:
  :stats
                "#,
            ),

            ":config" | "config" => Some(
                r#"
:config - Show Configuration

USAGE:
  :config

DESCRIPTION:
  Displays current daemon configuration:
  - Kiln path
  - Database path
  - History file location
  - Output format
  - Query timeout
  - Other settings

EXAMPLES:
  :config

SEE ALSO:
  Configuration file: ~/.crucible/config.yaml
                "#,
            ),

            _ => None,
        }
    }
}

/// Error type for command parsing
#[derive(Debug, Error)]
pub enum CommandParseError {
    #[error("Command must start with ':'")]
    MissingPrefix,

    #[error("Empty command")]
    EmptyCommand,

    #[error("Unknown command: '{0}'. Type :help for available commands.")]
    UnknownCommand(String),

    #[error("Command '{command}' requires argument: {expected}")]
    MissingArgument { command: String, expected: String },

    #[error("Command '{command}' does not accept arguments: {}", .args.join(", "))]
    UnexpectedArguments { command: String, args: Vec<String> },

    #[error("Invalid argument '{argument}': expected {expected_type}")]
    InvalidArgument {
        argument: String,
        expected_type: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_commands() {
        assert_eq!(Command::parse(":tools").unwrap(), Command::ListTools);
        assert_eq!(Command::parse(":stats").unwrap(), Command::ShowStats);
        assert_eq!(Command::parse(":config").unwrap(), Command::ShowConfig);
        assert_eq!(Command::parse(":quit").unwrap(), Command::Quit);
    }

    #[test]
    fn test_parse_run_tool() {
        match Command::parse(":run search_by_tags project ai").unwrap() {
            Command::RunTool { tool_name, args } => {
                assert_eq!(tool_name, "search_by_tags");
                assert_eq!(args, vec!["project", "ai"]);
            }
            _ => panic!("Expected RunTool"),
        }
    }

    #[test]
    fn test_parse_rune_script() {
        match Command::parse(":rune custom.rn arg1 arg2").unwrap() {
            Command::RunRune { script_path, args } => {
                assert_eq!(script_path, "custom.rn");
                assert_eq!(args, vec!["arg1", "arg2"]);
            }
            _ => panic!("Expected RunRune"),
        }
    }

    #[test]
    fn test_parse_log_level() {
        match Command::parse(":log debug").unwrap() {
            Command::SetLogLevel(level) => {
                assert_eq!(level, LevelFilter::DEBUG);
            }
            _ => panic!("Expected SetLogLevel"),
        }
    }

    #[test]
    fn test_parse_format() {
        match Command::parse(":format json").unwrap() {
            Command::SetFormat(format) => {
                assert!(matches!(format, OutputFormat::Json));
            }
            _ => panic!("Expected SetFormat"),
        }
    }

    #[test]
    fn test_parse_help() {
        match Command::parse(":help run").unwrap() {
            Command::Help(topic) => {
                assert_eq!(topic, Some("run".to_string()));
            }
            _ => panic!("Expected Help"),
        }
    }

    #[test]
    fn test_parse_history() {
        match Command::parse(":history 50").unwrap() {
            Command::ShowHistory(limit) => {
                assert_eq!(limit, Some(50));
            }
            _ => panic!("Expected ShowHistory"),
        }
    }

    #[test]
    fn test_command_aliases() {
        assert_eq!(Command::parse(":h").unwrap(), Command::Help(None));
        assert_eq!(Command::parse(":?").unwrap(), Command::Help(None));
        assert_eq!(Command::parse(":q").unwrap(), Command::Quit);
        assert_eq!(Command::parse(":exit").unwrap(), Command::Quit);
        assert_eq!(Command::parse(":cls").unwrap(), Command::ClearScreen);
    }

    #[test]
    fn test_error_cases() {
        // Missing prefix
        assert!(matches!(
            Command::parse("tools"),
            Err(CommandParseError::MissingPrefix)
        ));

        // Unknown command
        assert!(matches!(
            Command::parse(":unknown"),
            Err(CommandParseError::UnknownCommand(_))
        ));

        // Missing arguments
        assert!(matches!(
            Command::parse(":run"),
            Err(CommandParseError::MissingArgument { .. })
        ));

        // Unexpected arguments
        assert!(matches!(
            Command::parse(":tools extra"),
            Err(CommandParseError::UnexpectedArguments { .. })
        ));
    }
}
