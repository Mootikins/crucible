---
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
// From crucible-surrealdb/src/event_handlers/storage_handler.rs

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

### Location

Place Lua handler files in:
```
{kiln}/.crucible/handlers/*.lua
```

### Basic Structure

```lua
-- my_handler.lua

--- Handle events
-- @handler event="note:parsed" pattern="*" priority=100
function handle_note_parsed(ctx, event)
    cru.log("info", "Note parsed: " .. event.identifier)
    return event
end

--- Handle file changes
-- @handler event="file:changed" pattern="*" priority=100
function handle_file_changed(ctx, event)
    cru.log("info", "File changed: " .. event.identifier)
    return event
end
```

### Event API in Lua

Events expose fields for common operations:

```lua
-- @handler event="note:parsed" pattern="*" priority=100
function handle(ctx, event)
    -- Get event metadata
    local event_type = event.event_type  -- "note:parsed", "file:changed", etc.
    local identifier = event.identifier  -- Path or entity ID

    -- Access payload
    local tags = event.payload.tags
    local content = event.payload.content

    return event
end
```

### Cancelling Events

Handlers can cancel preventable events:

```lua
-- @handler event="tool:before" pattern="*" priority=5
function block_secrets(ctx, event)
    if string.find(event.identifier, ".secret") then
        cru.log("warn", "Blocked access to secret file")
        event.cancelled = true
    end
    return event
end
```

### Emitting Custom Events

```lua
-- @handler event="note:parsed" pattern="*" priority=100
function handle(ctx, event)
    -- Process event
    process_note(event)

    -- Emit custom event
    ctx:emit("my_handler_done", {
        source = event.identifier,
        timestamp = os.time()
    })

    return event
end
```

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

```rust
#[tokio::test]
async fn test_handler_in_event_system() {
    use crucible_cli::event_system::initialize_event_system;

    let temp_dir = TempDir::new()?;
    let config = create_test_config(temp_dir.path().to_path_buf());

    let handle = initialize_event_system(&config).await?;

    std::fs::write(temp_dir.path().join("test.md"), "# Test\n\nContent")?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify expected outcomes
    handle.shutdown().await?;
}
```

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

1. **Registration**: Handlers are registered during `initialize_event_system()`
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
- [[Meta/Analysis/Event Architecture]] - Internal event system design
- [[Help/Lua/Language Basics]] - Lua syntax
