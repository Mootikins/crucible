//! Types for Rune tool system

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// A discovered Rune tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneTool {
    /// Tool name (derived from filename or #[tool] attribute)
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema (JSON Schema)
    pub input_schema: Value,
    /// Path to the .rn file
    pub path: PathBuf,
    /// Function to call in the script
    pub entry_point: String,
    /// Optional version
    pub version: Option<String>,
    /// Optional tags
    pub tags: Vec<String>,
    /// Whether the tool is enabled
    pub enabled: bool,
}

impl RuneTool {
    /// Create a new RuneTool with default values
    pub fn new(name: impl Into<String>, path: PathBuf) -> Self {
        let name = name.into();
        Self {
            description: format!("Rune tool: {}", name),
            name,
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            path,
            entry_point: "main".to_string(),
            version: None,
            tags: vec![],
            enabled: true,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set input schema
    pub fn with_schema(mut self, schema: Value) -> Self {
        self.input_schema = schema;
        self
    }

    /// Set entry point function
    pub fn with_entry_point(mut self, entry: impl Into<String>) -> Self {
        self.entry_point = entry.into();
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// Configuration for Rune tool discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneDiscoveryConfig {
    /// Directories to scan for .rn files
    pub tool_directories: Vec<PathBuf>,
    /// File extensions to look for
    pub extensions: Vec<String>,
    /// Whether to scan subdirectories
    pub recursive: bool,
}

impl Default for RuneDiscoveryConfig {
    fn default() -> Self {
        Self {
            tool_directories: vec![],
            extensions: vec!["rn".to_string(), "rune".to_string()],
            recursive: true,
        }
    }
}

impl RuneDiscoveryConfig {
    /// Create config with default directories
    ///
    /// Default directories:
    /// - `~/.crucible/runes/` (global)
    /// - `{kiln}/runes/` (kiln-specific, if provided)
    pub fn with_defaults(kiln_path: Option<&std::path::Path>) -> Self {
        let mut dirs = vec![];

        // Global runes directory
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".crucible").join("runes"));
        }

        // Kiln-specific runes directory
        if let Some(kiln) = kiln_path {
            dirs.push(kiln.join("runes"));
        }

        Self {
            tool_directories: dirs,
            ..Default::default()
        }
    }

    /// Merge with another config (for overlays)
    pub fn merge(&mut self, other: &RuneDiscoveryConfig) {
        // Add directories from other, avoiding duplicates
        for dir in &other.tool_directories {
            if !self.tool_directories.contains(dir) {
                self.tool_directories.push(dir.clone());
            }
        }

        // Add extensions from other
        for ext in &other.extensions {
            if !self.extensions.contains(ext) {
                self.extensions.push(ext.clone());
            }
        }
    }
}

/// Result of executing a Rune tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneExecutionResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Return value (serialized to JSON)
    pub result: Option<Value>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Tool name that was executed
    pub tool_name: String,
}

impl RuneExecutionResult {
    /// Create a successful result
    pub fn success(tool_name: impl Into<String>, result: Value, time_ms: u64) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            execution_time_ms: time_ms,
            tool_name: tool_name.into(),
        }
    }

    /// Create a failed result
    pub fn failure(tool_name: impl Into<String>, error: impl Into<String>, time_ms: u64) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(error.into()),
            execution_time_ms: time_ms,
            tool_name: tool_name.into(),
        }
    }
}
