//! Tool execution types
//!
//! Data structures for tool execution results and status tracking.

use std::fmt;

/// Result of a tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Output from the tool (stdout or return value)
    pub output: String,
    /// Execution status (success or error)
    pub status: ToolStatus,
}

/// Status of tool execution
#[derive(Debug, Clone)]
pub enum ToolStatus {
    /// Tool executed successfully
    Success,
    /// Tool execution failed with error message
    Error(String),
}

impl ToolResult {
    /// Create a successful result
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            status: ToolStatus::Success,
        }
    }

    /// Create an error result
    pub fn error(message: impl Into<String>) -> Self {
        let msg = message.into();
        Self {
            output: String::new(),
            status: ToolStatus::Error(msg),
        }
    }

    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        matches!(self.status, ToolStatus::Success)
    }

    /// Check if execution failed
    pub fn is_error(&self) -> bool {
        matches!(self.status, ToolStatus::Error(_))
    }

    /// Get error message if present
    pub fn error_message(&self) -> Option<&str> {
        match &self.status {
            ToolStatus::Error(msg) => Some(msg),
            _ => None,
        }
    }
}

impl fmt::Display for ToolResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.status {
            ToolStatus::Success => write!(f, "{}", self.output),
            ToolStatus::Error(err) => write!(f, "Error: {}", err),
        }
    }
}

impl fmt::Display for ToolStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolStatus::Success => write!(f, "Success"),
            ToolStatus::Error(err) => write!(f, "Error: {}", err),
        }
    }
}
