//! MCP-style tool calling infrastructure using rmcp
//!
//! This module provides:
//! - CLI tool definitions using rmcp's Tool type
//! - Grammar generation for constrained tool call output
//! - Scoring for MCP-formatted tool calls
//! - Call/response flow testing
//!
//! ## MCP Tool Call Format (JSON-RPC style)
//!
//! Request:
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "method": "tools/call",
//!   "params": {
//!     "name": "rg",
//!     "arguments": {"pattern": "TODO", "path": "src/"}
//!   },
//!   "id": 1
//! }
//! ```
//!
//! Response:
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "result": {
//!     "content": [{"type": "text", "text": "..."}],
//!     "isError": false
//!   },
//!   "id": 1
//! }
//! ```

use rmcp::model::{CallToolResult, Content, Tool, ToolAnnotations};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

/// Tool call request parameters (what model generates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallParams {
    /// Tool name
    pub name: String,
    /// Tool arguments
    pub arguments: Value,
}

/// CLI tools exposed via MCP
pub struct CliTools;

impl CliTools {
    /// Get all CLI tool definitions using rmcp::model::Tool
    pub fn all() -> Vec<Tool> {
        vec![Self::rg(), Self::fd(), Self::cat(), Self::ls()]
    }

    /// ripgrep - search file contents
    pub fn rg() -> Tool {
        Tool {
            name: Cow::Borrowed("rg"),
            title: Some("ripgrep".to_string()),
            description: Some(Cow::Borrowed("Search file contents using ripgrep")),
            input_schema: Arc::new(serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Search pattern (regex supported)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to search (default: current directory)"
                    },
                    "flags": {
                        "type": "string",
                        "description": "Additional rg flags (e.g., '-i' for case-insensitive)"
                    }
                },
                "required": ["pattern"]
            })).unwrap()),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// fd - find files
    pub fn fd() -> Tool {
        Tool {
            name: Cow::Borrowed("fd"),
            title: Some("fd-find".to_string()),
            description: Some(Cow::Borrowed("Find files by name pattern using fd")),
            input_schema: Arc::new(serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "File name pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (default: current directory)"
                    },
                    "extension": {
                        "type": "string",
                        "description": "Filter by file extension (e.g., 'rs', 'py')"
                    }
                },
                "required": ["pattern"]
            })).unwrap()),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// cat - read files
    pub fn cat() -> Tool {
        Tool {
            name: Cow::Borrowed("cat"),
            title: Some("cat".to_string()),
            description: Some(Cow::Borrowed("Read and display file contents")),
            input_schema: Arc::new(serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file to read"
                    },
                    "lines": {
                        "type": "string",
                        "description": "Line range (e.g., '1-10' or '50-')"
                    }
                },
                "required": ["path"]
            })).unwrap()),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// ls - list directory
    pub fn ls() -> Tool {
        Tool {
            name: Cow::Borrowed("ls"),
            title: Some("ls".to_string()),
            description: Some(Cow::Borrowed("List directory contents")),
            input_schema: Arc::new(serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path to list (default: current directory)"
                    },
                    "all": {
                        "type": "boolean",
                        "description": "Include hidden files"
                    },
                    "long": {
                        "type": "boolean",
                        "description": "Use long listing format"
                    }
                },
                "required": []
            })).unwrap()),
            output_schema: None,
            annotations: Some(ToolAnnotations::new().read_only(true)),
            icons: None,
            meta: None,
        }
    }

    /// Passthrough tool - accepts raw CLI args
    pub fn passthrough(name: &'static str, description: &'static str) -> Tool {
        Tool {
            name: Cow::Borrowed(name),
            title: Some(name.to_string()),
            description: Some(Cow::Borrowed(description)),
            input_schema: Arc::new(serde_json::from_value(json!({
                "type": "object",
                "properties": {
                    "args": {
                        "type": "string",
                        "description": "Command line arguments to pass directly"
                    }
                },
                "required": ["args"]
            })).unwrap()),
            output_schema: None,
            annotations: None,
            icons: None,
            meta: None,
        }
    }

    /// Get passthrough versions of all CLI tools
    pub fn all_passthrough() -> Vec<Tool> {
        vec![
            Self::passthrough("rg", "ripgrep - search file contents"),
            Self::passthrough("fd", "fd-find - find files by name"),
            Self::passthrough("cat", "Read file contents"),
            Self::passthrough("ls", "List directory contents"),
        ]
    }
}

