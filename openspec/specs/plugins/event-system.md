# Event System

**Status**: Implemented
**System**: plugins
**Related**: [hooks.md](./hooks.md), [mcp-gateway.md](../agents/mcp-gateway.md)

## Overview

The unified event system provides the foundation for all event-driven functionality in Crucible. It enables hooks, interceptors, and other plugins to observe and modify operations throughout the system using a consistent, type-safe API.

**Key Components**:
- **EventBus**: Central dispatcher that routes events to handlers
- **Event**: Immutable event data with type, identifier, payload, and metadata
- **EventContext**: Mutable context for passing data between handlers and emitting events
- **Handler**: Function that processes events and returns modified events

## Architecture

```text
┌─────────────────┐
│  Tool Execution │
│  Note Parsing   │
│  MCP Gateway    │
└────────┬────────┘
         │
         ▼
    ┌─────────┐      emit()       ┌──────────────┐
    │  Event  │──────────────────► │  EventBus    │
    └─────────┘                    └──────┬───────┘
                                          │
                    ┌─────────────────────┴──────────────┐
                    │  Find matching handlers by:        │
                    │  - event_type (exact match)        │
                    │  - pattern (glob on identifier)    │
                    │  - enabled state                   │
                    └────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
    ┌─────────┐         ┌─────────┐         ┌─────────┐
    │Handler 1│         │Handler 2│         │Handler 3│
    │priority │         │priority │         │priority │
    │   10    │         │   50    │         │  100    │
    └────┬────┘         └────┬────┘         └────┬────┘
         │                   │                   │
         └────────────────┬──┴───────────────────┘
                          ▼
                   Event (possibly
                     modified)
```

## Event Types

All event types follow the format `category:action`.

### Tool Events

| Event Type | When Fired | Cancellable | Purpose |
|------------|-----------|-------------|---------|
| `tool:before` | Before tool execution | ✅ Yes | Validate args, modify inputs, cancel execution |
| `tool:after` | After successful execution | ❌ No | Transform output, log results, trigger workflows |
| `tool:error` | Tool execution failed | ❌ No | Handle errors, retry logic, notifications |
| `tool:discovered` | New tool registered | ✅ Yes | Filter tools, add metadata, namespace prefixing |

### Note Events

| Event Type | When Fired | Cancellable | Purpose |
|------------|-----------|-------------|---------|
| `note:parsed` | Note parsing complete | ❌ No | Index content, extract metadata, trigger updates |
| `note:created` | New note created | ❌ No | Initialize templates, trigger workflows |
| `note:modified` | Note content changed | ❌ No | Update indexes, invalidate caches, sync |

### MCP Events

| Event Type | When Fired | Cancellable | Purpose |
|------------|-----------|-------------|---------|
| `mcp:attached` | Upstream MCP server connected | ❌ No | Log connection, discover tools, emit tool:discovered |

### Custom Events

| Event Type | When Fired | Cancellable | Purpose |
|------------|-----------|-------------|---------|
| `custom` | User-defined via `ctx.emit_custom()` | Varies | Application-specific events |

## Event Structure

```rust
pub struct Event {
    /// Event type (tool:before, tool:after, note:parsed, etc.)
    pub event_type: EventType,

    /// Identifier for pattern matching (tool name, note path, etc.)
    pub identifier: String,

    /// Event payload (tool args, result, note content, etc.)
    pub payload: JsonValue,

    /// Timestamp in milliseconds since UNIX epoch
    pub timestamp_ms: u64,

    /// Whether this event has been cancelled (for tool:before)
    pub cancelled: bool,

    /// Source of the event (kiln, just, rune, upstream:server_name)
    pub source: Option<String>,
}
```

### Payload Formats

#### `tool:before` Payload

```json
{
  "arg1": "value1",
  "arg2": 42
}
```

The entire payload represents the tool's input arguments.

#### `tool:after` Payload

