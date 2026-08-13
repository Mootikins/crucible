---
title: Custom Handlers
description: Write custom event handlers in Rust or Lua for advanced processing
status: implemented
tags:
  - extending
  - handlers
  - rust
  - lua
  - events
aliases:
  - Writing Handlers
  - Handler Development
---

# Custom Handlers

This guide explains how to create custom event handlers for the Crucible event system. For the simpler hook-based approach, see [[Help/Extending/Event Hooks]].

## Handler Types

Crucible supports two types of handlers:

1. **Rust Handlers**: Compiled handlers with full access to the Rust ecosystem
2. **Lua Handlers**: Scripted handlers for user customization without recompilation

## Rust Handlers

### Basic Structure

```rust
use crucible_core::events::{SessionEvent, SharedEventBus};
use std::sync::Arc;

pub struct MyHandler {
    // Handler state (e.g., database connection, service reference)
    service: Arc<MyService>,
    emitter: SharedEventBus<SessionEvent>,
}

impl MyHandler {
    /// Handler priority (lower runs first)
    pub const PRIORITY: u32 = 150;

    pub fn new(service: Arc<MyService>, emitter: SharedEventBus<SessionEvent>) -> Self {
        Self { service, emitter }
    }

    /// Handle a NoteParsed event
    async fn handle_note_parsed(&self, path: &str, block_count: usize) -> Result<()> {
        // Your processing logic here
        self.service.process(path).await?;

        // Optionally emit downstream events
        self.emitter.emit(SessionEvent::Custom {
            name: "my_handler_complete".to_string(),
            payload: serde_json::json!({ "path": path }),
        }).await?;

        Ok(())
    }
}
```

### Built-in Handler Examples

#### StorageHandler

Handles database persistence:

```rust
// Illustrative — see crucible-daemon/src/watch/handlers/ for current handlers

pub struct StorageHandler {
    store: Arc<EAVGraphStore>,
    emitter: SharedEventBus<SessionEvent>,
}

impl StorageHandler {
    pub const PRIORITY: u32 = 100;

    async fn handle_note_parsed(&self, event: &SessionEvent) -> Result<()> {
        if let SessionEvent::NoteParsed { path, payload, .. } = event {
            let entity_id = self.store.upsert_note(path, payload).await?;

            self.emitter.emit(SessionEvent::EntityStored {
                entity_id: entity_id.clone(),
                entity_type: EventEntityType::Note,
            }).await?;
        }
        Ok(())
    }
}
```

## Lua Handlers

Lua handlers are scripts that process events without requiring Rust compilation.

> [!NOTE] One way in: a plugin
> Handlers are not a separate kind of thing to install. A plugin is a superset
> of a handler collection — it can register handlers *and* contribute tools,
> commands, views, config and services — so there is one import mechanism
> rather than two that overlap.
>
> A `<kiln>/handlers/` and `<kiln>/.crucible/handlers/` scan did exist, keyed
> off `-- @handler` doc comments. It is gone: it was a second, weaker loader
> for something plugins already do, and it auto-ran Lua out of any kiln you
> opened. Nothing shipped used it.

### Location

Handlers live in plugins, and register with `crucible.on` at load:

```
~/.config/crucible/plugins/      # your plugins
<runtimepath entry>/plugins/     # trees you opt into in config.toml
```

Registration is daemon-wide — a handler fires for every session, and filters on
what it is given (`ctx.session_id`, the event payload, `opts.pattern`) rather
than on where it was installed from. See [[Help/Extending/Event Hooks]] for the
eleven events and the cancel / handled / transform contract, and
[[Help/Extending/Creating Plugins]] for how a kiln can ship a plugin.

### Basic Structure

```lua
-- ~/.config/crucible/plugins/my-plugin.lua

crucible.on("pre_tool_call", { pattern = "*", priority = 100 }, function(ctx, event)
    cru.log("info", "Tool called: " .. tostring(event.tool))
end)

return { name = "my-plugin" }
```

### Event API in Lua

Handlers receive `(ctx, event)`.

`ctx` carries exactly one field: `ctx.session_id`. It is not decoration —
plugin handlers are registered once into a single Lua state shared by every
session in the daemon, so a handler keeping per-session state must key it by
this.

`event` is one **flat** table: `event.type` is the event name, and the payload
fields sit alongside it at the top level. There is no `event.payload` envelope
and no `event.identifier`.

```lua
crucible.on("pre_tool_call", { pattern = "*", priority = 100 }, function(ctx, event)
    cru.log("info", string.format(
        "session %s called %s", tostring(ctx.session_id), tostring(event.tool)))
end)
```

Field names differ per event — see [[Help/Extending/Event Hooks]] for each
event's payload.

### Cancelling Events

Return a directive table; do not mutate `event`. Lua-side mutation is ignored
because the return value is what chains to the next handler.

