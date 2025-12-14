# Rune MCP Tool Integration: Implementation Plan

## Executive Summary

Enable Rune scripts to call MCP tools via dynamically-generated functions that mirror the MCP tool schemas. When an MCP server connects, its tools become available as Rune functions like `cru::mcp::github::search_repositories(args)`.

## Current Architecture

### crucible-rune (existing)

```
crucible-rune/
├── mcp_gateway.rs     # MCP client connection, tool discovery
├── executor.rs        # Rune VM execution
├── rune_types.rs      # Rune module with typed bindings
├── registry.rs        # Tool discovery from .rn files
├── event_bus.rs       # Event system for hooks
└── hook_system.rs     # Hook registration and execution
```

**Key Types:**
- `UpstreamMcpClient` - Connects to MCP servers, discovers tools
- `UpstreamTool` - Tool metadata including `input_schema: JsonValue`
- `ToolCallResult` - Result with `content: Vec<ContentBlock>`, `is_error: bool`
- `RuneExecutor` - Compiles and runs Rune scripts
- `Module` - Rune module with registered functions

**Rune Version:** 0.14 (supports `Module::function()` for dynamic registration)

### MCP Gateway Flow

```
Connect to MCP server
       ↓
List tools → Vec<UpstreamTool>
       ↓
Each tool has:
  - name: "search_repositories"
  - description: "Search GitHub repos"
  - input_schema: { type: "object", properties: {...} }
       ↓
Call tool → ToolCallResult { content, is_error }
```

## Design Goals

1. **Dynamic function generation** from MCP schemas at runtime
2. **Rust-like API**: `cru::mcp::github::search_repositories(args)`
3. **Type-safe results** via `McpResult` wrapper
4. **Serial execution** (no parallel tool calls initially)
5. **No schema validation** initially (opt-in later)
6. **No caching** (tools can affect world state)

## Proposed Architecture

### Module Structure

```
cru::mcp::<server>::<tool_name>(arg1, arg2, ...) -> McpResult

Example:
  cru::mcp::github::search_repositories("rust", 1, 30)
  cru::mcp::filesystem::read_file("/path/to/file")
```

### Result Type

```
McpResult
├── text() -> Option<String>
├── json() -> Result<Value, Error>
├── structured() -> Option<Value>
├── is_error() -> bool
└── all_text() -> String
```

### Dynamic Module Generation with Macro-Generated Arities

The challenge: Rune's `Module::function()` requires concrete closure types at compile time,
but MCP schemas have variable numbers of parameters determined at runtime.

**Solution**: Macro-generate functions for arities 0-10, dispatch based on schema.

```rust
// In mcp_module.rs (new file)

/// Macro to generate function registrations for different arities
macro_rules! register_mcp_tool {
    // 0 args
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 0, $param_names:expr) => {
        let client = $client.clone();
        let tool = $tool_name.to_string();
        $module.function($name, move || {
            let client = client.clone();
            let tool = tool.clone();
            async move {
                let args = serde_json::json!({});
                call_mcp_tool(&client, &tool, args).await
            }
        })?;
    };
    // 1 arg
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 1, $param_names:expr) => {
        let client = $client.clone();
        let tool = $tool_name.to_string();
        let params = $param_names.clone();
        $module.function($name, move |a: Value| {
            let client = client.clone();
            let tool = tool.clone();
            let params = params.clone();
            async move {
                let args = build_args_json(&params, vec![a])?;
                call_mcp_tool(&client, &tool, args).await
            }
        })?;
    };
    // 2 args
    ($module:expr, $name:expr, $client:expr, $tool_name:expr, 2, $param_names:expr) => {
        let client = $client.clone();
        let tool = $tool_name.to_string();
        let params = $param_names.clone();
        $module.function($name, move |a: Value, b: Value| {
            let client = client.clone();
            let tool = tool.clone();
            let params = params.clone();
            async move {
                let args = build_args_json(&params, vec![a, b])?;
                call_mcp_tool(&client, &tool, args).await
            }
        })?;
    };
    // ... continue for 3-10 args
}

/// Build JSON args object from positional Rune values
fn build_args_json(param_names: &[String], values: Vec<Value>) -> Result<JsonValue, VmError> {
    let mut obj = serde_json::Map::new();
    for (name, value) in param_names.iter().zip(values.iter()) {
        obj.insert(name.clone(), rune_to_json(value)?);
    }
    Ok(JsonValue::Object(obj))
}

/// Call MCP tool and wrap result
async fn call_mcp_tool(
    client: &UpstreamMcpClient,
    tool_name: &str,
    args: JsonValue,
) -> Result<McpResult, VmError> {
    let result = client.call_tool(tool_name, args).await
        .map_err(|e| VmError::panic(format!("MCP error: {}", e)))?;
    Ok(McpResult::from(result))
}

/// Generate a Rune module for an MCP server's tools
pub fn generate_mcp_server_module(
    server_name: &str,
    tools: &[UpstreamTool],
    client: Arc<UpstreamMcpClient>,
) -> Result<Module, ContextError> {
    // Create module: cru::mcp::<server_name>
    let mut module = Module::with_crate_item("cru", ["mcp", server_name])?;

    for tool in tools {
        // Extract ordered parameter names from schema
        let param_names = extract_param_names(&tool.input_schema);
        let arity = param_names.len();

        // Dispatch to appropriate macro based on arity
        match arity {
            0 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 0, param_names),
            1 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 1, param_names),
            2 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 2, param_names),
            3 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 3, param_names),
            4 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 4, param_names),
            5 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 5, param_names),
            6 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 6, param_names),
            7 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 7, param_names),
            8 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 8, param_names),
            9 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 9, param_names),
            10 => register_mcp_tool!(module, [&tool.name], client, &tool.name, 10, param_names),
            _ => {
                tracing::warn!("Tool {} has {} params, max supported is 10", tool.name, arity);
                continue;
            }
        }
    }

    Ok(module)
}

/// Extract parameter names from JSON schema in deterministic order
fn extract_param_names(schema: &JsonValue) -> Vec<String> {
    // Get required params first (in order), then optional params
    let mut names = Vec::new();

    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for r in required {
            if let Some(name) = r.as_str() {
                names.push(name.to_string());
            }
        }
    }

    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for key in props.keys() {
            if !names.contains(key) {
                names.push(key.clone());
            }
        }
    }

    names
}
```

