//! Workspace tool JSON-schema definitions.
//!
//! Split out of `tools/workspace.rs` for the module-size budget; pure
//! schema data, no behavior.

use rmcp::model::Tool;
use std::sync::Arc;

/// All workspace tool definitions for registration.
#[must_use]
pub(super) fn tool_definitions() -> Vec<Tool> {
    vec![
        read_file_definition(),
        edit_file_definition(),
        write_file_definition(),
        bash_definition(),
        glob_definition(),
        grep_definition(),
    ]
}

fn read_file_definition() -> Tool {
    Tool::new(
        "read_file",
        "Read file contents. Returns content with line numbers.",
        Arc::new(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to file (absolute or relative to workspace)"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Line number to start from (1-indexed)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum lines to read"
                    }
                },
                "required": ["path"]
            })
            // SAFETY: json!() macro with object literal always produces a JSON object
            .as_object()
            .unwrap()
            .clone(),
        ),
    )
}

fn edit_file_definition() -> Tool {
    Tool::new(
        "edit_file",
        "Edit file by replacing text. old_string must match exactly.",
        Arc::new(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to file"
                    },
                    "old_string": {
                        "type": "string",
                        "description": "Text to find and replace"
                    },
                    "new_string": {
                        "type": "string",
                        "description": "Replacement text"
                    },
                    "replace_all": {
                        "type": "boolean",
                        "description": "Replace all occurrences (default: false)"
                    }
                },
                "required": ["path", "old_string", "new_string"]
            })
            // SAFETY: json!() macro with object literal always produces a JSON object
            .as_object()
            .unwrap()
            .clone(),
        ),
    )
}

fn write_file_definition() -> Tool {
    Tool::new(
        "write_file",
        "Write content to file. Creates parent directories if needed.",
        Arc::new(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to file"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write"
                    }
                },
                "required": ["path", "content"]
            })
            // SAFETY: json!() macro with object literal always produces a JSON object
            .as_object()
            .unwrap()
            .clone(),
        ),
    )
}

fn bash_definition() -> Tool {
    Tool::new(
        "bash",
        "Execute bash command. Use for git, npm, cargo, etc. Set background=true for long-running commands.",
        Arc::new(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Bash command to execute"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Timeout in milliseconds (default: 120000)"
                    },
                    "background": {
                        "type": "boolean",
                        "description": "Run in background (returns task_id immediately)"
                    }
                },
                "required": ["command"]
            })
            // SAFETY: json!() macro with object literal always produces a JSON object
            .as_object()
            .unwrap()
            .clone(),
        ),
    )
}

fn glob_definition() -> Tool {
    Tool::new(
        "glob",
        "Find files matching glob pattern (e.g., '**/*.rs').",
        Arc::new(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search (default: workspace root)"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results (default: 100)"
                    }
                },
                "required": ["pattern"]
            })
            // SAFETY: json!() macro with object literal always produces a JSON object
            .as_object()
            .unwrap()
            .clone(),
        ),
    )
}

fn grep_definition() -> Tool {
    Tool::new(
        "grep",
        "Search file contents with regex. Uses ripgrep.",
        Arc::new(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search"
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search"
                    },
                    "glob": {
                        "type": "string",
                        "description": "Filter files by glob (e.g., '*.rs')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum matches (default: 50)"
                    }
                },
                "required": ["pattern"]
            })
            // SAFETY: json!() macro with object literal always produces a JSON object
            .as_object()
            .unwrap()
            .clone(),
        ),
    )
}