```lua
crucible.on("pre_tool_call", { pattern = "*", priority = 5 }, function(ctx, event)
    local path = event.args and event.args.file_path or ""
    if string.find(path, "%.secret") then
        cru.log("warn", "Blocked access to secret file")
        return { cancel = true, reason = "secret file access denied" }
    end
end)
```

`pre_tool_call` is the only hook that fails **closed**: a handler that raises
denies the call. Every other hook fails open, so a broken handler cannot wedge
a session. Cancel is meaningful only before execution — `tool_result` runs
after the fact, where cancel and handle are ignored.

To supply the result yourself instead of blocking, return
`{ handled = true, result = ... }`; the default executor is skipped.

### Transforming

Rather than emitting new events, handlers reshape the value flowing through
them by returning a patch. Each handler sees the previous one's output.

```lua
-- Redact secrets from what the model sees.
crucible.on("tool_result", { pattern = "bash" }, function(ctx, event)
    return { result = event.result:gsub("token=%S+", "token=[REDACTED]") }
end)
```

There is no `ctx:emit` for custom events on this path. To trigger downstream
work, call the API you need directly from the handler — handlers may await
async APIs such as `cru.shell.exec`, `cru.http`, and `cru.timer.sleep`.

## Testing Handlers

### Unit Tests

```rust
#[tokio::test]
async fn test_my_handler() {
    use crucible_core::events::NoOpEmitter;

    let emitter = Arc::new(NoOpEmitter::new());
    let handler = MyHandler::new(service, emitter);

    let result = handler.handle_note_parsed("test.md", 5).await;
    assert!(result.is_ok());
}
```

### Integration Tests

Register the handler on a `HandlerRegistry` and drive it with a `FileEvent`
(`crucible-daemon/src/watch/handlers/mod.rs`):

```rust
#[tokio::test]
async fn test_handler_in_registry() {
    let mut registry = HandlerRegistry::new();
    registry.register(Arc::new(MyHandler::new(service, emitter)));

    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("test.md");
    std::fs::write(&path, "# Test\n\nContent").unwrap();

    for handler in registry.get_handlers_for_event(&event) {
        handler.handle(&event).await.unwrap();
    }
    // Verify expected outcomes
}
```

For Lua handlers, the shipped-plugin suites under `runtime/plugins/*/tests/`
are the working reference; they run in CI via `shipped_plugin_lua_suite_passes`.

## Best Practices

### 1. Use Appropriate Priority

| Range | Use |
|-------|-----|
| 50-99 | Pre-processing hooks |
| 100-199 | Core data handlers (storage, tags) |
| 200-299 | Enrichment handlers (embeddings) |
| 300-499 | Analytics/reporting |
| 500+ | Custom user handlers |

### 2. Fail Gracefully

```rust
async fn handle_event(&self, event: &SessionEvent) -> Result<()> {
    match self.process(event).await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Log but don't fail the cascade
            warn!("Handler error (non-fatal): {}", e);
            Ok(())
        }
    }
}
```

### 3. Emit Downstream Events

Keep the cascade flowing by emitting appropriate events:

```rust
// After storing entity
self.emitter.emit(SessionEvent::EntityStored { ... }).await?;

// After updating blocks
self.emitter.emit(SessionEvent::BlocksUpdated { ... }).await?;
```

### 4. Avoid Blocking Operations

Use async/await for I/O operations:

```rust
// Good: Async I/O
let result = self.database.query(sql).await?;

// Bad: Blocking I/O
let result = std::fs::read_to_string(path)?;  // Blocks the async runtime
```

### 5. Handle Event Types Explicitly

```rust
async fn handle(&self, event: &SessionEvent) -> Result<()> {
    match event {
        SessionEvent::NoteParsed { path, .. } => {
            self.handle_note_parsed(path).await
        }
        SessionEvent::FileDeleted { path } => {
            self.handle_file_deleted(path).await
        }
        _ => Ok(()),  // Ignore other event types
    }
}
```

## Handler Lifecycle

1. **Registration**: Rust handlers are registered on a `HandlerRegistry`; Lua
   handlers register via `crucible.on` when their plugin loads
2. **Execution**: Handlers execute in priority order when events are emitted
3. **Cascade**: Handlers can emit new events, triggering further handlers
4. **Shutdown**: Handlers are dropped when the EventBus is dropped

## Troubleshooting

### Handler Not Executing

1. Check event type matches handler subscription
2. Verify priority allows handler to run
3. Check pattern matching (glob syntax)
4. Enable debug logging: `RUST_LOG=crucible_cli=debug`

### Events Not Propagating

1. Ensure handlers return the event (not cancel it)
2. Check for fatal errors in handler chain
3. Verify emitter is properly configured

### Lua Handler Errors

1. Check syntax with `lua -p handlers/*.lua`
2. Verify handler function signature
3. Check for runtime errors in logs

## See Also

- [[Help/Extending/Event Hooks]] - Simpler hook-based approach
- [[Help/Lua/Language Basics]] - Lua syntax
