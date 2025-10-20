# Crucible Migration Guide: MCP to Service Architecture

> **Status**: Active Migration Guide
> **Version**: 1.0.0
> **Date**: 2025-10-20
> **Purpose**: Guide users and developers transitioning from MCP-based architecture to the new service-oriented architecture

## Overview

This migration guide outlines the transition from the Model Context Protocol (MCP) server architecture to the new service-oriented architecture in Crucible. The new architecture replaces the MCP server with a more flexible, extensible service layer while maintaining compatibility where possible.

## Key Changes

### Architecture Overview

#### Before (MCP-based)
```
┌─────────────────────────────────────────────────────┐
│                      MCP Server                      │
├─────────────────────────────────────────────────────┤
│ Tools  │ Search │ Storage │ Agent │ Config          │
│ (Rune) │        │         │       │                 │
└─────────────────────────────────────────────────────┘
          ↓
┌─────────────────────────────────────────────────────┐
│                  Core System                         │
├─────────────────────────────────────────────────────┤
│ crucible-core │ crucible-daemon │ crucible-tauri     │
└─────────────────────────────────────────────────────┘
```

#### After (Service-oriented)
```
┌─────────────────────────────────────────────────────┐
│                   Service Layer                     │
├─────────────────────────────────────────────────────┤
│ Search │ Index │ Agent │ Tool │ HotReload          │
│        │       │       │      │                   │
├─────────────────────────────────────────────────────┤
│ Scripting Layer                                    │
├─────────────────────────────────────────────────────┤
│ crucible-rune │ crucible-tools │ crucible-rune-macros│
└─────────────────────────────────────────────────────┘
          ↓
┌─────────────────────────────────────────────────────┐
│                  Core System                         │
├─────────────────────────────────────────────────────┤
│ crucible-core │ crucible-daemon │ crucible-tauri     │
└─────────────────────────────────────────────────────┘
```

## Migration Steps

### 1. Update Dependencies

#### Remove MCP Dependencies
```toml
# Remove from Cargo.toml
[dependencies]
crucible-mcp = "0.1.0"  # REMOVE THIS
```

#### Add New Service Dependencies
```toml
# Add to Cargo.toml
[dependencies]
crucible-services = "0.1.0"
crucible-rune = "0.1.0"
crucible-tools = "0.1.0"
crucible-rune-macros = "0.1.0"
```

### 2. Update Configuration

#### Before (MCP Configuration)
```yaml
# config/mcp.yaml
mcp:
  host: "localhost"
  port: 3000
  tools:
    - name: "search"
      path: "./tools/search.rn"
    - name: "analyze"
      path: "./tools/analyze.rn"
```

#### After (Service Configuration)
```yaml
# config/services.yaml
services:
  search:
    enabled: true
    type: "crucible_services::SearchService"
    config:
      index_path: "./indexes"

  tools:
    static:
      - name: "search"
        module: "crucible_tools::search"
        function: "search_notes"
      - name: "metadata"
        module: "crucible_tools::metadata"
        function: "extract_metadata"

    dynamic:
      - name: "custom_search"
        path: "./tools/custom_search.rn"
        hot_reload: true

  agent:
    enabled: true
    type: "crucible_services::AgentService"
    config:
      tools: ["search", "metadata", "custom_search"]
```

### 3. Tool Migration

#### Before (MCP Tools)
```rust
// tools/search.rn
fn search(query: string) -> Result<Vec<Note>, Error> {
    // Search implementation
}
```

#### After (Service Tools)
##### Option 1: Static Tool (using crucible-tools)
```rust
// src/tools/search.rs
use crucible_tools::{Tool, ToolResult};

#[derive(Tool)]
#[tool(desc = "Search notes by query")]
pub fn search_notes(query: String, limit: Option<usize>) -> ToolResult<Vec<String>> {
    // Search implementation
    ToolResult::Success(note_ids)
}
```

##### Option 2: Dynamic Tool (using crucible-rune)
```rust
// tools/search.rn
pub fn search(query: string, limit: int?) -> Vec<Note> {
    // Search implementation with hot-reload support
    let search = crucible_services::get_search_service();
    search.find_notes(query, limit.unwrap_or(10))
}
```

##### Option 3: Macro-Generated Tool (using crucible-rune-macros)
```rust
// src/tools/search.rs
use crucible_rune_macros::rune_tool;

#[rune_tool(
    desc = "Search notes by query",
    category = "search",
    tags = ["notes", "query"]
)]
pub fn search_notes(query: String, limit: Option<usize>) -> Result<Vec<String>, String> {
    // Search implementation
    Ok(match limit {
        Some(n) => search_with_limit(&query, n),
        None => search_all(&query),
    })
}
```

### 4. Code Changes

#### Before (MCP Integration)
```rust
// In your application
let mcp_client = MCPClient::new("localhost:3000);
let tools = mcp_client.list_tools().await?;
let result = mcp_client.execute_tool("search", params).await?;
```

#### After (Service Integration)
```rust
// In your application
use crucible_services::ServiceRegistry;

let services = ServiceRegistry::new(config).await?;
let search_service = services.get_search_service();
let result = search_service.search(params).await?;

// Or use tools directly
let tools = services.list_available_tools().await?;
let result = services.execute_tool("search", params).await?;
```

### 5. REPL Updates

#### Before
```rust
// REPL command
:run search "my query"
```

#### After
```rust
// REPL commands remain the same, but backend changed
:run search "my query"
:tools  # Now shows both static and dynamic tools
```

## Feature Mapping

