---
description: React to events in your kiln with Rune scripts
status: implemented
tags:
  - extending
  - hooks
  - rune
  - events
aliases:
  - Hooks
  - Rune Hooks
---

# Event Hooks

Event hooks let you react to things happening in your kiln - tool calls, note changes, server connections. Write a Rune function, add an attribute, and Crucible calls it automatically.

## Basic Example

```rune
/// Log every tool call
#[hook(event = "tool:after", pattern = "*")]
pub fn log_tools(ctx, event) {
    println("Tool called: {}", event.identifier);
    event
}
```

Place this in a `.rn` file in your `Scripts/` folder and it runs whenever a tool executes.

## The Hook Attribute

Every hook needs the `#[hook(...)]` attribute:

```rune
#[hook(event = "tool:after", pattern = "gh_*", priority = 50)]
pub fn my_hook(ctx, event) {
    // Process event
    event  // Always return the event
}
```

**Parameters:**
| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `event` | Yes | - | Event type to handle |
| `pattern` | No | `"*"` | Glob pattern for filtering |
| `priority` | No | `100` | Lower runs first |

## Event Types

**Tool Events:**
- `tool:before` - Before tool runs (can cancel)
- `tool:after` - After tool completes
- `tool:error` - Tool failed
- `tool:discovered` - New tool registered

**Note Events:**
- `note:parsed` - Note was parsed
- `note:created` - New note created
- `note:modified` - Note changed

**Server Events:**
- `mcp:attached` - External MCP server connected

## Pattern Matching

Patterns use glob syntax:

```rune
#[hook(event = "tool:after", pattern = "*")]        // All tools
#[hook(event = "tool:after", pattern = "gh_*")]     // GitHub tools
#[hook(event = "tool:after", pattern = "just_test*")] // Just test recipes
```

## Practical Examples

### Filter Verbose Output

```rune
/// Keep only summary from test output
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn filter_test_output(ctx, event) {
    let result = event.payload.result;

    if let Some(content) = result.content {
        let text = content[0].text;
        let filtered = keep_summary_lines(text);
        content[0].text = filtered;
    }

    event
}
```

### Block Dangerous Operations

```rune
/// Prevent accidental deletions
#[hook(event = "tool:before", pattern = "*delete*", priority = 5)]
pub fn block_deletes(ctx, event) {
    println("Blocked: {}", event.identifier);
    event.cancelled = true;
    event
}
```

### Add Metadata to Tools

```rune
/// Tag tools by category
#[hook(event = "tool:discovered", pattern = "just_*", priority = 5)]
pub fn categorize_recipes(ctx, event) {
    let name = event.identifier;

    if name.contains("test") {
        event.payload.category = "testing";
    } else if name.contains("build") {
        event.payload.category = "build";
    }

    event
}
```

## The Event Object

Hooks receive an `event` with these fields:

```rune
event.event_type    // "tool:after", "note:parsed", etc.
event.identifier    // Tool name, note path, etc.
event.payload       // Event-specific data
event.timestamp_ms  // When it happened
event.cancelled     // Set true to cancel (tool:before only)
```

## The Context Object

Use `ctx` to store data and emit new events:

```rune
ctx.set("key", value)           // Store data
ctx.get("key")                  // Retrieve data
ctx.emit_custom("my:event", #{  // Emit custom event
    data: "value"
})
```

## Priority Guide

Lower numbers run earlier:

| Range | Use |
|-------|-----|
| 0-9 | Security/validation |
| 10-49 | Early processing |
| 50-99 | Transformation |
| 100-149 | General (default) |
| 150-199 | Cleanup |
| 200+ | Logging/audit |

## Common Patterns

### Multi-Stage Processing

```rune
/// Stage 1: Extract data
#[hook(event = "note:parsed", pattern = "*", priority = 10)]
pub fn extract(ctx, event) {
    ctx.set("tags", event.payload.tags);
    event
}

/// Stage 2: Use extracted data
#[hook(event = "note:parsed", pattern = "*", priority = 20)]
pub fn process(ctx, event) {
    if let Some(tags) = ctx.get("tags") {
        // Process tags
    }
    event
}
```

### Conditional Processing

```rune
#[hook(event = "tool:after", pattern = "*", priority = 100)]
pub fn conditional(ctx, event) {
    if should_process(event) {
        // Do something
    }
    event
}
```

## Best Practices

1. **Always return the event** - even if unchanged
2. **Keep hooks fast** - avoid blocking operations
3. **Use specific patterns** - reduces unnecessary invocations
4. **Handle errors gracefully** - check before accessing fields
5. **Add doc comments** - explain what the hook does

## See Also

- [[Help/Rune/Event Types]] - Complete event type reference
- [[Help/Rune/Error Handling]] - Fail-open semantics
- [[Help/Rune/Language Basics]] - Rune syntax
- [[Help/Rune/Crucible API]] - Available functions
- [[Help/Extending/MCP Gateway]] - External tool integration
- [[Extending Crucible]] - All extension points