### McpResult Implementation

```rust
// In mcp_types.rs (new file)

use rune::Any;

/// Result wrapper for MCP tool calls
#[derive(Debug, Clone, Any)]
#[rune(item = ::cru::mcp)]
pub struct McpResult {
    content: Vec<ContentBlock>,
    structured: Option<JsonValue>,
    is_error: bool,
}

impl McpResult {
    /// Get first text content
    #[rune::function]
    pub fn text(&self) -> Option<String> {
        self.content.iter().find_map(|c| match c {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        })
    }

    /// Try to parse content as JSON
    #[rune::function]
    pub fn json(&self) -> Result<Value, VmError> {
        // Try structured first
        if let Some(s) = &self.structured {
            return json_to_rune(s);
        }
        // Try parsing text
        if let Some(text) = self.text() {
            let parsed: JsonValue = serde_json::from_str(&text)
                .map_err(|e| VmError::panic(format!("JSON parse error: {}", e)))?;
            return json_to_rune(&parsed);
        }
        Err(VmError::panic("No JSON content available"))
    }

    /// Get structured content if server provided it
    #[rune::function]
    pub fn structured(&self) -> Option<Value> {
        self.structured.as_ref().and_then(|s| json_to_rune(s).ok())
    }

    /// Check if tool returned error
    #[rune::function]
    pub fn is_error(&self) -> bool {
        self.is_error
    }

    /// Get all text content joined
    #[rune::function]
    pub fn all_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
```

### Usage Examples

```rune
// Basic usage - positional args match schema order
use cru::mcp::github;
use cru::mcp::filesystem;

pub async fn search_github(query) {
    // search_repositories(query: String, page: i64, per_page: i64)
    let result = github::search_repositories(query, 1, 30).await?;

    // Parse as JSON
    let data = result.json()?;
    data.items
}

// With error handling
pub async fn safe_read(path) {
    // read_file(path: String)
    let result = filesystem::read_file(path).await?;

    if result.is_error() {
        return Err(`Failed to read: ${path}`);
    }

    Ok(result.text()?)
}

// Chaining tools from different servers
pub async fn find_todos_in_repo(repo_url) {
    // Clone repo - clone_repository(url: String, path: String)
    let clone_result = github::clone_repository(repo_url, "/tmp/repo").await?;

    // Search for TODOs - search_files(path: String, pattern: String, glob: String)
    let search_result = filesystem::search_files("/tmp/repo", "TODO", "**/*.rs").await?;

    search_result.json()?
}

// Introspection (future)
pub fn list_github_tools() {
    for tool in cru::mcp::list_tools("github") {
        println(`${tool.name}: ${tool.description}`);
    }
}
```

## Implementation Phases

### Phase 1: Core Types (Week 1)

**Files to create:**
- `crates/crucible-rune/src/mcp_types.rs` - McpResult, ToolInfo types
- `crates/crucible-rune/src/mcp_module.rs` - Module generation

**Tasks:**
1. Create `McpResult` struct with Rune bindings
2. Create `ToolInfo` struct for tool metadata
3. Create `McpClient` struct wrapping `UpstreamMcpClient`
4. Implement JSON ↔ Rune value conversion helpers

