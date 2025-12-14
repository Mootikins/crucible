//! Dynamic MCP module generation for Rune
//!
//! Generates Rune modules from MCP server tool schemas at runtime.
//! Each MCP server becomes a module like `cru::mcp::<server_name>` with
//! functions matching the tool names and parameter signatures.
//!
//! # Example
//!
//! When connecting to a GitHub MCP server with a `search_repositories` tool:
//!
//! ```rune
//! use cru::mcp::github;
//!
//! // Function signature derived from MCP schema
//! let result = github::search_repositories("rust", 1, 30).await?;
//! ```

use crate::mcp_gateway::UpstreamMcpClient;
use crate::mcp_types::{build_args_json, McpResult};
use crucible_core::traits::{McpToolInfo as UpstreamTool, ToolCallResult};
use rune::runtime::{VmError, VmResult};
use rune::{ContextError, Module, Value};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tracing::{debug, warn};

// =============================================================================
// Macro for registering MCP tools with different arities
// =============================================================================

/// Macro to generate async function registrations for different parameter counts.
///
/// Since Rune's `Module::function()` requires concrete closure types at compile time,
/// we generate variants for arities 0-10 and dispatch at runtime based on schema.
macro_rules! register_mcp_tool {
    // 0 arguments
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 0, $_param_names:expr) => {{
        let client = $client.clone();
        let tool = $tool_name.to_string();
        $module
            .function($name, move || {
                let client = client.clone();
                let tool = tool.clone();
                async move { call_mcp_tool_async(&client, &tool, serde_json::json!({})).await }
            })
            .build()?;
    }};

    // 1 argument
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 1, $param_names:expr) => {{
        let client = $client.clone();
        let tool = $tool_name.to_string();
        let params = $param_names.clone();
        $module
            .function($name, move |a: Value| {
                let client = client.clone();
                let tool = tool.clone();
                let params = params.clone();
                async move {
                    let args = match build_args_json(&params, vec![a]) {
                        Ok(args) => args,
                        Err(e) => return VmResult::err(e),
                    };
                    call_mcp_tool_async(&client, &tool, args).await
                }
            })
            .build()?;
    }};

    // 2 arguments
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 2, $param_names:expr) => {{
        let client = $client.clone();
        let tool = $tool_name.to_string();
        let params = $param_names.clone();
        $module
            .function($name, move |a: Value, b: Value| {
                let client = client.clone();
                let tool = tool.clone();
                let params = params.clone();
                async move {
                    let args = match build_args_json(&params, vec![a, b]) {
                        Ok(args) => args,
                        Err(e) => return VmResult::err(e),
                    };
                    call_mcp_tool_async(&client, &tool, args).await
                }
            })
            .build()?;
    }};

    // 3 arguments
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 3, $param_names:expr) => {{
        let client = $client.clone();
        let tool = $tool_name.to_string();
        let params = $param_names.clone();
        $module
            .function($name, move |a: Value, b: Value, c: Value| {
                let client = client.clone();
                let tool = tool.clone();
                let params = params.clone();
                async move {
                    let args = match build_args_json(&params, vec![a, b, c]) {
                        Ok(args) => args,
                        Err(e) => return VmResult::err(e),
                    };
                    call_mcp_tool_async(&client, &tool, args).await
                }
            })
            .build()?;
    }};

    // 4 arguments
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 4, $param_names:expr) => {{
        let client = $client.clone();
        let tool = $tool_name.to_string();
        let params = $param_names.clone();
        $module
            .function($name, move |a: Value, b: Value, c: Value, d: Value| {
                let client = client.clone();
                let tool = tool.clone();
                let params = params.clone();
                async move {
                    let args = match build_args_json(&params, vec![a, b, c, d]) {
                        Ok(args) => args,
                        Err(e) => return VmResult::err(e),
                    };
                    call_mcp_tool_async(&client, &tool, args).await
                }
            })
            .build()?;
    }};

    // 5 arguments
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 5, $param_names:expr) => {{
        let client = $client.clone();
        let tool = $tool_name.to_string();
        let params = $param_names.clone();
        $module
            .function(
                $name,
                move |a: Value, b: Value, c: Value, d: Value, e: Value| {
                    let client = client.clone();
                    let tool = tool.clone();
                    let params = params.clone();
                    async move {
                        let args = match build_args_json(&params, vec![a, b, c, d, e]) {
                            Ok(args) => args,
                            Err(e) => return VmResult::err(e),
                        };
                        call_mcp_tool_async(&client, &tool, args).await
                    }
                },
            )
            .build()?;
    }};

}

