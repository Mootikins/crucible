//! LLM (Large Language Model) data types
//!
//! Canonical data types for LLM tool-calling and token accounting shared across
//! crates. Provider-specific request/response shapes live in the provider
//! adapters (crucible-daemon `llm/`), not here.

use serde::{Deserialize, Serialize};

/// Message role in LLM API conversations (canonical type).
///
/// This is the canonical message role type for LLM provider communication.
/// It maps directly to OpenAI/Anthropic API message roles.
///
/// Use this type for:
/// - LLM provider communication
/// - Session persistence (Rig sessions, TUI state)
/// - Any code that needs standard assistant/user/system roles
///
/// Note: `crucible_daemon::acp::MessageRole` uses `Agent` instead of `Assistant`
/// for ACP protocol terminology - convert using `From`/`Into` when bridging.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System message (sets behavior)
    System,
    /// User message (input)
    User,
    /// Assistant message (response)
    Assistant,
    /// Function result message (legacy, prefer Tool)
    Function,
    /// Tool result message
    Tool,
}

/// Tool call made by the assistant
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Tool type (typically "function")
    pub r#type: String,
    /// Function call details
    pub function: FunctionCall,
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: String) -> Self {
        Self {
            id: id.into(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: name.into(),
                arguments,
            },
        }
    }
}

/// Function call details
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Function arguments (JSON string)
    pub arguments: String,
}

/// Tool definition for LLM tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolDefinition {
    /// Tool type (typically "function")
    pub r#type: String,
    /// Function definition
    pub function: FunctionDefinition,
}

impl LlmToolDefinition {
    /// Create a new tool definition
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: name.into(),
                description: description.into(),
                parameters: Some(parameters),
            },
        }
    }
}

impl From<super::tools::ToolDefinition> for LlmToolDefinition {
    fn from(tool: super::tools::ToolDefinition) -> Self {
        Self {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.parameters,
            },
        }
    }
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Function parameters schema (JSON Schema)
    pub parameters: Option<serde_json::Value>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Prompt tokens used
    pub prompt_tokens: u32,
    /// Completion tokens used
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
    /// Tokens read from prompt cache (Anthropic: 90% cost reduction)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
    /// Tokens written to prompt cache (Anthropic: 1.25x cost on first write)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_tokens: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call() {
        let call = ToolCall::new("call_1", "search", r#"{"query": "rust"}"#.to_string());
        assert_eq!(call.id, "call_1");
        assert_eq!(call.function.name, "search");
    }

    #[test]
    fn test_tool_definition() {
        let tool = LlmToolDefinition::new(
            "search",
            "Search for information",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }),
        );
        assert_eq!(tool.function.name, "search");
        assert_eq!(tool.r#type, "function");
    }

    #[test]
    fn test_llm_tool_definition_from_tool_definition() {
        use crate::traits::tools::ToolDefinition;

        let tool_def = ToolDefinition::new("read_file", "Read contents of a file").with_parameters(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
        );

        let llm_tool: LlmToolDefinition = tool_def.into();

        assert_eq!(llm_tool.r#type, "function");
        assert_eq!(llm_tool.function.name, "read_file");
        assert_eq!(llm_tool.function.description, "Read contents of a file");
        assert!(llm_tool.function.parameters.is_some());
    }
}