```json
{
  "result": {
    "content": [
      {"type": "text", "text": "Tool output here"}
    ],
    "is_error": false
  },
  "duration_ms": 123,
  "upstream": "github"  // optional, for upstream tools
}
```

#### `tool:error` Payload

```json
{
  "error": "Error message",
  "duration_ms": 50,
  "upstream": "github"  // optional
}
```

#### `tool:discovered` Payload

```json
{
  "name": "gh_search_code",
  "original_name": "search_code",
  "description": "Search code on GitHub",
  "input_schema": { /* JSON Schema */ },
  "upstream": "github"  // optional
}
```

#### `note:parsed` Payload

```json
{
  "path": "notes/example.md",
  "title": "Example Note",
  "frontmatter": {
    "tags": ["rust", "documentation"],
    "created": "2024-12-05"
  },
  "tags": ["rust", "documentation"],
  "wikilinks": [
    {
      "target": "other-note",
      "display": null,
      "section": "heading",
      "line": 15
    }
  ],
  "inline_links": [
    {
      "text": "Rust Docs",
      "url": "https://doc.rust-lang.org",
      "title": null,
      "line": 20
    }
  ],
  "blocks": [
    {
      "block_type": "heading",
      "content": "Introduction",
      "attributes": {"level": 1},
      "start_line": 1,
      "end_line": 1,
      "hash": "abc123"
    },
    {
      "block_type": "paragraph",
      "content": "This is a paragraph.",
      "attributes": {},
      "start_line": 3,
      "end_line": 3,
      "hash": "def456"
    }
  ],
  "metadata": {
    "word_count": 150,
    "char_count": 800,
    "heading_count": 3,
    "code_block_count": 1,
    "wikilink_count": 1
  },
  "content_hash": "sha256:...",
  "file_size": 2048
}
```

#### `note:created` Payload

```json
{
  "path": "notes/new.md",
  "file_size": 512,
  "frontmatter": { /* if parseable */ },
  "created_at": "2024-12-05T14:30:00Z"
}
```

#### `note:modified` Payload

```json
{
  "path": "notes/example.md",
  "change_type": "content",  // or "frontmatter", "both", "renamed"
  "old_hash": "abc123",
  "new_hash": "def456",
  "changed_blocks": [
    {
      "operation": "modified",  // or "added", "removed"
      "hash": "block789",
      "block_type": "paragraph",
      "line": 10
    }
  ],
  "modified_at": "2024-12-05T14:35:00Z"
}
```

#### `mcp:attached` Payload

```json
{
  "name": "github",
  "server": {
    "name": "@modelcontextprotocol/server-github",
    "version": "0.2.0",
    "protocol_version": "2024-11-05"
  },
  "transport": {
    "type": "stdio",
    "command": "npx"
  }
}
```

## EventContext

The `EventContext` provides mutable state that flows through the handler pipeline.

### Features

**Metadata Storage**: Share data between handlers
```rust
ctx.set("key", json!({"data": "value"}));
let value = ctx.get("key");
```

**Event Emission**: Emit new events during processing
```rust
ctx.emit(Event::custom("my_event", json!({})));
ctx.emit_custom("audit:log", json!({"action": "processed"}));
```

**Cross-Handler Communication**: Pass information forward
```rust
// Handler 1
ctx.set("processed_by", json!("handler1"));

// Handler 2 (later in pipeline)
if ctx.contains("processed_by") {
    // Skip processing
}
```

## Handler API

### Creating Handlers

```rust
use crucible_rune::event_bus::{Handler, EventType};

let handler = Handler::new(
    "my_handler",
    EventType::ToolAfter,
    "just_*",  // glob pattern
    |ctx, mut event| {
        // Modify event
        if let Some(obj) = event.payload.as_object_mut() {
            obj.insert("processed", json!(true));
        }

        // Emit audit event
        ctx.emit_custom("audit", json!({
            "tool": event.identifier,
            "timestamp": event.timestamp_ms
        }));

        Ok(event)
    }
)
.with_priority(50)
.with_enabled(true);
```