// =============================================================================
// MCP Tool Calling
// =============================================================================

/// Trait for MCP clients that can call tools
///
/// This abstraction allows mocking in tests.
pub trait McpToolCaller: Send + Sync {
    /// Call a tool by name with JSON arguments
    fn call_tool(
        &self,
        tool_name: &str,
        args: JsonValue,
    ) -> impl std::future::Future<Output = Result<ToolCallResult, String>> + Send;
}

/// Implementation for the real UpstreamMcpClient
impl McpToolCaller for UpstreamMcpClient {
    async fn call_tool(&self, tool_name: &str, args: JsonValue) -> Result<ToolCallResult, String> {
        self.call_tool_with_events(tool_name, args)
            .await
            .map_err(|e| e.to_string())
    }
}

/// Implementation for Arc-wrapped clients (for sharing across closures)
impl<T: McpToolCaller> McpToolCaller for Arc<T> {
    async fn call_tool(&self, tool_name: &str, args: JsonValue) -> Result<ToolCallResult, String> {
        (**self).call_tool(tool_name, args).await
    }
}

/// Async wrapper for calling MCP tools from Rune
async fn call_mcp_tool_async<C: McpToolCaller>(
    client: &C,
    tool_name: &str,
    args: JsonValue,
) -> VmResult<McpResult> {
    match client.call_tool(tool_name, args).await {
        Ok(result) => VmResult::Ok(McpResult::from(result)),
        Err(e) => VmResult::err(VmError::panic(format!("MCP tool error: {}", e))),
    }
}

// =============================================================================
// Schema Parsing
// =============================================================================

/// Extract parameter names from a JSON Schema in deterministic order.
///
/// Returns required parameters first (in schema order), then optional parameters.
/// This order determines the positional argument order in generated functions.
///
/// # Example Schema
/// ```json
/// {
///   "type": "object",
///   "properties": {
///     "query": { "type": "string" },
///     "page": { "type": "integer" },
///     "per_page": { "type": "integer" }
///   },
///   "required": ["query"]
/// }
/// ```
///
/// Returns: `["query", "page", "per_page"]`
pub fn extract_param_names(schema: &JsonValue) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // First: required parameters (preserves order from schema)
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for r in required {
            if let Some(name) = r.as_str() {
                if seen.insert(name.to_string()) {
                    names.push(name.to_string());
                }
            }
        }
    }

    // Then: optional parameters (from properties, alphabetically for determinism)
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        let mut optional: Vec<_> = props
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        optional.sort(); // Deterministic order for optional params

        for key in optional {
            names.push(key);
        }
    }

    names
}

/// Extract parameter types from a JSON Schema (for documentation/validation)
pub fn extract_param_types(schema: &JsonValue) -> Vec<(String, String)> {
    let mut params = Vec::new();

    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (name, prop_schema) in props {
            let type_name = prop_schema
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("any")
                .to_string();
            params.push((name.clone(), type_name));
        }
    }

    params
}

// =============================================================================
// Module Generation
// =============================================================================

