//! MCP tool calling for Lua
//!
//! Allows Lua scripts to invoke MCP server tools.
//!
//! # Usage
//!
//! ```lua
//! local mcp = require("mcp")
//!
//! -- List available tools from a server
//! local tools = mcp.list_tools("github")
//! for _, tool in ipairs(tools) do
//!     print(tool.name, tool.description)
//! end
//!
//! -- Call a tool with arguments
//! local result = mcp.call("github", "search_repos", {query = "rust", limit = 10})
//! if result.success then
//!     print(result.content)
//! else
//!     print("Error:", result.error)
//! end
//!
//! -- List available servers
//! local servers = mcp.servers()
//! for _, name in ipairs(servers) do
//!     print(name)
//! end
//! ```
//!
//! # Architecture
//!
//! Unlike Rune which requires macros for different function arities, Lua can
//! simply accept a table of arguments. This makes the implementation simpler:
//!
//! ```text
//! Lua script
//!     │
//!     ▼
//! mcp.call(server, tool, {args...})
//!     │
//!     ├─► Convert Lua table to JSON
//!     ▼
//! LuaMcpClient.call_tool(server, tool, json_args)
//!     │
//!     ▼
//! MCP Server
//! ```

use crate::error::LuaError;
use crate::json_query::{json_to_lua, lua_to_json};
use mlua::{Lua, Result as LuaResult, Table, Value};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tracing::debug;

// =============================================================================
// Lua MCP Client Trait
// =============================================================================

/// Trait for MCP client that Lua can use
///
/// Implementations should handle:
/// - Connection management to multiple MCP servers
/// - Tool discovery from connected servers
/// - Tool invocation with JSON arguments
pub trait LuaMcpClient: Send + Sync {
    /// List available tools for a server
    ///
    /// Returns information about all tools available on the specified server.
    fn list_tools(&self, server: &str) -> Vec<McpToolInfo>;

    /// Call a tool and get result
    ///
    /// # Arguments
    /// * `server` - Name of the MCP server
    /// * `tool` - Name of the tool to call
    /// * `args` - JSON object with tool arguments
    ///
    /// # Returns
    /// Result containing success status and either content or error message
    fn call_tool(&self, server: &str, tool: &str, args: JsonValue) -> Result<McpToolResult, String>;

    /// List available server names
    ///
    /// Returns names of all connected MCP servers.
    fn list_servers(&self) -> Vec<String> {
        // Default implementation returns empty list
        vec![]
    }
}

// =============================================================================
// MCP Types
// =============================================================================

/// Info about an MCP tool
#[derive(Clone, Debug)]
pub struct McpToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for input parameters (optional)
    pub input_schema: Option<JsonValue>,
}

/// Result from calling an MCP tool
#[derive(Clone, Debug)]
pub struct McpToolResult {
    /// Whether the call succeeded
    pub success: bool,
    /// Content returned by the tool (if successful)
    pub content: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
}

impl McpToolResult {
    /// Create a successful result
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            success: true,
            content: Some(content.into()),
            error: None,
        }
    }

    /// Create an error result
    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            content: None,
            error: Some(error.into()),
        }
    }
}

// =============================================================================
// Module Registration
// =============================================================================