### Pattern Matching

Handlers use glob patterns to match event identifiers:

| Pattern | Matches | Examples |
|---------|---------|----------|
| `*` | Everything | All events |
| `just_*` | Starts with "just_" | `just_test`, `just_build` |
| `*_test` | Ends with "_test" | `unit_test`, `integration_test` |
| `gh_search_*` | GitHub search tools | `gh_search_code`, `gh_search_issues` |
| `note?` | Exactly 5 chars starting with "note" | `note1`, `notes` |
| `test` | Exact match | Only `test` |

### Handler Priority

Handlers execute in priority order (lower numbers = earlier execution):

- **0-9**: Critical early processing (security, validation)
- **10-49**: Early hooks (filtering, enrichment)
- **50-99**: Normal hooks (transformation)
- **100-149**: Late hooks (default priority)
- **150-199**: Post-processing hooks
- **200+**: Audit and logging hooks

### Error Handling

Handlers use **fail-open semantics**: errors are logged but don't stop the pipeline.

```rust
// Non-fatal error (pipeline continues)
Err(HandlerError::non_fatal("my_handler", "Something went wrong"))

// Fatal error (pipeline stops)
Err(HandlerError::fatal("my_handler", "Critical failure"))
```

### Cancellation

Only `tool:before` events can be cancelled:

```rust
if should_cancel(&event) {
    event.cancel();
    return Ok(event);
}
```

Cancelled events:
- Stop handler execution immediately
- Return to caller with `event.is_cancelled() == true`
- Allow caller to abort the operation

## Event Lifecycle

### Example: Tool Execution

```text
1. Tool call initiated
   ↓
2. Emit tool:before
   ├─ Handler: validate_args (priority 10)
   │  └─ Check arguments, return modified args
   ├─ Handler: add_defaults (priority 20)
   │  └─ Add default values if missing
   └─ Handler: security_check (priority 5)
      └─ Check permissions, potentially cancel
   ↓
3. Check if cancelled
   ├─ Yes → Return error
   └─ No → Continue
   ↓
4. Execute tool with modified args
   ↓
5. Emit tool:after (or tool:error)
   ├─ Handler: test_filter (priority 10)
   │  └─ Filter verbose test output
   ├─ Handler: toon_transform (priority 50)
   │  └─ Transform to TOON format
   └─ Handler: audit_log (priority 200)
      └─ Log execution to database
   ↓
6. Return result to caller
```

### Example: Note Parsing

```text
1. Note file read from disk
   ↓
2. Parse markdown + extract metadata
   ↓
3. Emit note:parsed
   ├─ Handler: index_content (priority 50)
   │  └─ Update search index
   ├─ Handler: extract_tags (priority 40)
   │  └─ Extract and normalize tags
   └─ Handler: update_graph (priority 60)
      └─ Update link graph
   ↓
4. Store in database
```

## EventBus API

### Registration

```rust
let mut bus = EventBus::new();

// Register a handler
bus.register(handler);

// Unregister by name
bus.unregister("my_handler");

// Get handler by name
let handler = bus.get_handler("my_handler");

// Count handlers for event type
let count = bus.count_handlers(EventType::ToolAfter);
```

### Emission

```rust
// Basic emission
let event = Event::tool_after("just_test", json!({"output": "..."}));
let (result, ctx, errors) = bus.emit(event);

// Check for errors
for error in errors {
    eprintln!("Handler error: {}", error);
}

// Recursive emission (processes events emitted by handlers)
let results = bus.emit_recursive(event);
```

## Built-in Events

Crucible emits events automatically for:

**Tools**:
- `tool:before` - Before `just_*`, `rune_*`, `kiln_*`, and upstream MCP tools
- `tool:after` - After successful execution
- `tool:error` - On execution failure
- `tool:discovered` - When tools are registered (startup, hot-reload, upstream attach)

