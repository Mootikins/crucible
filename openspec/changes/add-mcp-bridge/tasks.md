## 1. Refactor: Unified Discovery Paths

- [x] 1.1 Create `DiscoveryPaths` struct with type_name, defaults, additional, use_defaults
- [x] 1.2 Implement `new(type_name, kiln_path)` with global + kiln defaults
- [x] 1.3 Add `all_paths()` method combining defaults + additional
- [ ] 1.4 Add TOML config schema `[discovery.<type>]` with additional_paths, use_defaults
- [x] 1.5 Migrate existing tool discovery to use `DiscoveryPaths`
- [x] 1.6 Migrate existing event handler discovery to use `DiscoveryPaths`
- [x] 1.7 Add unit tests for path resolution

## 2. Refactor: Unified Attribute Discovery

- [x] 2.1 Create `FromAttributes` trait with `attribute_name()` and `from_attrs()`
- [x] 2.2 Create `AttributeDiscovery` struct with generic `discover_all<T: FromAttributes>()`
- [x] 2.3 Extract common regex parsing from discovery.rs into shared module
- [x] 2.4 Implement `FromAttributes` for `RuneTool`
- [x] 2.5 Implement `FromAttributes` for `RuneHook` (new)
- [x] 2.6 Register `#[hook(...)]` no-op macro in executor (like `#[tool]`, `#[param]`)
- [ ] 2.7 (DEFERRED) Add caching of discovered attributes in SurrealDB for fast reload
- [x] 2.8 Add unit tests for attribute parsing

## 3. Core Event System

- [x] 3.1 Define `Event` struct with type, pattern identifier, payload, timestamp
- [x] 3.2 Define `EventType` enum (tool:before, tool:after, note:parsed, etc.)
- [x] 3.3 Define `EventContext` with metadata storage and emit capability
- [x] 3.4 Implement `EventBus` for registration and dispatch
- [x] 3.5 Add wildcard pattern matching for event identifiers (glob-style)
- [x] 3.6 Add priority ordering for handler execution
- [x] 3.7 Add unit tests for event dispatch and pattern matching

## 4. Hook System

- [x] 4.1 Define `Hook` trait with `handle(ctx, event) -> Option<Event>` signature
- [x] 4.2 Implement `RuneHookHandler` wrapper for Rune script handlers
- [x] 4.3 Implement `BuiltinHook` wrapper for Rust function handlers
- [x] 4.4 Use `DiscoveryPaths::new("hooks", kiln_path)` for hook discovery
- [x] 4.5 Use `AttributeDiscovery::discover_all::<RuneHook>()` for parsing
- [ ] 4.6 Add hot-reload for hook scripts via file watcher (deferred - requires file watcher integration)
- [x] 4.7 Add integration tests with sample Rune hooks

## 5. Built-in Hooks (Filters)

- [x] 5.1 Refactor `filter_test_output` into `TestFilterHook` on `tool:after`
- [x] 5.2 Create `ToonTransformHook` on `tool:after` using tq crate (stub - needs tq integration)
- [x] 5.3 Create `EventEmitHook` that publishes to external consumers
- [x] 5.4 Add configurable patterns for built-in hooks (default: `just_test*`, `just_ci*`)
- [x] 5.5 Add unit tests for each built-in hook

## 6. Tool Events

- [x] 6.1 Emit `tool:before` event before tool execution (ToolEventEmitter)
- [x] 6.2 Emit `tool:after` event after tool execution
- [x] 6.3 Emit `tool:error` event on tool failure
- [x] 6.4 Allow `tool:before` hooks to modify arguments or cancel execution
- [x] 6.5 Allow `tool:after` hooks to transform result
- [x] 6.6 Add tool source metadata (kiln, just, rune, upstream) via ToolSource enum

Note: ToolEventEmitter provides the API, but ExtendedMcpServer integration is pending (Section 10)

## 7. Note Events

- [x] 7.1 Emit `note:parsed` event when note is parsed (include AST blocks)
- [x] 7.2 Emit `note:created` event when new note is created
- [x] 7.3 Emit `note:modified` event when note content changes
- [x] 7.4 Define note event payload with path, frontmatter, blocks
- [x] 7.5 Expose parsed AST structure in event context

Note: NoteEventEmitter provides the API with NotePayload, BlockInfo, etc.
Parser/watcher integration is pending (Section 10)

## 8. MCP Gateway Client

- [x] 8.1 Create `UpstreamMcpClient` struct using rmcp client role
- [x] 8.2 Implement stdio transport (spawn process + connect) - TransportConfig::Stdio
- [x] 8.3 Implement HTTP+SSE transport - TransportConfig::Sse
- [x] 8.4 Add tool discovery via `tools/list` request - update_tools() method
- [x] 8.5 Handle `toolListChanged` notifications - update_tools() for refresh
- [x] 8.6 Emit `mcp:attached` and `tool:discovered` events
- [x] 8.7 Route upstream tool calls through event system - call_tool_with_events()
- [ ] 8.8 Add integration test with mock MCP server (deferred - requires rmcp transport wiring)

Note: mcp_gateway.rs provides the client API. Actual rmcp transport connection
requires additional wiring in call_tool_internal(). McpGatewayManager provides
multi-client management.

## 9. Tool Selector (as hook)

- [x] 9.1 Implement `ToolSelectorHook` on `tool:discovered` event
- [x] 9.2 Add whitelist filtering (`allowed_tools`)
- [x] 9.3 Add blacklist filtering (`blocked_tools`)
- [x] 9.4 Add namespace prefixing (`prefix`) - also added `suffix` support
- [x] 9.5 Add unit tests for selector logic

Note: ToolSelectorConfig provides config, create_tool_selector_hook() creates
the handler. Blacklist takes precedence over whitelist.

## 10. Integration

- [x] 10.1 Wire EventBus into ExtendedMcpServer
- [x] 10.2 Replace existing event_pipeline with unified EventBus
- [x] 10.3 Replace existing EventHandler (recipe enrichment) with hook on `tool:discovered`
- [x] 10.4 Add `upstream_clients` to ExtendedMcpServer
- [x] 10.5 Aggregate tools from Kiln + Just + Rune + upstream MCPs

## 11. Configuration

- [x] 11.1 Add `[discovery]` section for path configuration
- [x] 11.2 Add `[gateway]` section for upstream MCP server definitions
- [x] 11.3 Add `[hooks.builtin]` section for built-in hook settings
- [x] 11.4 Support pattern overrides for built-in hooks
- [x] 11.5 Add configuration validation at startup

## 12. Documentation

- [x] 12.1 Document event types and their payloads
- [x] 12.2 Document `#[hook(...)]` attribute format
- [x] 12.3 Document discovery path conventions
- [x] 12.4 Add example Rune hook scripts (filter, transform, emit)
- [x] 12.5 Document MCP gateway configuration
