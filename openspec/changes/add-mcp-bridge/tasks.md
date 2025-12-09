## 1. Refactor: Unified Discovery Paths

- [ ] 1.1 Create `DiscoveryPaths` struct with type_name, defaults, additional, use_defaults
- [ ] 1.2 Implement `new(type_name, kiln_path)` with global + kiln defaults
- [ ] 1.3 Add `all_paths()` method combining defaults + additional
- [ ] 1.4 Add TOML config schema `[discovery.<type>]` with additional_paths, use_defaults
- [ ] 1.5 Migrate existing tool discovery to use `DiscoveryPaths`
- [ ] 1.6 Migrate existing event handler discovery to use `DiscoveryPaths`
- [ ] 1.7 Add unit tests for path resolution

## 2. Refactor: Unified Attribute Discovery

- [ ] 2.1 Create `FromAttributes` trait with `attribute_name()` and `from_attrs()`
- [ ] 2.2 Create `AttributeDiscovery` struct with generic `discover_all<T: FromAttributes>()`
- [ ] 2.3 Extract common regex parsing from discovery.rs into shared module
- [ ] 2.4 Implement `FromAttributes` for `RuneTool`
- [ ] 2.5 Implement `FromAttributes` for `RuneHook` (new)
- [ ] 2.6 Register `#[hook(...)]` no-op macro in executor (like `#[tool]`, `#[param]`)
- [ ] 2.7 (DEFERRED) Add caching of discovered attributes in SurrealDB for fast reload
- [ ] 2.8 Add unit tests for attribute parsing

## 3. Core Event System

- [ ] 3.1 Define `Event` struct with type, pattern identifier, payload, timestamp
- [ ] 3.2 Define `EventType` enum (tool:before, tool:after, note:parsed, etc.)
- [ ] 3.3 Define `EventContext` with metadata storage and emit capability
- [ ] 3.4 Implement `EventBus` for registration and dispatch
- [ ] 3.5 Add wildcard pattern matching for event identifiers (glob-style)
- [ ] 3.6 Add priority ordering for handler execution
- [ ] 3.7 Add unit tests for event dispatch and pattern matching

## 4. Hook System

- [ ] 4.1 Define `Hook` trait with `handle(ctx, event) -> Option<Event>` signature
- [ ] 4.2 Implement `RuneHook` wrapper for Rune script handlers
- [ ] 4.3 Implement `BuiltinHook` wrapper for Rust function handlers
- [ ] 4.4 Use `DiscoveryPaths::new("hooks", kiln_path)` for hook discovery
- [ ] 4.5 Use `AttributeDiscovery::discover_all::<RuneHook>()` for parsing
- [ ] 4.6 Add hot-reload for hook scripts via file watcher
- [ ] 4.7 Add integration tests with sample Rune hooks

## 5. Built-in Hooks (Filters)

- [ ] 5.1 Refactor `filter_test_output` into `TestFilterHook` on `tool:after`
- [ ] 5.2 Create `ToonTransformHook` on `tool:after` using tq crate
- [ ] 5.3 Create `EventEmitHook` that publishes to external consumers
- [ ] 5.4 Add configurable patterns for built-in hooks (default: `just_test*`, `just_ci*`)
- [ ] 5.5 Add unit tests for each built-in hook

## 6. Tool Events

- [ ] 6.1 Emit `tool:before` event before tool execution in ExtendedMcpServer
- [ ] 6.2 Emit `tool:after` event after tool execution
- [ ] 6.3 Emit `tool:error` event on tool failure
- [ ] 6.4 Allow `tool:before` hooks to modify arguments or cancel execution
- [ ] 6.5 Allow `tool:after` hooks to transform result
- [ ] 6.6 Add tool source metadata (kiln, just, rune, upstream)

## 7. Note Events

- [ ] 7.1 Emit `note:parsed` event when note is parsed (include AST blocks)
- [ ] 7.2 Emit `note:created` event when new note is created
- [ ] 7.3 Emit `note:modified` event when note content changes
- [ ] 7.4 Define note event payload with path, frontmatter, blocks
- [ ] 7.5 Expose parsed AST structure in event context

## 8. MCP Gateway Client

- [ ] 8.1 Create `UpstreamMcpClient` struct using rmcp client role
- [ ] 8.2 Implement stdio transport (spawn process + connect)
- [ ] 8.3 Implement HTTP+SSE transport
- [ ] 8.4 Add tool discovery via `tools/list` request
- [ ] 8.5 Handle `toolListChanged` notifications
- [ ] 8.6 Emit `mcp:attached` and `tool:discovered` events
- [ ] 8.7 Route upstream tool calls through event system
- [ ] 8.8 Add integration test with mock MCP server

## 9. Tool Selector (as hook)

- [ ] 9.1 Implement `ToolSelectorHook` on `tool:discovered` event
- [ ] 9.2 Add whitelist filtering (`allowed_tools`)
- [ ] 9.3 Add blacklist filtering (`blocked_tools`)
- [ ] 9.4 Add namespace prefixing (`prefix`)
- [ ] 9.5 Add unit tests for selector logic

## 10. Integration

- [ ] 10.1 Wire EventBus into ExtendedMcpServer
- [ ] 10.2 Replace existing event_pipeline with unified EventBus
- [ ] 10.3 Replace existing EventHandler (recipe enrichment) with hook on `tool:discovered`
- [ ] 10.4 Add `upstream_clients` to ExtendedMcpServer
- [ ] 10.5 Aggregate tools from Kiln + Just + Rune + upstream MCPs

## 11. Configuration

- [ ] 11.1 Add `[discovery]` section for path configuration
- [ ] 11.2 Add `[gateway]` section for upstream MCP server definitions
- [ ] 11.3 Add `[hooks.builtin]` section for built-in hook settings
- [ ] 11.4 Support pattern overrides for built-in hooks
- [ ] 11.5 Add configuration validation at startup

## 12. Documentation

- [ ] 12.1 Document event types and their payloads
- [ ] 12.2 Document `#[hook(...)]` attribute format
- [ ] 12.3 Document discovery path conventions
- [ ] 12.4 Add example Rune hook scripts (filter, transform, emit)
- [ ] 12.5 Document MCP gateway configuration
