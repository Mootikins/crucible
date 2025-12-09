## Why

Crucible needs to act as an **MCP gateway** - connecting to external MCP servers (filesystem, GitHub, databases, etc.) and exposing their tools through a unified interface with filtering and transformation. This enables:

1. **Tool aggregation** - Combine tools from multiple MCP servers into a single interface
2. **Output transformation** - Convert JSON responses to TOON, filter verbose output, add semantic compression
3. **Event-driven hooks** - All tool calls (from any source) emit events that Rune hooks can intercept
4. **LLM enrichment** (future) - Intercept tool results and enrich with auxiliary LLM calls based on flow/spec

The gateway pattern means Crucible actively connects to upstream MCPs and processes all tool interactions through a unified event system - useful whether accessed via MCP, ACP, or as a standalone agent.

## What Changes

### New: Unified Event System
- All tool calls emit typed events (`tool:before`, `tool:after`, `tool:error`)
- Tool discovery emits `tool:discovered` for filtering/enrichment at gateway level
- MCP connections emit `mcp:attached` when upstream server connects
- Note operations emit events (`note:parsed`, `note:created`, `note:modified`)
- Rune hooks subscribe to events with wildcard patterns

### New: Hook System (replaces Interceptors)
- Hooks ARE event handlers - same system for tools, notes, and custom events
- Built-in hooks: test output filter, TOON transform, event emitter
- Rune hooks discovered in `KILN/.crucible/hooks/`
- Hooks can transform event payloads (filter pattern)

### New: MCP Gateway Client
- Connect to upstream MCP servers via rmcp (stdio spawn or HTTP+SSE)
- Emit `mcp:attached` and `tool:discovered` events
- Handle `toolListChanged` notifications for dynamic updates
- Configuration section for external MCP server definitions

### New: Tool Selector (as hook)
- Implemented as `tool:discovered` event hook
- Whitelist/blacklist tools from upstream servers
- Rename/namespace tools to avoid conflicts

### Modified: ExtendedMcpServer
- Add upstream MCP client registry
- Route ALL tool calls through event system
- Replace existing event_pipeline with unified EventBus

## Impact

- **Affected specs**: agents/tool-execution, plugins (new capability)
- **Affected crates**: `crucible-tools`, `crucible-rune`
- **Dependencies**: rmcp (already present, used for client role)
- **Future**: Just MCP may be extracted to separate repo, connected as upstream