**Notes**:
- `note:parsed` - After parsing markdown
- `note:created` - When file watcher detects new note
- `note:modified` - When file watcher detects changes

**MCP**:
- `mcp:attached` - When upstream MCP server connects

## Usage Examples

### Example 1: Rate Limiting

```rust
let rate_limiter = Handler::new(
    "rate_limiter",
    EventType::ToolBefore,
    "gh_*",  // GitHub tools only
    |ctx, mut event| {
        let tool_name = &event.identifier;

        // Check rate limit
        if is_rate_limited(tool_name) {
            event.cancel();
            return Ok(event);
        }

        // Track usage
        ctx.set("rate_limit_checked", json!(true));
        Ok(event)
    }
)
.with_priority(5);  // Run early
```

### Example 2: Result Caching

```rust
let cache = Handler::new(
    "result_cache",
    EventType::ToolBefore,
    "*",
    |ctx, mut event| {
        let cache_key = compute_cache_key(&event);

        if let Some(cached) = get_from_cache(&cache_key) {
            // Store cached result in context
            ctx.set("cached_result", cached);
            // Cancel execution - we have the answer
            event.cancel();
        }

        Ok(event)
    }
)
.with_priority(10);
```

### Example 3: Webhook Notification

```rust
let webhook = Handler::new(
    "webhook_notify",
    EventType::ToolAfter,
    "*",
    |ctx, event| {
        // Emit a custom event for webhook processing
        ctx.emit_custom("webhook:notify", json!({
            "tool": event.identifier,
            "timestamp": event.timestamp_ms,
            "source": event.source
        }));

        Ok(event)
    }
)
.with_priority(200);  // Run late
```

### Example 4: Note Backlink Indexing

```rust
let backlinks = Handler::new(
    "index_backlinks",
    EventType::NoteParsed,
    "*",
    |ctx, event| {
        if let Some(wikilinks) = event.payload.get("wikilinks") {
            for link in wikilinks.as_array().unwrap() {
                if let Some(target) = link.get("target") {
                    // Update backlink index
                    index_backlink(
                        &event.identifier,  // source note
                        target.as_str().unwrap()  // target note
                    );
                }
            }
        }

        Ok(event)
    }
);
```

## Integration with Rune Hooks

The EventBus integrates seamlessly with Rune script hooks:

1. **Rune hooks discovered**: Scripts with `#[hook(...)]` attributes are found
2. **Compiled to RuneHookHandler**: Each hook becomes a handler
3. **Registered on EventBus**: Hooks registered alongside built-in handlers
4. **Event dispatch**: EventBus treats Rune and Rust handlers identically
5. **Hot reload**: File changes trigger recompilation and re-registration

See [hooks.md](./hooks.md) for details on writing Rune hooks.

## Performance Considerations

**Handler Count**: The EventBus can handle hundreds of handlers efficiently. Handlers are filtered by event type before pattern matching.

**Pattern Matching**: Glob matching is O(n×m) where n=pattern length, m=text length. Use exact matches when possible.

**Payload Size**: Events use JSON for flexibility, but large payloads (>1MB) should be avoided. Consider storing large data elsewhere and passing references.

**Handler Execution**: Handlers run synchronously in priority order. Avoid blocking operations in handlers - use `ctx.emit()` to trigger async processing.

**Error Recovery**: Non-fatal errors continue the pipeline. Use fatal errors only for unrecoverable situations.

## See Also

- [hooks.md](./hooks.md) - Writing Rune hooks that use the event system
- [discovery.md](./discovery.md) - How hooks are discovered and loaded
- [mcp-gateway.md](../agents/mcp-gateway.md) - MCP integration with events
- `crates/crucible-rune/src/event_bus.rs` - Implementation
- `crates/crucible-rune/src/builtin_hooks.rs` - Built-in hook examples