/// Generate system prompt from rmcp Tool definitions
pub fn tools_to_system_prompt(tools: &[Tool]) -> String {
    let mut prompt = String::from("You are a tool-calling assistant. Available tools:\n\n");

    for tool in tools {
        prompt.push_str(&format!("## {}\n", tool.name));
        if let Some(desc) = &tool.description {
            prompt.push_str(&format!("{}\n", desc));
        }

        // Extract properties from schema
        if let Some(props) = tool.input_schema.get("properties") {
            prompt.push_str("Parameters:\n");
            if let Some(obj) = props.as_object() {
                let required: Vec<&str> = tool
                    .input_schema
                    .get("required")
                    .and_then(|r| r.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();

                for (name, schema) in obj {
                    let desc = schema
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");
                    let req_marker = if required.contains(&name.as_str()) {
                        " (required)"
                    } else {
                        ""
                    };
                    prompt.push_str(&format!("- {}{}: {}\n", name, req_marker, desc));
                }
            }
        }
        prompt.push('\n');
    }

    prompt.push_str("\nRespond with a JSON tool call in this format:\n");
    prompt.push_str(r#"{"name": "tool_name", "arguments": {...}}"#);
    prompt.push_str("\n\nOutput ONLY the JSON, nothing else.");

    prompt
}

/// Generate GBNF grammar for MCP tool calls
pub fn tools_to_grammar(tools: &[Tool]) -> String {
    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    // Build tool name alternatives
    let tool_alts = tool_names
        .iter()
        .map(|n| format!("\"{}\"", n))
        .collect::<Vec<_>>()
        .join(" | ");

    format!(
        r#"root ::= "{{" ws "\"name\":" ws tool-name "," ws "\"arguments\":" ws arguments ws "}}"
tool-name ::= {tool_alts}
arguments ::= "{{" ws (argument ("," ws argument)*)? ws "}}"
argument ::= "\"" key "\":" ws value
key ::= [a-zA-Z_][a-zA-Z0-9_]*
value ::= string | number | boolean | "null"
string ::= "\"" ([^"\\] | "\\" .)* "\""
number ::= "-"? [0-9]+ ("." [0-9]+)?
boolean ::= "true" | "false"
ws ::= [ \t\n]*"#,
        tool_alts = tool_alts
    )
}

/// Parse tool call params from model output
///
/// Handles both valid JSON and common malformed outputs:
/// - Unquoted string values: `{"name": rg, ...}` → `{"name": "rg", ...}`
/// - Extra whitespace/tabs
pub fn parse_tool_call(output: &str) -> Option<ToolCallParams> {
    let output = output.trim();

    // Find the JSON object
    let start = output.find('{')?;
    let end = output.rfind('}')?;

    if start > end {
        return None;
    }

    let json_str = &output[start..=end];

    // First try standard JSON parsing
    if let Ok(parsed) = serde_json::from_str::<ToolCallParams>(json_str) {
        return Some(parsed);
    }

    // Try to fix common issues:
    // 1. Unquoted tool names: {"name": rg, -> {"name": "rg",
    let fixed = fix_unquoted_values(json_str);
    serde_json::from_str::<ToolCallParams>(&fixed).ok()
}

/// Fix common JSON issues: unquoted string values
fn fix_unquoted_values(json: &str) -> String {
    use regex::Regex;

    // Match: "name": followed by unquoted word (not starting with {, [, ", digit, true, false, null)
    // Pattern: "key": value where value is unquoted
    let re = Regex::new(r#"("(?:name|args|pattern|path|extension)")\s*:\s*([a-zA-Z_][a-zA-Z0-9_\-\./*]*)"#).unwrap();

    let result = re.replace_all(json, |caps: &regex::Captures| {
        format!(r#"{}: "{}""#, &caps[1], &caps[2])
    });

    result.to_string()
}

/// Score a tool call against expected
pub fn score_tool_call(
    actual: &ToolCallParams,
    expected_tool: &str,
    expected_args: &HashMap<String, String>,
) -> crate::scoring::Score {
    let tool_correct = actual.name == expected_tool;

    let param_accuracy = if expected_args.is_empty() {
        1.0
    } else {
        let matches = expected_args
            .iter()
            .filter(|(key, val)| {
                actual
                    .arguments
                    .get(*key)
                    .and_then(|v| v.as_str())
                    .map(|v| v.contains(val.as_str()))
                    .unwrap_or(false)
            })
            .count();
        matches as f64 / expected_args.len() as f64
    };

    crate::scoring::Score {
        parsed: true,
        tool_correct,
        param_accuracy,
        task_success: None,
    }
}

/// Execute a tool call and return CallToolResult
///
/// Supports two modes:
/// 1. **Structured**: Individual parameters (pattern, path, etc.)
/// 2. **Passthrough**: Single "args" parameter with raw CLI args
pub async fn execute_tool_call(params: &ToolCallParams) -> CallToolResult {
    use std::process::Command;

    // Check for passthrough mode: {"args": "..."}
    if let Some(args_str) = params.arguments.get("args").and_then(|v| v.as_str()) {
        // Passthrough: execute command with raw args
        let mut cmd = Command::new(&params.name);
        for arg in args_str.split_whitespace() {
            cmd.arg(arg);
        }
        return match cmd.output() {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    CallToolResult::success(vec![Content::text(stdout.to_string())])
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    CallToolResult::error(vec![Content::text(stderr.to_string())])
                }
            }
            Err(e) => CallToolResult::error(vec![Content::text(format!("Execution error: {}", e))]),
        };
    }

    // Structured mode: individual parameters
    let result = match params.name.as_str() {
        "rg" => {
            let pattern = params
                .arguments
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let path = params
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let flags = params
                .arguments
                .get("flags")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mut cmd = Command::new("rg");
            if !flags.is_empty() {
                for flag in flags.split_whitespace() {
                    cmd.arg(flag);
                }
            }
            cmd.arg(pattern).arg(path);
            cmd.output()
        }
        "fd" => {
            let pattern = params
                .arguments
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let path = params
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let ext = params.arguments.get("extension").and_then(|v| v.as_str());

            let mut cmd = Command::new("fd");
            if let Some(e) = ext {
                cmd.arg("-e").arg(e);
            }
            cmd.arg(pattern).arg(path);
            cmd.output()
        }
        "cat" => {
            let path = params
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Command::new("cat").arg(path).output()
        }
        "ls" => {
            let path = params
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let all = params
                .arguments
                .get("all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let long = params
                .arguments
                .get("long")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let mut cmd = Command::new("ls");
            if all {
                cmd.arg("-a");
            }
            if long {
                cmd.arg("-l");
            }
            cmd.arg(path);
            cmd.output()
        }
        _ => {
            return CallToolResult::error(vec![Content::text(format!(
                "Unknown tool: {}",
                params.name
            ))]);
        }
    };

    match result {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                CallToolResult::success(vec![Content::text(stdout.to_string())])
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                CallToolResult::error(vec![Content::text(stderr.to_string())])
            }
        }
        Err(e) => CallToolResult::error(vec![Content::text(format!("Execution error: {}", e))]),
    }
}

/// A complete MCP call/response flow
#[derive(Debug, Clone)]
pub struct McpFlow {
    /// User request
    pub user_message: String,
    /// Tool call generated by model
    pub tool_call: ToolCallParams,
    /// Result from tool execution
    pub tool_result: CallToolResult,
    /// Follow-up assistant response (if any)
    pub assistant_response: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_tools_schema() {
        let tools = CliTools::all();
        assert_eq!(tools.len(), 4);

        let rg = &tools[0];
        assert_eq!(rg.name, "rg");
        assert!(rg.input_schema.get("properties").is_some());

        // Check annotations
        assert!(rg.annotations.is_some());
        let annotations = rg.annotations.as_ref().unwrap();
        assert_eq!(annotations.read_only_hint, Some(true));
    }

    #[test]
    fn test_parse_tool_call() {
        let json = r#"{"name": "rg", "arguments": {"pattern": "TODO", "path": "src/"}}"#;
        let call = parse_tool_call(json).unwrap();

        assert_eq!(call.name, "rg");
        assert_eq!(call.arguments.get("pattern").unwrap(), "TODO");
        assert_eq!(call.arguments.get("path").unwrap(), "src/");
    }

    #[test]
    fn test_parse_tool_call_with_surrounding_text() {
        let json = r#"Here's the tool call: {"name": "fd", "arguments": {"pattern": "*.rs"}} That should work."#;
        let call = parse_tool_call(json).unwrap();

        assert_eq!(call.name, "fd");
    }

    #[test]
    fn test_parse_tool_call_unquoted_name() {
        // Model sometimes generates unquoted tool names
        let json = r#"{"name": rg, "arguments": {"pattern": "TODO"}}"#;
        let call = parse_tool_call(json).unwrap();

        assert_eq!(call.name, "rg");
        assert_eq!(call.arguments.get("pattern").unwrap(), "TODO");
    }

    #[test]
    fn test_parse_tool_call_with_tabs() {
        // Model generates tabs and whitespace
        let json = r#"{"name": 	cat, 	"arguments":	{	"path": 	"Cargo.toml"	}}"#;
        let call = parse_tool_call(json).unwrap();

        assert_eq!(call.name, "cat");
    }

    #[test]
    fn test_tools_to_system_prompt() {
        let tools = CliTools::all();
        let prompt = tools_to_system_prompt(&tools);

        assert!(prompt.contains("## rg"));
        assert!(prompt.contains("pattern (required)"));
        assert!(prompt.contains(r#"{"name": "tool_name"#));
    }

    #[test]
    fn test_tools_to_grammar() {
        let tools = CliTools::all();
        let grammar = tools_to_grammar(&tools);

        assert!(grammar.contains(r#""rg" | "fd" | "cat" | "ls""#));
        assert!(grammar.contains("tool-name"));
        assert!(grammar.contains("arguments"));
    }

    #[test]
    fn test_score_tool_call() {
        let call = ToolCallParams {
            name: "rg".to_string(),
            arguments: json!({"pattern": "TODO", "path": "src/"}),
        };

        let expected_args: HashMap<String, String> =
            [("pattern".to_string(), "TODO".to_string())]
                .into_iter()
                .collect();

        let score = score_tool_call(&call, "rg", &expected_args);
        assert!(score.tool_correct);
        assert_eq!(score.param_accuracy, 1.0);
    }

    #[test]
    fn test_passthrough_tools() {
        let tools = CliTools::all_passthrough();
        assert_eq!(tools.len(), 4);

        // All passthrough tools have single "args" parameter
        for tool in &tools {
            let props = tool.input_schema.get("properties").unwrap();
            assert!(props.get("args").is_some());
        }
    }

    #[tokio::test]
    async fn test_execute_ls() {
        let params = ToolCallParams {
            name: "ls".to_string(),
            arguments: json!({"path": "."}),
        };

        let result = execute_tool_call(&params).await;
        assert!(result.is_error.is_none() || result.is_error == Some(false));
        assert!(!result.content.is_empty());
    }
}