/// Generate a Rune module for an MCP server's tools.
///
/// Creates a module at `cru::mcp::<server_name>` with functions for each tool.
/// Function signatures match the tool's input schema parameters.
///
/// # Arguments
/// * `server_name` - Name of the MCP server (becomes module path segment)
/// * `tools` - List of tools discovered from the MCP server
/// * `client` - Client for calling tools (wrapped in Arc for sharing)
///
/// # Returns
/// A Rune Module that can be installed into a Context
///
/// # Example
/// ```rust,ignore
/// let tools = client.list_tools().await?;
/// let module = generate_mcp_server_module("github", &tools, client)?;
/// context.install(module)?;
/// ```
pub fn generate_mcp_server_module<C: McpToolCaller + 'static>(
    server_name: &str,
    tools: &[UpstreamTool],
    client: Arc<C>,
) -> Result<Module, ContextError> {
    let mut module = Module::with_crate_item("cru", ["mcp", server_name])?;

    for tool in tools {
        let param_names = extract_param_names(&tool.input_schema);
        let arity = param_names.len();

        debug!(
            "Registering MCP tool: {}::{} with {} params: {:?}",
            server_name, tool.name, arity, param_names
        );

        // Dispatch to appropriate macro based on arity
        // Note: Rune 0.14 only supports up to 5 arguments for Module::function()
        match arity {
            0 => register_mcp_tool!(module, [&*tool.name], client, &tool.name, 0, param_names),
            1 => register_mcp_tool!(module, [&*tool.name], client, &tool.name, 1, param_names),
            2 => register_mcp_tool!(module, [&*tool.name], client, &tool.name, 2, param_names),
            3 => register_mcp_tool!(module, [&*tool.name], client, &tool.name, 3, param_names),
            4 => register_mcp_tool!(module, [&*tool.name], client, &tool.name, 4, param_names),
            5 => register_mcp_tool!(module, [&*tool.name], client, &tool.name, 5, param_names),
            _ => {
                warn!(
                    "Tool {} has {} parameters, max supported is 5 (Rune 0.14 limit). Skipping.",
                    tool.name, arity
                );
                continue;
            }
        }
    }

    Ok(module)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_param_names_required_first() {
        let schema = json!({
            "type": "object",
            "properties": {
                "optional1": { "type": "string" },
                "required1": { "type": "string" },
                "optional2": { "type": "integer" }
            },
            "required": ["required1"]
        });

        let names = extract_param_names(&schema);

        // Required first, then optional alphabetically
        assert_eq!(names[0], "required1");
        assert!(names.contains(&"optional1".to_string()));
        assert!(names.contains(&"optional2".to_string()));
    }

    #[test]
    fn test_extract_param_names_multiple_required() {
        let schema = json!({
            "type": "object",
            "properties": {
                "c": { "type": "string" },
                "a": { "type": "string" },
                "b": { "type": "string" }
            },
            "required": ["b", "a"]
        });

        let names = extract_param_names(&schema);

        // Required in schema order: b, a
        // Then optional: c
        assert_eq!(names, vec!["b", "a", "c"]);
    }

    #[test]
    fn test_extract_param_names_no_required() {
        let schema = json!({
            "type": "object",
            "properties": {
                "zebra": { "type": "string" },
                "apple": { "type": "string" }
            }
        });

        let names = extract_param_names(&schema);

        // Alphabetical when no required
        assert_eq!(names, vec!["apple", "zebra"]);
    }

    #[test]
    fn test_extract_param_names_empty() {
        let schema = json!({
            "type": "object"
        });

        let names = extract_param_names(&schema);
        assert!(names.is_empty());
    }

    #[test]
    fn test_extract_param_types() {
        let schema = json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "page": { "type": "integer" },
                "flag": { "type": "boolean" }
            }
        });

        let types = extract_param_types(&schema);

        assert!(types.contains(&("query".to_string(), "string".to_string())));
        assert!(types.contains(&("page".to_string(), "integer".to_string())));
        assert!(types.contains(&("flag".to_string(), "boolean".to_string())));
    }

    // Mock client for testing module generation
    struct MockMcpClient {
        responses: std::collections::HashMap<String, ToolCallResult>,
    }

    impl MockMcpClient {
        fn new() -> Self {
            Self {
                responses: std::collections::HashMap::new(),
            }
        }

        fn with_response(mut self, tool: &str, result: ToolCallResult) -> Self {
            self.responses.insert(tool.to_string(), result);
            self
        }
    }

    impl McpToolCaller for MockMcpClient {
        async fn call_tool(
            &self,
            tool_name: &str,
            _args: JsonValue,
        ) -> Result<ToolCallResult, String> {
            self.responses
                .get(tool_name)
                .cloned()
                .ok_or_else(|| format!("Unknown tool: {}", tool_name))
        }
    }

    #[test]
    fn test_generate_module_empty_tools() {
        let client = Arc::new(MockMcpClient::new());
        let tools: Vec<UpstreamTool> = vec![];

        let result = generate_mcp_server_module("test", &tools, client);
        assert!(result.is_ok());
    }

    #[test]
    fn test_generate_module_with_tools() {
        use crate::mcp_gateway::ContentBlock;

        let client = Arc::new(
            MockMcpClient::new().with_response(
                "echo",
                ToolCallResult {
                    content: vec![ContentBlock::Text {
                        text: "echoed".to_string(),
                    }],
                    is_error: false,
                },
            ),
        );

        let tools = vec![UpstreamTool {
            name: "echo".to_string(),
            prefixed_name: "test_echo".to_string(),
            description: Some("Echo a message".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            }),
            upstream: "test".to_string(),
        }];

        let result = generate_mcp_server_module("test", &tools, client);
        assert!(result.is_ok());
    }
}