/// Register MCP module with a client
///
/// Creates a global `mcp` module with the following functions:
///
/// - `mcp.list_tools(server)` - List tools for a server
/// - `mcp.call(server, tool, args)` - Call a tool
/// - `mcp.servers()` - List available servers
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lua::mcp::{register_mcp_module, LuaMcpClient, McpToolInfo, McpToolResult};
/// use std::sync::Arc;
///
/// struct MyClient;
/// impl LuaMcpClient for MyClient {
///     fn list_tools(&self, server: &str) -> Vec<McpToolInfo> { vec![] }
///     fn call_tool(&self, server: &str, tool: &str, args: JsonValue) -> Result<McpToolResult, String> {
///         Ok(McpToolResult::ok("result"))
///     }
/// }
///
/// let lua = mlua::Lua::new();
/// register_mcp_module(&lua, Arc::new(MyClient))?;
/// ```
pub fn register_mcp_module<C: LuaMcpClient + 'static>(lua: &Lua, client: Arc<C>) -> LuaResult<()> {
    let mcp = lua.create_table()?;

    // -------------------------------------------------------------------------
    // mcp.list_tools(server) -> [{name, description, input_schema}]
    // -------------------------------------------------------------------------
    let c = client.clone();
    mcp.set(
        "list_tools",
        lua.create_function(move |lua, server: String| {
            debug!(server = %server, "mcp.list_tools called");

            let tools = c.list_tools(&server);
            let result = lua.create_table()?;

            for (i, tool) in tools.iter().enumerate() {
                let t = lua.create_table()?;
                t.set("name", tool.name.clone())?;
                t.set("description", tool.description.clone())?;

                // Include input_schema if available
                if let Some(schema) = &tool.input_schema {
                    let lua_schema = json_to_lua(lua, schema.clone())?;
                    t.set("input_schema", lua_schema)?;
                }

                result.set(i + 1, t)?;
            }

            Ok(result)
        })?,
    )?;

    // -------------------------------------------------------------------------
    // mcp.call(server, tool, args) -> {success, content, error}
    // -------------------------------------------------------------------------
    let c = client.clone();
    mcp.set(
        "call",
        lua.create_function(move |lua, (server, tool, args): (String, String, Table)| {
            debug!(server = %server, tool = %tool, "mcp.call invoked");

            // Convert Lua table to JSON
            let json_args = lua_to_json(lua, Value::Table(args)).map_err(mlua::Error::external)?;

            let result_table = lua.create_table()?;

            match c.call_tool(&server, &tool, json_args) {
                Ok(result) => {
                    result_table.set("success", result.success)?;
                    if let Some(content) = result.content {
                        result_table.set("content", content)?;
                    }
                    if let Some(error) = result.error {
                        result_table.set("error", error)?;
                    }
                }
                Err(e) => {
                    result_table.set("success", false)?;
                    result_table.set("error", e)?;
                }
            }

            Ok(Value::Table(result_table))
        })?,
    )?;

    // -------------------------------------------------------------------------
    // mcp.call_json(server, tool, args) -> {success, content, error}
    // Alternative that takes JSON string instead of table
    // -------------------------------------------------------------------------
    let c = client.clone();
    mcp.set(
        "call_json",
        lua.create_function(move |lua, (server, tool, args_json): (String, String, String)| {
            debug!(server = %server, tool = %tool, "mcp.call_json invoked");

            // Parse JSON string to JSON value
            let json_args: JsonValue =
                serde_json::from_str(&args_json).map_err(mlua::Error::external)?;

            let result_table = lua.create_table()?;

            match c.call_tool(&server, &tool, json_args) {
                Ok(result) => {
                    result_table.set("success", result.success)?;
                    if let Some(content) = result.content {
                        result_table.set("content", content)?;
                    }
                    if let Some(error) = result.error {
                        result_table.set("error", error)?;
                    }
                }
                Err(e) => {
                    result_table.set("success", false)?;
                    result_table.set("error", e)?;
                }
            }

            Ok(Value::Table(result_table))
        })?,
    )?;

    // -------------------------------------------------------------------------
    // mcp.servers() -> list of available server names
    // -------------------------------------------------------------------------
    let c = client.clone();
    mcp.set(
        "servers",
        lua.create_function(move |lua, ()| {
            debug!("mcp.servers called");

            let servers = c.list_servers();
            let result = lua.create_table()?;

            for (i, name) in servers.iter().enumerate() {
                result.set(i + 1, name.clone())?;
            }

            Ok(result)
        })?,
    )?;

    // -------------------------------------------------------------------------
    // mcp.has_tool(server, tool) -> boolean
    // Convenience function to check if a tool exists
    // -------------------------------------------------------------------------
    let c = client.clone();
    mcp.set(
        "has_tool",
        lua.create_function(move |_, (server, tool_name): (String, String)| {
            let tools = c.list_tools(&server);
            let has_it = tools.iter().any(|t| t.name == tool_name);
            Ok(has_it)
        })?,
    )?;

    // Register globally
    lua.globals().set("mcp", mcp)?;

    Ok(())
}

