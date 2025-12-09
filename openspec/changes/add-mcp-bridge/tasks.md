## 1. Core Event System

- [ ] 1.1 Define `Event` struct with type, pattern identifier, payload, timestamp
- [ ] 1.2 Define `EventType` enum (tool:before, tool:after, note:parsed, etc.)
- [ ] 1.3 Define `EventContext` with metadata storage and emit capability
- [ ] 1.4 Implement `EventBus` for registration and dispatch
- [ ] 1.5 Add wildcard pattern matching for event identifiers (glob-style)
- [ ] 1.6 Add priority ordering for handler execution
- [ ] 1.7 Add unit tests for event dispatch and pattern matching

## 2. Hook System

- [ ] 2.1 Define `Hook` trait with `handle(ctx, event) -> Option<Event>` signature
- [ ] 2.2 Implement `RuneHook` wrapper for Rune script handlers
- [ ] 2.3 Implement `BuiltinHook` wrapper for Rust function handlers
- [ ] 2.4 Add hook discovery in `KILN/.crucible/hooks/` directory
- [ ] 2.5 Support `on_tool_after`, `on_note_parsed` naming convention
- [ ] 2.6 Support `#[hook(event = "tool:after", pattern = "just_*")]` attribute
- [ ] 2.7 Add hot-reload for hook scripts via file watcher
- [ ] 2.8 Add integration tests with sample Rune hooks

## 3. Built-in Hooks (Filters)

- [ ] 3.1 Refactor `filter_test_output` into `TestFilterHook` on `tool:after`
- [ ] 3.2 Create `ToonTransformHook` on `tool:after` using tq crate
- [ ] 3.3 Create `EventEmitHook` that publishes to external consumers
- [ ] 3.4 Add configurable patterns for built-in hooks (default: `just_test*`, `just_ci*`)
- [ ] 3.5 Add unit tests for each built-in hook

## 4. Tool Events

- [ ] 4.1 Emit `tool:before` event before tool execution in ExtendedMcpServer
- [ ] 4.2 Emit `tool:after` event after tool execution
- [ ] 4.3 Emit `tool:error` event on tool failure
- [ ] 4.4 Allow `tool:before` hooks to modify arguments or cancel execution
- [ ] 4.5 Allow `tool:after` hooks to transform result
- [ ] 4.6 Add tool source metadata (kiln, just, rune, upstream)

## 5. Note Events

- [ ] 5.1 Emit `note:parsed` event when note is parsed (include AST blocks)
- [ ] 5.2 Emit `note:created` event when new note is created
- [ ] 5.3 Emit `note:modified` event when note content changes
- [ ] 5.4 Define note event payload with path, frontmatter, blocks
- [ ] 5.5 Expose parsed AST structure in event context

## 6. MCP Bridge Client

- [ ] 6.1 Create `UpstreamMcpClient` struct using rmcp client role
- [ ] 6.2 Implement stdio transport (spawn process + connect)
- [ ] 6.3 Implement HTTP+SSE transport
- [ ] 6.4 Add tool discovery via `tools/list` request
- [ ] 6.5 Handle `toolListChanged` notifications
- [ ] 6.6 Route upstream tool calls through event system
- [ ] 6.7 Add integration test with mock MCP server

## 7. Tool Selector

- [ ] 7.1 Implement `ToolSelector` with filter/transform methods
- [ ] 7.2 Add whitelist filtering (`allowed_tools`)
- [ ] 7.3 Add blacklist filtering (`blocked_tools`)
- [ ] 7.4 Add namespace prefixing (`prefix`)
- [ ] 7.5 Integrate selector as `tool:discovered` event hook

## 8. Integration

- [ ] 8.1 Wire EventBus into ExtendedMcpServer
- [ ] 8.2 Replace existing event_pipeline with unified EventBus
- [ ] 8.3 Replace existing EventHandler (recipe enrichment) with hook
- [ ] 8.4 Add `upstream_clients` to ExtendedMcpServer
- [ ] 8.5 Aggregate tools from Kiln + Just + Rune + upstream MCPs

## 9. Configuration

- [ ] 9.1 Define TOML schema for event/hook configuration
- [ ] 9.2 Add `[hooks]` section for built-in hook settings
- [ ] 9.3 Add `[bridge]` section for upstream MCP configuration
- [ ] 9.4 Support pattern overrides for built-in hooks
- [ ] 9.5 Add configuration validation at startup

## 10. Documentation

- [ ] 10.1 Document event types and their payloads
- [ ] 10.2 Document hook script format and discovery
- [ ] 10.3 Add example Rune hook scripts (filter, transform, emit)
- [ ] 10.4 Document built-in hooks and configuration
- [ ] 10.5 Document MCP bridge configuration
