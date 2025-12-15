# Writing Event Handlers

This guide explains how to create custom event handlers for the Crucible event system.

## Handler Types

Crucible supports two types of handlers:

1. **Rust Handlers**: Compiled handlers with full access to the Rust ecosystem
2. **Rune Handlers**: Scripted handlers for user customization without recompilation

## Rust Handlers

### Basic Structure

```rust
use crucible_core::events::{SessionEvent, SharedEventBus};
use crucible_rune::{EventType, Handler};
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

### Registering a Rust Handler

```rust
use crucible_rune::{EventBus, Handler};

fn register_handler(bus: &mut EventBus, handler: MyHandler) {
    let handler = Arc::new(handler);

    // Create closure that captures the handler
    let h = handler.clone();
    let bus_handler = Handler::new(
        "my_handler_note_parsed",
        EventType::NoteParsed,
        "*",  // Pattern to match (glob syntax)
        move |_ctx, event| {
            // Note: Can't use async directly in closure
            // Actual async handling happens via emit_session
            Ok(event)
        },
    ).with_priority(MyHandler::PRIORITY);

    bus.register(bus_handler);
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

    pub fn new(store: Arc<EAVGraphStore>, emitter: SharedEventBus<SessionEvent>) -> Self {
        Self { store, emitter }
    }

    async fn handle_note_parsed(&self, event: &SessionEvent) -> Result<()> {
        if let SessionEvent::NoteParsed { path, payload, .. } = event {
            // Extract and store the entity
            let entity_id = self.store.upsert_note(path, payload).await?;

            // Emit EntityStored event
            self.emitter.emit(SessionEvent::EntityStored {
                entity_id: entity_id.clone(),
                entity_type: EventEntityType::Note,
            }).await?;
        }
        Ok(())
    }
}
```

#### TagHandler

Handles tag extraction and association:

```rust
// From crucible-surrealdb/src/event_handlers/tag_handler.rs

pub struct TagHandler {
    store: Arc<EAVGraphStore>,
    emitter: SharedEventBus<SessionEvent>,
}

impl TagHandler {
    pub const PRIORITY: u32 = 110;

    async fn handle_note_parsed(&self, event: &SessionEvent) -> Result<()> {
        if let SessionEvent::NoteParsed { path, payload, .. } = event {
            // Extract tags from frontmatter
            let tags = extract_tags(payload)?;

            // Associate each tag and emit events
            for tag in tags {
                self.store.associate_tag(path, &tag).await?;
                self.emitter.emit(SessionEvent::TagAssociated {
                    entity_id: path.to_string(),
                    tag: tag.clone(),
                }).await?;
            }
        }
        Ok(())
    }
}
```

## Rune Handlers

Rune handlers are scripts that process events without requiring Rust compilation.

### Location

Place Rune handler files in:
```
{kiln}/.crucible/handlers/*.rn
```

### Basic Structure

```rune
// my_handler.rn

// Main handler function - receives event, returns event
pub fn handle(event) {
    // Check event type
    let event_type = event.event_type();

    match event_type {
        "note_parsed" => {
            println!("Note parsed: {}", event.path());
            // Process the event
        }
        "file_changed" => {
            println!("File changed: {}", event.path());
        }
        _ => {
            // Ignore other events
        }
    }

    // Return the event to continue processing
    event
}
```

### Event API in Rune

Events expose methods for common operations:

```rune
pub fn handle(event) {
    // Get event metadata
    let event_type = event.event_type();  // "note_parsed", "file_changed", etc.
    let identifier = event.identifier();  // Path or entity ID

    // Check event categories
    if event.is_note_event() {
        println!("Processing note event");
    }
    if event.is_file_event() {
        println!("Processing file event");
    }
    if event.is_storage_event() {
        println!("Processing storage event");
    }

    event
}
```

### Cancelling Events

Handlers can cancel preventable events:

```rune
pub fn handle(event) {
    // Security check - prevent processing of sensitive files
    if event.path().contains(".secret") {
        println!("Blocking access to secret file");
        return null;  // Cancel the event
    }

    event
}
```

### Emitting Custom Events

```rune
pub fn handle(event) {
    // Process the event
    process_event(event);

    // Emit a custom event for downstream handlers
    emit_custom("my_handler_done", #{
        "source": event.path(),
        "timestamp": now()
    });

    event
}
```

## Testing Handlers

### Unit Tests

```rust
#[tokio::test]
async fn test_my_handler() {
    use crucible_core::events::NoOpEmitter;

    // Create handler with no-op emitter for testing
    let emitter = Arc::new(NoOpEmitter::new());
    let handler = MyHandler::new(service, emitter);

    // Test handle method directly
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

    // Initialize with all handlers
    let handle = initialize_event_system(&config).await?;

    // Create a file to trigger events
    std::fs::write(temp_dir.path().join("test.md"), "# Test\n\nContent")?;

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify expected outcomes
    // ...

    handle.shutdown().await?;
}
```

### Testing Rune Handlers

```rust
#[tokio::test]
async fn test_rune_handler() {
    let temp_dir = TempDir::new()?;

    // Create handlers directory
    let handlers_dir = temp_dir.path().join(".crucible").join("handlers");
    std::fs::create_dir_all(&handlers_dir)?;

    // Write test handler
    std::fs::write(
        handlers_dir.join("test.rn"),
        r#"
        pub fn handle(event) {
            println!("Test handler invoked");
            event
        }
        "#,
    )?;

    // Initialize and test
    let config = create_test_config(temp_dir.path().to_path_buf());
    let handle = initialize_event_system(&config).await?;

    // Verify handler was loaded
    let count = handle.handler_count().await;
    assert!(count >= 3); // storage + tag + rune

    handle.shutdown().await?;
}
```

## Handler Best Practices

### 1. Use Appropriate Priority

- **50-99**: Pre-processing hooks
- **100-199**: Core data handlers (storage, tags)
- **200-299**: Enrichment handlers (embeddings)
- **300-499**: Analytics/reporting
- **500+**: Custom user handlers

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

1. Ensure handlers return `Ok(event)` not `Cancel`
2. Check for fatal errors in handler chain
3. Verify emitter is properly configured

### Rune Handler Errors

1. Check syntax with `rune check handlers/*.rn`
2. Verify `handle` function signature
3. Check for runtime errors in logs