| MCP Feature | Service Architecture Equivalent | Notes |
|-------------|----------------------------------|-------|
| `list_tools()` | `services.list_available_tools()` | Combined static + dynamic tools |
| `execute_tool()` | `services.execute_tool()` | More efficient routing |
| `tool_metadata()` | `ToolRegistry::get_tool()` | Enhanced metadata support |
| `hot_reload()` | `crucible_rune::hot_reload()` | Better hot-reload support |
| `search()` | `crucible_tools::search()` + `crucible_services::SearchService` | Split into static + dynamic |

## Breaking Changes

### 1. Removed Components
- **MCP Server**: Replaced by service layer
- **crucible-mcp crate**: No longer available
- Direct MCP client connections

### 2. Changed APIs
- Tool execution now goes through service registry
- Hot-reload is now part of crucible-rune
- Tool metadata is richer and more structured

### 3. Configuration Changes
- MCP configuration replaced by service configuration
- Tool definitions are more structured
- Runtime configuration is more flexible

## Migration Benefits

### 1. Better Performance
- Reduced overhead from MCP protocol
- Direct tool execution without network hops
- Improved hot-reload capabilities

### 2. Enhanced Extensibility
- Static and dynamic tools in one registry
- Procedural macros for compile-time tool generation
- Better integration with core system

### 3. Improved Reliability
- No external MCP server dependency
- Better error handling and validation
- Stronger type safety with macros

### 4. Future-Proof Architecture
- Service-oriented design enables easier integration
- Plugin system is more robust
- Better separation of concerns

## Migration Examples

### Example 1: Search Tool Migration

#### Before (MCP)
```rust
// tools/search.rn
pub fn search(query: string) -> Result<Vec<Note>, Error> {
    let db = get_database();
    db.query("SELECT * FROM notes WHERE content CONTAINS $query", query)
}
```

#### After (Service)
```rust
// src/tools/search.rs
use crucible_rune_macros::rune_tool;

#[rune_tool(desc = "Search notes by query")]
pub fn search_notes(query: String) -> Result<Vec<String>, String> {
    use crucible_surrealdb::SurrealService;

    let db = SurrealService::new().await?;
    let notes: Vec<Note> = db
        .query("SELECT * FROM notes WHERE content CONTAINS $query")
        .bind(("query", query))
        .await
        .map_err(|e| format!("Database error: {}", e))?;

    Ok(notes.into_iter().map(|n| n.id).collect())
}
```

### Example 2: Agent Tool Migration

#### Before (MCP Agent)
```rust
// tools/agent.rn
pub fn analyze_note(path: string) -> Result<Analysis, Error> {
    let content = read_file(path)?;
    let llm = get_llm_service();
    llm.analyze(content)
}
```

#### After (Service Agent)
```rust
// src/tools/analysis.rs
use crucible_rune_macros::rune_tool;
use crucible_llm::LLMService;

#[rune_tool(
    desc = "Analyze note content",
    category = "analysis",
    async
)]
pub async fn analyze_note(path: String) -> Result<serde_json::Value, String> {
    // Read file
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    // Analyze with LLM
    let llm = LLMService::new().await?;
    let analysis = llm.analyze_content(&content).await
        .map_err(|e| format!("Analysis failed: {}", e))?;

    Ok(analysis)
}
```

## Testing Migration

### 1. Unit Tests
```rust
// Test static tool
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_notes() {
        let result = search_notes("test".to_string()).unwrap();
        assert!(!result.is_empty());
    }
}
```

### 2. Integration Tests
```rust
#[tokio::test]
async fn test_service_tool_execution() {
    let services = ServiceRegistry::new(test_config()).await?;
    let tools = services.list_available_tools().await?;

    assert!(tools.iter().any(|t| t.name == "search_notes"));

    let result = services.execute_tool("search_notes", json!({
        "query": "test"
    })).await?;

    assert!(result.is_success());
}
```

## Troubleshooting

### Common Issues

#### 1. Tool Not Found
**Problem**: `Error: Tool 'search' not found`

**Solution**: Check that the tool is registered in both static and dynamic registries

```rust
// Debug tool listing
let tools = services.list_available_tools().await?;
println!("Available tools: {:?}", tools);
```

#### 2. Service Not Started
**Problem**: `Error: Service registry not initialized`

**Solution**: Ensure service registry is properly initialized before use

```rust
// Initialize services
let services = ServiceRegistry::new(config).await?;
services.start_all().await?;
```

#### 3. Hot Reload Not Working
**Problem**: Changes to Rune scripts don't take effect

**Solution**: Ensure hot-reload feature is enabled

```yaml
services:
  tools:
    hot_reload: true  # Enable hot-reload
```

## Performance Considerations

### 1. Tool Registration
- Static tools are registered at compile time
- Dynamic tools are registered at runtime
- Tool lookup is O(1) for static, O(log n) for dynamic

### 2. Service Initialization
- Services start lazily on first use
- Background services can be pre-started
- Resource usage is optimized

### 3. Memory Usage
- Static tools have minimal overhead
- Dynamic tools use hot-reload with file watching
- Tool metadata is cached for performance

## Future Enhancements

The service architecture enables several future enhancements:

1. **Service Discovery**: Automatic service registration and discovery
2. **Service Composition**: Combine multiple services for complex operations
3. **Service Metrics**: Built-in performance monitoring and metrics
4. **Service Isolation**: Run services in separate sandboxes
5. **Service Scaling**: Automatic scaling based on load

## Support

For migration assistance:
1. Check the [Crucible Architecture](../docs/ARCHITECTURE.md) documentation
2. Review the [Crucible Services](../crates/crucible-services/) API
3. Join the Crucible community discussions
4. File issues on the GitHub repository

---

*This migration guide will be updated as the service architecture evolves. Check for the latest version in the documentation repository.*