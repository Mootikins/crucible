## Why

Crucible needs to connect to external MCP servers (filesystem, GitHub, databases, etc.) and expose their tools through its unified interface. This enables:

1. **Tool aggregation** - Combine tools from multiple MCP servers into a single interface
2. **Output transformation** - Convert JSON responses to TOON, filter verbose output, add semantic compression
3. **LLM enrichment** - Intercept tool results and enrich with auxiliary LLM calls based on flow/spec
4. **Event emission** - Publish computed data to event bus for workflows and other consumers

The bridge pattern (vs proxy) means Crucible actively connects to upstream MCPs rather than passively forwarding - useful whether Crucible is accessed via MCP, ACP, or as a standalone agent.

## What Changes

### New: MCP Bridge Client
- Connect to upstream MCP servers via rmcp (stdio spawn or HTTP+SSE)
- Discover and aggregate tools with configurable namespacing
- Handle `toolListChanged` notifications for dynamic updates

### New: Interceptor Pipeline
- Pre-call hooks: validate, modify, or reject tool requests
- Post-call hooks: transform, filter, or enrich tool results
- Built-in interceptors: TOON transform, test output filter, LLM enrichment, event emitter
- Rune-based custom interceptors for user-defined logic

### New: Tool Selector
- Whitelist/blacklist tools from upstream servers
- Rename/namespace tools to avoid conflicts
- Priority ordering for tool discovery

### Modified: ExtendedMcpServer
- Add upstream MCP client registry
- Route tool calls through interceptor pipeline
- Expose aggregated tools from all sources (Kiln + Just + Rune + upstream MCPs)

## Impact

- **Affected specs**: agents/tool-execution, plugins (new capability)
- **Affected crates**: `crucible-tools`, `crucible-rune` (new: `crucible-mcp-bridge` or integrated)
- **Dependencies**: rmcp (already present, used for client role)
