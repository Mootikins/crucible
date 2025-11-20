// Autocomplete for commands, tool names, and table names

use reedline::{Completer, Span, Suggestion};
use std::sync::Arc;

use crucible_core::CrucibleCore;

/// REPL autocompleter
pub struct ReplCompleter {
    /// Core coordinator for database introspection
    core: Arc<CrucibleCore>,

    /// Built-in commands
    commands: Vec<CommandCompletion>,
}

/// Command completion with description
struct CommandCompletion {
    name: String,
    description: String,
}

impl ReplCompleter {
    pub fn new(core: Arc<CrucibleCore>) -> Self {
        let commands = Self::build_command_list();
        Self {
            core,
            commands,
        }
    }

    /// Build list of built-in commands with descriptions
    fn build_command_list() -> Vec<CommandCompletion> {
        vec![
            CommandCompletion {
                name: ":tools".to_string(),
                description: "List available tools".to_string(),
            },
            CommandCompletion {
                name: ":run".to_string(),
                description: "Execute a tool".to_string(),
            },
            CommandCompletion {
                name: ":rune".to_string(),
                description: "Run a Rune script".to_string(),
            },
            CommandCompletion {
                name: ":stats".to_string(),
                description: "Show statistics".to_string(),
            },
            CommandCompletion {
                name: ":config".to_string(),
                description: "Show configuration".to_string(),
            },
            CommandCompletion {
                name: ":log".to_string(),
                description: "Set log level".to_string(),
            },
            CommandCompletion {
                name: ":format".to_string(),
                description: "Set output format".to_string(),
            },
            CommandCompletion {
                name: ":help".to_string(),
                description: "Show help".to_string(),
            },
            CommandCompletion {
                name: ":history".to_string(),
                description: "Show command history".to_string(),
            },
            CommandCompletion {
                name: ":clear".to_string(),
                description: "Clear screen".to_string(),
            },
            CommandCompletion {
                name: ":quit".to_string(),
                description: "Exit CLI".to_string(),
            },
        ]
    }

    /// Complete command names
    fn complete_commands(&self, prefix: &str, pos: usize) -> Vec<Suggestion> {
        self.commands
            .iter()
            .filter(|cmd| cmd.name.starts_with(prefix))
            .map(|cmd| Suggestion {
                value: cmd.name.clone(),
                description: Some(cmd.description.clone()),
                style: None,
                extra: None,
                span: Span::new(0, pos),
                append_whitespace: true,
            })
            .collect()
    }

  
    /// Complete log levels for `:log` command
    fn complete_log_levels(
        &self,
        prefix: &str,
        start_pos: usize,
        end_pos: usize,
    ) -> Vec<Suggestion> {
        let levels = [
            ("trace", "Most verbose logging"),
            ("debug", "Debug information"),
            ("info", "Informational messages"),
            ("warn", "Warnings only"),
            ("error", "Errors only"),
            ("off", "No logging"),
        ];

        levels
            .iter()
            .filter(|(level, _)| level.starts_with(prefix))
            .map(|(level, description)| Suggestion {
                value: (*level).to_string(),
                description: Some((*description).to_string()),
                style: None,
                extra: None,
                span: Span::new(start_pos, end_pos),
                append_whitespace: false,
            })
            .collect()
    }

    /// Complete output formats for `:format` command
    fn complete_formats(&self, prefix: &str, start_pos: usize, end_pos: usize) -> Vec<Suggestion> {
        let formats = [
            ("table", "Human-readable table"),
            ("json", "JSON format"),
            ("csv", "CSV format"),
        ];

        formats
            .iter()
            .filter(|(format, _)| format.starts_with(prefix))
            .map(|(format, description)| Suggestion {
                value: (*format).to_string(),
                description: Some((*description).to_string()),
                style: None,
                extra: None,
                span: Span::new(start_pos, end_pos),
                append_whitespace: false,
            })
            .collect()
    }

    /// Complete SurrealQL keywords
    fn complete_keywords(&self, prefix: &str, start_pos: usize, end_pos: usize) -> Vec<Suggestion> {
        let keywords = vec![
            "SELECT", "FROM", "WHERE", "ORDER", "BY", "LIMIT", "CREATE", "UPDATE", "DELETE",
            "INSERT", "INTO", "SET", "UNSET", "MERGE", "CONTENT", "AND", "OR", "NOT", "IN",
            "CONTAINS", "BEGIN", "COMMIT", "CANCEL",
        ];

        keywords
            .iter()
            .filter(|kw| kw.to_lowercase().starts_with(&prefix.to_lowercase()))
            .map(|kw| Suggestion {
                value: (*kw).to_string(),
                description: None,
                style: None,
                extra: None,
                span: Span::new(start_pos, end_pos),
                append_whitespace: true,
            })
            .collect()
    }

    /// Complete table names after FROM keyword using real database introspection
    fn complete_table_names(
        &self,
        prefix: &str,
        start_pos: usize,
        end_pos: usize,
    ) -> Vec<Suggestion> {
        // Try to query real table names from database
        // If no runtime is available (e.g., in tests), use fallback
        let tables = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            // We have a runtime, use it
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    self.core.list_tables().await.unwrap_or_else(|_| {
                        vec!["notes".to_string(), "tags".to_string(), "files".to_string()]
                    })
                })
            })
        } else {
            // No runtime available (test context), use fallback
            vec!["notes".to_string(), "tags".to_string(), "files".to_string()]
        };

        tables
            .iter()
            .filter(|table| table.starts_with(prefix))
            .map(|table| Suggestion {
                value: table.clone(),
                description: Some("Table".to_string()),
                style: None,
                extra: None,
                span: Span::new(start_pos, end_pos),
                append_whitespace: true,
            })
            .collect()
    }
}

impl Completer for ReplCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let prefix = &line[..pos];

        // Complete commands (starts with ':')
        if prefix.starts_with(':') && !prefix.contains(' ') {
            return self.complete_commands(prefix, pos);
        }

  
        // Complete log levels after `:log `
        if let Some(level_start) = prefix.strip_prefix(":log ") {
            let start_pos = 5; // ":log ".len()
            return self.complete_log_levels(level_start, start_pos, pos);
        }

        // Complete formats after `:format ` or `:fmt `
        if let Some(format_start) = prefix.strip_prefix(":format ") {
            let start_pos = 8; // ":format ".len()
            return self.complete_formats(format_start, start_pos, pos);
        }
        if let Some(format_start) = prefix.strip_prefix(":fmt ") {
            let start_pos = 5; // ":fmt ".len()
            return self.complete_formats(format_start, start_pos, pos);
        }

        // For SurrealQL queries, provide keyword completion
        if !prefix.starts_with(':') {
            // Find the current word being typed
            let words: Vec<&str> = prefix.split_whitespace().collect();
            if let Some(last_word) = words.last() {
                let word_start = pos - last_word.len();

                // Complete keywords
                let mut suggestions = self.complete_keywords(last_word, word_start, pos);

                // If previous word is FROM, complete table names
                if words.len() >= 2 && words[words.len() - 2].to_uppercase() == "FROM" {
                    suggestions = self.complete_table_names(last_word, word_start, pos);
                }

                return suggestions;
            }
        }

        vec![]
    }
}