/// Register MCP module without a client (stub for testing)
///
/// Creates a global `mcp` module with stub functions that return empty results.
/// Useful for testing Lua scripts that reference MCP but don't need real connections.
pub fn register_mcp_module_stub(lua: &Lua) -> Result<(), LuaError> {
    let mcp = lua.create_table()?;

    // Stub list_tools - returns empty array
    mcp.set(
        "list_tools",
        lua.create_function(|lua, _server: String| lua.create_table())?,
    )?;

    // Stub call - returns error
    mcp.set(
        "call",
        lua.create_function(|lua, (_server, _tool, _args): (String, String, Table)| {
            let result = lua.create_table()?;
            result.set("success", false)?;
            result.set("error", "MCP not configured (stub mode)")?;
            Ok(Value::Table(result))
        })?,
    )?;

    // Stub call_json - returns error
    mcp.set(
        "call_json",
        lua.create_function(|lua, (_server, _tool, _args): (String, String, String)| {
            let result = lua.create_table()?;
            result.set("success", false)?;
            result.set("error", "MCP not configured (stub mode)")?;
            Ok(Value::Table(result))
        })?,
    )?;

    // Stub servers - returns empty array
    mcp.set(
        "servers",
        lua.create_function(|lua, ()| lua.create_table())?,
    )?;

    // Stub has_tool - returns false
    mcp.set(
        "has_tool",
        lua.create_function(|_, (_server, _tool): (String, String)| Ok(false))?,
    )?;

    lua.globals().set("mcp", mcp)?;

    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::RwLock;

    /// Mock MCP client for testing
    struct MockMcpClient {
        tools: RwLock<HashMap<String, Vec<McpToolInfo>>>,
        responses: RwLock<HashMap<String, McpToolResult>>,
    }

    impl MockMcpClient {
        fn new() -> Self {
            Self {
                tools: RwLock::new(HashMap::new()),
                responses: RwLock::new(HashMap::new()),
            }
        }

        fn add_tool(&self, server: &str, tool: McpToolInfo) {
            let mut tools = self.tools.write().unwrap();
            tools.entry(server.to_string()).or_default().push(tool);
        }

        fn add_response(&self, key: &str, response: McpToolResult) {
            let mut responses = self.responses.write().unwrap();
            responses.insert(key.to_string(), response);
        }
    }

    impl LuaMcpClient for MockMcpClient {
        fn list_tools(&self, server: &str) -> Vec<McpToolInfo> {
            let tools = self.tools.read().unwrap();
            tools.get(server).cloned().unwrap_or_default()
        }

        fn call_tool(
            &self,
            server: &str,
            tool: &str,
            _args: JsonValue,
        ) -> Result<McpToolResult, String> {
            let key = format!("{}:{}", server, tool);
            let responses = self.responses.read().unwrap();

            responses
                .get(&key)
                .cloned()
                .ok_or_else(|| format!("Unknown tool: {}", key))
        }

        fn list_servers(&self) -> Vec<String> {
            let tools = self.tools.read().unwrap();
            tools.keys().cloned().collect()
        }
    }

    fn setup_lua() -> Lua {
        Lua::new()
    }

    #[test]
    fn test_register_mcp_module() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());

        let result = register_mcp_module(&lua, client);
        assert!(result.is_ok());

        // Verify mcp global exists
        let mcp: Table = lua.globals().get("mcp").unwrap();
        assert!(mcp.contains_key("list_tools").unwrap());
        assert!(mcp.contains_key("call").unwrap());
        assert!(mcp.contains_key("servers").unwrap());
    }

    #[test]
    fn test_list_tools_empty() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());
        register_mcp_module(&lua, client).unwrap();

        let result: Table = lua
            .load(r#"return mcp.list_tools("github")"#)
            .eval()
            .unwrap();

        // Empty table (no tools registered)
        assert_eq!(result.raw_len(), 0);
    }

    #[test]
    fn test_list_tools_with_tools() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());

        client.add_tool(
            "github",
            McpToolInfo {
                name: "search_repos".to_string(),
                description: "Search GitHub repositories".to_string(),
                input_schema: None,
            },
        );

        client.add_tool(
            "github",
            McpToolInfo {
                name: "get_user".to_string(),
                description: "Get user info".to_string(),
                input_schema: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "username": { "type": "string" }
                    }
                })),
            },
        );

        register_mcp_module(&lua, client).unwrap();

        let result: Table = lua
            .load(r#"return mcp.list_tools("github")"#)
            .eval()
            .unwrap();

        assert_eq!(result.raw_len(), 2);

        // Check first tool
        let tool1: Table = result.get(1).unwrap();
        assert_eq!(tool1.get::<String>("name").unwrap(), "search_repos");
        assert_eq!(
            tool1.get::<String>("description").unwrap(),
            "Search GitHub repositories"
        );
    }

    #[test]
    fn test_call_tool_success() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());

        client.add_tool(
            "test",
            McpToolInfo {
                name: "echo".to_string(),
                description: "Echo back".to_string(),
                input_schema: None,
            },
        );

        client.add_response("test:echo", McpToolResult::ok("Hello, World!"));

        register_mcp_module(&lua, client).unwrap();

        let result: Table = lua
            .load(r#"return mcp.call("test", "echo", { message = "test" })"#)
            .eval()
            .unwrap();

        assert!(result.get::<bool>("success").unwrap());
        assert_eq!(
            result.get::<String>("content").unwrap(),
            "Hello, World!"
        );
    }

    #[test]
    fn test_call_tool_error() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());

        client.add_tool(
            "test",
            McpToolInfo {
                name: "fail".to_string(),
                description: "Always fails".to_string(),
                input_schema: None,
            },
        );

        client.add_response("test:fail", McpToolResult::err("Something went wrong"));

        register_mcp_module(&lua, client).unwrap();

        let result: Table = lua
            .load(r#"return mcp.call("test", "fail", {})"#)
            .eval()
            .unwrap();

        assert!(!result.get::<bool>("success").unwrap());
        assert_eq!(
            result.get::<String>("error").unwrap(),
            "Something went wrong"
        );
    }

    #[test]
    fn test_call_tool_unknown() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());
        register_mcp_module(&lua, client).unwrap();

        let result: Table = lua
            .load(r#"return mcp.call("unknown", "tool", {})"#)
            .eval()
            .unwrap();

        assert!(!result.get::<bool>("success").unwrap());
        assert!(result.get::<String>("error").unwrap().contains("Unknown"));
    }

    #[test]
    fn test_servers() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());

        client.add_tool(
            "github",
            McpToolInfo {
                name: "test".to_string(),
                description: "Test".to_string(),
                input_schema: None,
            },
        );

        client.add_tool(
            "filesystem",
            McpToolInfo {
                name: "read".to_string(),
                description: "Read".to_string(),
                input_schema: None,
            },
        );

        register_mcp_module(&lua, client).unwrap();

        let servers: Table = lua.load(r#"return mcp.servers()"#).eval().unwrap();

        // Should have 2 servers (order not guaranteed)
        assert_eq!(servers.raw_len(), 2);
    }

    #[test]
    fn test_has_tool() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());

        client.add_tool(
            "test",
            McpToolInfo {
                name: "existing".to_string(),
                description: "Exists".to_string(),
                input_schema: None,
            },
        );

        register_mcp_module(&lua, client).unwrap();

        let has_existing: bool = lua
            .load(r#"return mcp.has_tool("test", "existing")"#)
            .eval()
            .unwrap();
        assert!(has_existing);

        let has_missing: bool = lua
            .load(r#"return mcp.has_tool("test", "missing")"#)
            .eval()
            .unwrap();
        assert!(!has_missing);
    }

    #[test]
    fn test_call_json() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());

        client.add_tool(
            "test",
            McpToolInfo {
                name: "json_echo".to_string(),
                description: "Echo JSON".to_string(),
                input_schema: None,
            },
        );

        client.add_response("test:json_echo", McpToolResult::ok("JSON received"));

        register_mcp_module(&lua, client).unwrap();

        let result: Table = lua
            .load(r#"return mcp.call_json("test", "json_echo", '{"key": "value"}')"#)
            .eval()
            .unwrap();

        assert!(result.get::<bool>("success").unwrap());
        assert_eq!(result.get::<String>("content").unwrap(), "JSON received");
    }

    #[test]
    fn test_mcp_stub() {
        let lua = setup_lua();
        register_mcp_module_stub(&lua).unwrap();

        // All operations should work but return empty/error results
        let tools: Table = lua
            .load(r#"return mcp.list_tools("any")"#)
            .eval()
            .unwrap();
        assert_eq!(tools.raw_len(), 0);

        let servers: Table = lua.load(r#"return mcp.servers()"#).eval().unwrap();
        assert_eq!(servers.raw_len(), 0);

        let has: bool = lua
            .load(r#"return mcp.has_tool("any", "tool")"#)
            .eval()
            .unwrap();
        assert!(!has);

        let result: Table = lua
            .load(r#"return mcp.call("any", "tool", {})"#)
            .eval()
            .unwrap();
        assert!(!result.get::<bool>("success").unwrap());
        assert!(result
            .get::<String>("error")
            .unwrap()
            .contains("stub mode"));
    }

    #[test]
    fn test_lua_integration_script() {
        let lua = setup_lua();
        let client = Arc::new(MockMcpClient::new());

        client.add_tool(
            "api",
            McpToolInfo {
                name: "fetch".to_string(),
                description: "Fetch data".to_string(),
                input_schema: None,
            },
        );

        client.add_response("api:fetch", McpToolResult::ok(r#"{"status": "ok"}"#));

        register_mcp_module(&lua, client).unwrap();

        // Run a more complex Lua script
        let result: bool = lua
            .load(
                r#"
                -- Check if tool exists
                if not mcp.has_tool("api", "fetch") then
                    return false
                end

                -- Call the tool
                local result = mcp.call("api", "fetch", { url = "https://example.com" })

                -- Verify result
                return result.success and result.content ~= nil
            "#,
            )
            .eval()
            .unwrap();

        assert!(result);
    }
}