**Tests:**
- Unit tests for McpResult methods
- Unit tests for value conversion

### Phase 2: Dynamic Registration (Week 2)

**Tasks:**
1. Implement `generate_mcp_server_module()` function
2. Hook into `UpstreamMcpClient::connect()` to trigger module generation
3. Create module registry to track generated modules
4. Implement `cru::mcp::connect()` function

**Tests:**
- Integration test: connect to mock MCP server
- Test dynamic function registration works
- Test function calls return McpResult

### Phase 3: Integration (Week 3)

**Tasks:**
1. Wire into existing RuneExecutor context
2. Update EventHandler to include MCP modules
3. Add reconnection handling (regenerate modules)
4. Error handling for disconnected servers

**Tests:**
- End-to-end test with real MCP server (filesystem)
- Test error cases (server disconnect, invalid tool)

### Phase 4: Polish (Week 4)

**Tasks:**
1. Add `list_servers()` and `list_tools()` introspection
2. Documentation and examples
3. Optional: Schema validation (opt-in)
4. Optional: Better error messages with schema hints

**Tests:**
- Full integration test suite
- Example scripts that work end-to-end

## File Structure (New)

```
crates/crucible-rune/src/
├── lib.rs                 # Add exports
├── mcp_module.rs          # NEW: Module generation
├── mcp_types.rs           # NEW: McpResult, ToolInfo, McpClient
├── mcp_gateway.rs         # MODIFY: Hook module generation
├── executor.rs            # MODIFY: Include MCP modules in context
└── ...
```

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_result_text() {
        let result = McpResult {
            content: vec![ContentBlock::Text { text: "hello".into() }],
            structured: None,
            is_error: false,
        };
        assert_eq!(result.text(), Some("hello".to_string()));
    }

    #[test]
    fn test_mcp_result_json() {
        let result = McpResult {
            content: vec![ContentBlock::Text {
                text: r#"{"key": "value"}"#.into()
            }],
            structured: None,
            is_error: false,
        };
        // Would need Rune VM context for full test
    }
}
```

### Integration Tests

```rust
// tests/mcp_integration.rs

#[tokio::test]
async fn test_mcp_module_generation() {
    // Create mock MCP server
    let mock_tools = vec![
        UpstreamTool {
            name: "test_tool".into(),
            prefixed_name: "mock_test_tool".into(),
            description: Some("A test tool".into()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "arg1": { "type": "string" }
                }
            }),
            upstream: "mock".into(),
        }
    ];

    // Generate module
    let client = Arc::new(MockMcpClient::new(mock_tools.clone()));
    let module = generate_mcp_server_module("mock", &mock_tools, client)?;

    // Verify function exists
    // ...
}
```

### End-to-End Rune Script Test

```rune
// tests/scripts/mcp_test.rn

#[test]
pub async fn test_mcp_connect() {
    let client = cru::mcp::connect("mock")?;
    assert!(client.name() == "mock");
}

#[test]
pub async fn test_mcp_call() {
    let client = cru::mcp::connect("mock")?;
    let result = client.test_tool(#{ arg1: "hello" })?;
    assert!(!result.is_error());
    assert!(result.text().is_some());
}
```

## Open Questions

1. **Module lifecycle**: When MCP server disconnects, should we:
   - Keep stale module (calls fail at runtime)?
   - Remove module (scripts fail to compile)?
   - Mark as disconnected (special error type)?

2. **Async context**: How to handle async MCP calls in Rune?
   - Rune supports async natively
   - Need to ensure executor properly awaits

3. **Type conversion edge cases**:
   - What if JSON has types Rune can't represent?
   - Binary data in images?
   - Large responses?

4. **Schema evolution**: If MCP server updates tools:
   - Regenerate modules on reconnect?
   - Version modules?

5. **Testing without MCP**: How to test scripts that use MCP?
   - Mock MCP servers?
   - Record/replay?

## Dependencies

**Existing (no changes):**
- rune = "0.14"
- rune-modules = "0.14"
- serde_json
- tokio (async runtime)

**No new dependencies needed.**

## Risks

1. **Rune 0.14 dynamic registration**: Need to verify `Module::function()` works as expected for async closures
2. **Performance**: Generating modules for large tool sets could be slow
3. **Memory**: Holding client references in closures
4. **Compatibility**: MCP spec changes could break assumptions

## Success Criteria

1. Scripts can call `cru::mcp::connect("server")` and get a client
2. Client has dynamically-generated methods matching MCP tools
3. Methods return `McpResult` with `.text()`, `.json()`, `.is_error()`
4. Error handling works (disconnected server, invalid args)
5. All tests pass
6. At least one example script works end-to-end

## Next Steps

1. Review this plan
2. Create branch `feat/rune-mcp-integration`
3. Implement Phase 1 (core types)
4. Write tests as we go
5. Iterate based on learnings
