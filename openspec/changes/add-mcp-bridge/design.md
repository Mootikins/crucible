## Context

Crucible currently exposes tools via MCP (Kiln operations, Just recipes, Rune scripts). To be useful as an agent infrastructure layer, it needs to:

1. Act as MCP gateway - connect to external MCP servers and aggregate tools
2. Provide unified event system where ALL tool calls emit events
3. Enable Rune hooks to intercept/transform any event (tools, notes, etc.)
4. Transform tool outputs for LLM consumption (TOON, filtering)

This is a cross-cutting change affecting agents (tool registry) and plugins (event/hook system).

**Stakeholders:**
- LLM clients consuming Crucible's MCP interface
- Workflow system consuming tool result events
- Users writing custom Rune hooks

## Goals / Non-Goals

**Goals:**
- Connect to any MCP server (stdio or HTTP+SSE) using rmcp
- Unified event system for tools, notes, and custom events
- Hooks = event handlers (same system for everything)
- Built-in hooks: TOON transform, test filter, event emit
- User-defined Rune hooks with hot-reload
- Fail-open semantics (hook errors don't break tool calls)
- Configuration-driven upstream MCP server definitions

**Non-Goals:**
- Passive proxying (gateway actively connects and processes)
- Authentication/authorization for upstream servers (handled by server itself)
- Load balancing or failover between redundant servers
- Protocol translation (MCP only, not HTTP REST or gRPC)
- Workflow hierarchies (deferred - needs more design on nested structure)

## Decisions

### Decision: Use rmcp for both server and client roles

rmcp already handles MCP protocol details. Use `rmcp::Client` for upstream connections.

**Alternatives considered:**
- Custom MCP client: More control, but duplicates work
- Different library: No better Rust MCP client exists

### Decision: Interceptor pipeline with fail-open semantics

Interceptors form a chain. Errors log warnings but don't fail the tool call.

**Rationale:** Tool results are more valuable than transformation. Users can debug interceptors without breaking workflows.

### Decision: Built-in interceptors as separate structs

Each interceptor is a separate struct implementing `Interceptor` trait, not a monolithic handler.

**Rationale:** Composability, testability, single responsibility. Users can mix built-in and custom interceptors.

### Decision: Rune interceptors in `KILN/.crucible/interceptors/`

Per-kiln interceptor scripts, discovered at startup and hot-reloaded.

**Alternatives considered:**
- Global interceptors only: Less flexible per-project customization
- Inline in config: Harder to edit and version control

### Decision: InterceptorContext for cross-cutting data

Single context object passed through pipeline with tool metadata and custom storage.

**Rationale:** Pre-call interceptors may compute data needed by post-call interceptors (e.g., timing, request ID).

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    ExtendedMcpServer                            │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                  Interceptor Pipeline                     │  │
│  │  [Selector] → [Pre-hooks] → [Execute] → [Post-hooks]     │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                   │
│  ┌──────────┬──────────┬─────┴────┬────────────┐               │
│  │  Kiln    │  Just    │  Rune    │  Upstream  │               │
│  │  Tools   │  Tools   │  Tools   │  MCPs      │               │
│  └──────────┴──────────┴──────────┴────────────┘               │
└─────────────────────────────────────────────────────────────────┘
```

### Interceptor Trait

```rust
#[async_trait]
pub trait Interceptor: Send + Sync {
    /// Unique identifier for this interceptor
    fn name(&self) -> &str;

    /// Called before tool execution. Can modify request or short-circuit.
    async fn before_call(
        &self,
        ctx: &mut InterceptorContext,
        request: CallToolRequest,
    ) -> Result<CallToolRequest, InterceptorError>;

    /// Called after tool execution. Can transform result.
    async fn after_call(
        &self,
        ctx: &mut InterceptorContext,
        request: &CallToolRequest,
        result: CallToolResult,
    ) -> Result<CallToolResult, InterceptorError>;
}
```

### Pipeline Execution

```rust
impl InterceptorPipeline {
    pub async fn execute(
        &self,
        request: CallToolRequest,
        executor: impl ToolExecutor,
    ) -> CallToolResult {
        let mut ctx = InterceptorContext::new(&request);
        let mut req = request;

        // Pre-call phase
        for interceptor in &self.interceptors {
            match interceptor.before_call(&mut ctx, req).await {
                Ok(modified) => req = modified,
                Err(e) => {
                    warn!("Interceptor {} before_call failed: {}", interceptor.name(), e);
                    // Continue with unmodified request (fail-open)
                }
            }
        }

        // Execute tool
        let mut result = executor.execute(req.clone()).await;

        // Post-call phase
        for interceptor in &self.interceptors {
            match interceptor.after_call(&mut ctx, &req, result).await {
                Ok(modified) => result = modified,
                Err(e) => {
                    warn!("Interceptor {} after_call failed: {}", interceptor.name(), e);
                    // Continue with unmodified result (fail-open)
                }
            }
        }

        result
    }
}
```

### Configuration Schema

**Design decision:** Rune scripts self-register via `#[hook(...)]` attributes (same pattern as `#[tool(...)]`). TOML only configures discovery paths and built-in settings.

```toml
# =============================================================================
# MCP Gateway Configuration
# =============================================================================

[gateway]
enabled = true

[[gateway.servers]]
name = "github"
transport = "stdio"
command = ["npx", "-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
prefix = "gh_"

[[gateway.servers]]
name = "filesystem"
transport = "stdio"
command = ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/home/user"]
allowed_tools = ["read_file", "list_directory"]

[[gateway.servers]]
name = "remote-db"
transport = "sse"
url = "https://db-mcp.example.com/sse"
blocked_tools = ["drop_table", "truncate"]

# =============================================================================
# Hook Discovery (Rune scripts self-register via #[hook(...)])
# =============================================================================

[hooks]
# Directories to scan for .rn files with #[hook(...)] attributes
discovery_paths = [
    "~/.crucible/hooks/",
    "KILN/.crucible/hooks/",
]

# Built-in hook settings (these are Rust, not Rune)
[hooks.builtin.test_filter]
enabled = true
pattern = "just_test*"  # Override default pattern

[hooks.builtin.toon_transform]
enabled = true
pattern = "*"

[hooks.builtin.event_emit]
enabled = true
```

### Rune Hook Script Format

Scripts self-register using `#[hook(...)]` attribute (same pattern as `#[tool(...)]` for tools):

```rune
// ~/.crucible/hooks/summarize_search.rn

/// Summarize search results for LLM consumption
#[hook(event = "tool:after", pattern = "gh_search_*", priority = 50)]
pub fn summarize_search(ctx, event) {
    // Transform event.result
    let result = event.result;
    // ... transformation logic ...
    event.result = transformed;
    event  // Return modified event
}

/// Log all tool calls
#[hook(event = "tool:after", pattern = "*", priority = 100)]
pub fn log_all_tools(ctx, event) {
    ctx.emit("audit:tool_called", #{
        tool: event.tool_name,
        duration: event.duration_ms,
    });
    event  // Pass through unchanged
}
```

### Attribute Parsing Refactor

The `#[tool(...)]`, `#[param(...)]`, and `#[hook(...)]` attributes all follow the same pattern:
1. Regex-based discovery parses attributes from source
2. No-op macro registered so Rune compiler accepts them
3. Metadata extracted at discovery time, not compile time

This should be refactored into shared `AttributeDiscovery` infrastructure:

```rust
// crates/crucible-rune/src/attribute_discovery.rs
pub trait FromAttributes: Sized {
    fn attribute_name() -> &'static str;
    fn from_attrs(attrs: &str, fn_name: &str, path: &Path) -> Result<Self, Error>;
}

impl FromAttributes for RuneTool { ... }
impl FromAttributes for RuneHook { ... }

pub struct AttributeDiscovery;
impl AttributeDiscovery {
    pub fn discover_all<T: FromAttributes>(paths: &[PathBuf]) -> Vec<T>;
}
```

## Risks / Trade-offs

| Risk | Mitigation |
|------|------------|
| Upstream server unavailable | Timeout + clear error message, don't block other tools |
| Interceptor performance overhead | Interceptors are optional, measure and optimize hot paths |
| Rune script errors break pipeline | Fail-open semantics, log errors but continue |
| Tool name conflicts across servers | Mandatory namespace prefix for upstream tools |
| Hot-reload causes inconsistent state | Atomic swap of compiled interceptor, not gradual update |

## Migration Plan

1. **Phase 1**: Implement interceptor trait and pipeline, migrate existing `filter_test_output`
2. **Phase 2**: Add MCP bridge client with stdio transport
3. **Phase 3**: Add HTTP+SSE transport, built-in interceptors
4. **Phase 4**: Add Rune interceptor support with hot-reload
5. **Phase 5**: Configuration file support and CLI commands

No breaking changes - this is additive functionality.

## Open Questions

1. **Event bus implementation**: Use existing event system or create new one?
2. **LLM provider interface**: Reuse `crucible-llm` or dedicated interceptor provider?
3. **Timeout configuration**: Per-upstream or global? Per-interceptor?
4. **Metrics/observability**: Add OpenTelemetry spans for pipeline execution?
