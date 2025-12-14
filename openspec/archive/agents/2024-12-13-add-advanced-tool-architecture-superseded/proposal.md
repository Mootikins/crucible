## Why

Enable advanced tool use capabilities in Crucible AI agents, inspired by Anthropic's advanced tool use patterns. This will provide seamless integration of hundreds/thousands of tools with dynamic discovery, efficient execution, and reliable invocation patterns.

## What Changes

- Implement advanced tool execution environment with Rune-based scripting
- Add tool search, programmatic tool calling, and deferred loading capabilities
- Create schema conversion pipeline from MCP tool definitions to Rune functions
- Implement sandboxed execution environment for orchestration logic
- Add tool use examples and input validation for improved accuracy
- Enhance existing ToolExecutor trait with advanced capabilities

**BREAKING**: Extends ToolExecutor trait interface and adds new execution models

## Impact

- Affected specs: tool-execution, mcp-server, rune-plugins
- Affected code:
  - `crates/crucible-core/src/traits/tools.rs` (ToolExecutor trait)
  - `crates/crucible-tools/src/mcp_server.rs` (MCP server enhancements)
  - `crates/crucible-plugins/src/runtime.rs` (Rune execution engine)
  - `crates/crucible-cli/src/commands/mcp.rs` (MCP command updates)