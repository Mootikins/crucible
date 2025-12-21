---
description: Complete reference for event types and their payloads
status: implemented
tags:
  - rune
  - events
  - reference
aliases:
  - Event Reference
  - Hook Events
---

# Event Types

Complete reference for all event types available in the Rune hook system. Each event type fires at specific points in the tool execution lifecycle and carries a structured payload.

## Event Type Overview

| Event Type | String Value | When It Fires |
|------------|--------------|---------------|
| ToolBefore | `tool:before` | Before tool execution (can modify args or cancel) |
| ToolAfter | `tool:after` | After successful tool execution |
| ToolError | `tool:error` | Tool execution failed with error |
| ToolDiscovered | `tool:discovered` | New tool found during discovery |
| NoteParsed | `note:parsed` | Note parsing complete with AST |
| NoteCreated | `note:created` | New note file created |
| NoteModified | `note:modified` | Note content changed |
| FileDeleted | `file:deleted` | File removed from disk |
| McpAttached | `mcp:attached` | MCP server connected |
| Custom | `custom` | User-defined events |

## Tool Events

### tool:before

Fires before any tool executes. Can modify arguments or cancel execution.

```rune
#[hook(event = "tool:before", pattern = "*delete*", priority = 5)]
pub fn prevent_deletes(ctx, event) {
    println("Blocking dangerous tool: {}", event.identifier);
    event.cancelled = true;
    event
}
```

Payload: `{ arguments: {...}, source: "just|rune|upstream|kiln" }`

### tool:after

Fires after successful tool execution. Can transform results.

```rune
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn filter_test_output(ctx, event) {
    let result = event.payload.result;
    event.payload.result = extract_summary(result);
    event
}
```

Payload: `{ result: {...}, duration_ms: 1234, source: "..." }`

### tool:error

Fires when tool execution fails. Handle errors or implement retry logic.

```rune
#[hook(event = "tool:error", pattern = "fetch_*", priority = 20)]
pub fn retry_failed_fetch(ctx, event) {
    println("Tool failed: {}", event.identifier);
    ctx.emit_custom("retry:fetch", event.payload);
    event
}
```

Payload: `{ error: "message", arguments: {...}, source: "..." }`

## Note Events

### note:parsed

Fires after a note is fully parsed into AST.

```rune
#[hook(event = "note:parsed", pattern = "*.md", priority = 10)]
pub fn index_tags(ctx, event) {
    let tags = event.payload.tags;
    ctx.set("indexed_tags", tags);
    event
}
```

Payload includes: `path`, `title`, `frontmatter`, `tags`, `wikilinks`, `blocks`, `content_hash`

### note:created

Fires when a new note file is created.

Payload: `{ path: "...", title: "...", frontmatter: {...} }`

### note:modified

Fires when note content changes.

Payload: `{ path: "...", change_type: "content|frontmatter|both", blocks_added: [...], blocks_removed: [...] }`

## Other Events

### file:deleted

Fires when a file is removed from disk.

### mcp:attached

Fires when an upstream MCP server connects successfully.

Payload: `{ server: "name", tool_count: 42, info: {...} }`

### custom

Fires when `ctx.emit_custom(name, payload)` is called. User-defined payload structure.

## Event Structure

All events share this common structure:

```rune
event.event_type     // "tool:after", "note:parsed", etc.
event.identifier     // Tool name, note path, etc.
event.payload        // Event-specific data
event.timestamp_ms   // Unix timestamp
event.cancelled      // Set to true to cancel (tool:before only)
event.source         // "just", "rune", "kiln", "upstream"
```

## See Also

- [[Help/Extending/Event Hooks]] - How to write event hooks
- [[Help/Rune/Best Practices]] - Patterns for effective hooks
