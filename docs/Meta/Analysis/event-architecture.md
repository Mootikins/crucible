---
description: Internal architecture of Crucible's event-driven processing system
tags:
  - architecture
  - events
  - internals
---

# Event System Architecture

This document describes the event-driven architecture used by Crucible for processing file changes and maintaining the knowledge graph.

## Overview

Crucible uses an event-driven architecture to process file changes through a cascade of handlers. When a markdown file is created, modified, or deleted, the following event cascade is triggered:

```
FileChanged -> NoteParsed -> EntityStored -> BlocksUpdated -> EmbeddingRequested -> EmbeddingGenerated
     ^            ^              ^               ^                  ^                     ^
   Watch       Parser         Storage         Storage           Embedding            Embedding
```

## Core Components

### EventBus

The `EventBus` from `crucible-rune` is the central event dispatch mechanism. It:
- Maintains a registry of event handlers
- Dispatches events to handlers based on event type and pattern matching
- Supports priority-based handler ordering
- Provides fail-open semantics (handler errors don't block event processing)

### SessionEvent

The `SessionEvent` enum (from `crucible-core`) defines all possible events in the system:

```rust
pub enum SessionEvent {
    // File System Events
    FileChanged { path, kind },
    FileDeleted { path },
    FileMoved { from, to },

    // Note Events (Parsed)
    NoteParsed { path, block_count, payload },
    NoteCreated { path, title },
    NoteModified { path, change_type },

    // Storage Events
    EntityStored { entity_id, entity_type },
    EntityDeleted { entity_id, entity_type },
    BlocksUpdated { entity_id, block_count },

    // Tag Events
    TagAssociated { entity_id, tag },

    // Embedding Events
    EmbeddingRequested { entity_id, block_id, priority },
    EmbeddingStored { entity_id, block_id, dimensions, model },
    EmbeddingFailed { entity_id, block_id, error },
    EmbeddingBatchComplete { entity_id, count, duration_ms },

    // Custom Events (for Rune handlers)
    Custom { name, payload },

    // ... and more
}
```

### Event Handlers

Handlers subscribe to specific event types and perform operations when events are emitted:

| Handler | Priority | Events | Actions |
|---------|----------|--------|---------|
| StorageHandler | 100 | NoteParsed, FileDeleted, FileMoved | Store/update/delete entities in EAV graph |
| TagHandler | 110 | NoteParsed | Extract and associate tags |
| EmbeddingHandler | 200 | NoteParsed | Generate and store embeddings |
| Rune Handlers | 500+ | Custom | User-defined custom logic |

### WatchManager

The `WatchManager` from `crucible-watch` monitors the file system for changes:
- Uses the `notify` crate for cross-platform file watching
- Supports debouncing to prevent duplicate events
- Emits `FileChanged`, `FileDeleted`, and `FileMoved` events
- Configurable file filters (e.g., only `.md` files)

## Event Flow

### 1. File Change Detection

```
User saves file -> OS notifies -> WatchManager detects -> FileChanged event emitted
```

### 2. Note Parsing

```
FileChanged received -> Parser reads file -> Extracts frontmatter, blocks, links -> NoteParsed event emitted
```

### 3. Storage Persistence

```
NoteParsed received -> StorageHandler extracts entities -> EAV graph updated -> EntityStored event emitted
```

### 4. Tag Association

```
NoteParsed received -> TagHandler extracts tags -> Tags associated with entity -> TagAssociated events emitted
```

### 5. Embedding Generation

```
NoteParsed received -> EmbeddingHandler identifies blocks -> Embeddings generated -> EmbeddingStored events emitted
```

## Handler Registration

Handlers are registered during event system initialization:

```rust
use crucible_cli::event_system::initialize_event_system;

let handle = initialize_event_system(&config).await?;
// Handlers are now registered and the watch manager is running
```

### Priority System

Handlers execute in priority order (lower numbers first):
- **50**: Pre-processing hooks
- **100**: StorageHandler (entities must exist first)
- **110**: TagHandler (tags reference entities)
- **200**: EmbeddingHandler (embeddings reference entities/blocks)
- **500+**: Rune handlers (custom logic)

## Handler Result Types

Handlers return a `HandlerResult` to control event processing:

```rust
pub enum HandlerResult<E> {
    Continue(E),                    // Success, continue to next handler
    Cancel,                         // Stop processing (for preventable events)
    SoftError { event, error },     // Non-fatal error, continue with event
    FatalError(EventError),         // Fatal error, stop immediately
}
```

### Fail-Open Semantics

The event system uses fail-open semantics:
- Handler errors are logged but don't block event processing
- `SoftError` allows events to continue with recorded errors
- `FatalError` is reserved for critical failures

## Graceful Shutdown

The `EventSystemHandle` provides clean shutdown:

```rust
// Stop watching for new events
// Wait for pending events to drain
// Clean up resources
handle.shutdown().await?;
```

## Custom Handlers (Rune)

Users can create custom handlers in Rune scripting language. See [[Help/Extending/Event Hooks]] for the user-facing guide.

**Location**: `{kiln}/.crucible/handlers/*.rn`

```rune
// Example: custom_handler.rn
pub fn handle(event) {
    if event.event_type() == "note_parsed" {
        // Custom processing logic
        println!("Note parsed: {}", event.path);
    }
    event  // Return event to continue processing
}
```

Rune handlers run at priority 500+ (after built-in handlers).

## Integration Points

### CLI Commands

- **`cru process --watch`**: Uses full event system for file watching
- **`cru chat`**: Background watch with event system integration

### Programmatic Access

```rust
use crucible_cli::event_system::{EventSystemHandle, initialize_event_system};

// Initialize
let handle = initialize_event_system(&config).await?;

// Access components
let bus = handle.bus();
let watch = handle.watch_manager();
let storage = handle.storage_client();

// Add additional watches
watch.write().await.add_watch(path, watch_config).await?;

// Shutdown when done
handle.shutdown().await?;
```

## Performance Considerations

1. **Debouncing**: File changes are debounced (default 500ms) to prevent duplicate processing
2. **Parallel Processing**: Multiple files can be processed concurrently
3. **Embedding Batching**: Embeddings are batched to optimize provider calls
4. **Priority Ordering**: Critical handlers run first to minimize cascade failures

## Error Handling

- Handler errors are collected and attached to the `EmitOutcome`
- Fatal errors stop the event cascade immediately
- Non-fatal errors are logged and processing continues
- The event bus remains operational even if individual handlers fail

## See Also

- [[Help/Extending/Event Hooks]] - User-facing hook guide
- [[Help/Extending/Custom Handlers]] - Writing Rust and Rune handlers
- [[Help/Rune/Event Types]] - Complete event type reference
