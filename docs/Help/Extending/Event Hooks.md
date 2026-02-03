---
description: React to events in your kiln with Lua scripts
status: implemented
tags:
  - extending
  - hooks
  - lua
  - events
aliases:
  - Hooks
  - Lua Hooks
---

# Event Hooks

Event hooks let you react to things happening in your kiln - tool calls, note changes, server connections. Write a Lua function, add an annotation, and Crucible calls it automatically.

## Basic Example

```lua
--- Log every tool call
-- @handler event="tool:after" pattern="*" priority=100
function log_tools(ctx, event)
    cru.log("info", "Tool called: " .. event.identifier)
    return event
end
```

Place this in a `.lua` file in your `plugins/` folder and it runs whenever a tool executes.

## The Handler Annotation

Every hook needs the `@handler` annotation:

```lua
--- My hook description
-- @handler event="tool:after" pattern="gh_*" priority=50
function my_hook(ctx, event)
    -- Process event
    return event  -- Always return the event
end
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

```lua
-- @handler event="tool:after" pattern="*"           -- All tools
-- @handler event="tool:after" pattern="gh_*"        -- GitHub tools
-- @handler event="tool:after" pattern="just_test*"  -- Just test recipes
```

## Practical Examples

### Filter Verbose Output

```lua
--- Keep only summary from test output
-- @handler event="tool:after" pattern="just_test*" priority=10
function filter_test_output(ctx, event)
    local result = event.payload.result

    if result and result.content then
        local text = result.content[1].text
        local filtered = keep_summary_lines(text)
        result.content[1].text = filtered
    end

    return event
end
```

### Block Dangerous Operations

```lua
--- Prevent accidental deletions
-- @handler event="tool:before" pattern="*delete*" priority=5
function block_deletes(ctx, event)
    cru.log("warn", "Blocked: " .. event.identifier)
    event.cancelled = true
    return event
end
```

### Add Metadata to Tools

```lua
--- Tag tools by category
-- @handler event="tool:discovered" pattern="just_*" priority=5
function categorize_recipes(ctx, event)
    local name = event.identifier

    if string.find(name, "test") then
        event.payload.category = "testing"
    elseif string.find(name, "build") then
        event.payload.category = "build"
    end

    return event
end
```

## The Event Object

Hooks receive an `event` with these fields:

```lua
event.event_type    -- "tool:after", "note:parsed", etc.
event.identifier    -- Tool name, note path, etc.
event.payload       -- Event-specific data
event.timestamp_ms  -- When it happened
event.cancelled     -- Set true to cancel (tool:before only)
```

## The Context Object

Use `ctx` to store data and emit new events:

```lua
ctx:set("key", value)           -- Store data
ctx:get("key")                  -- Retrieve data
ctx:emit("my:event", {          -- Emit custom event
    data = "value"
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

```lua
--- Stage 1: Extract data
-- @handler event="note:parsed" pattern="*" priority=10
function extract(ctx, event)
    ctx:set("tags", event.payload.tags)
    return event
end

--- Stage 2: Use extracted data
-- @handler event="note:parsed" pattern="*" priority=20
function process(ctx, event)
    local tags = ctx:get("tags")
    if tags then
        -- Process tags
    end
    return event
end
```

### Conditional Processing

```lua
-- @handler event="tool:after" pattern="*" priority=100
function conditional(ctx, event)
    if should_process(event) then
        -- Do something
    end
    return event
end
```

## Best Practices

1. **Always return the event** - even if unchanged
2. **Keep hooks fast** - avoid blocking operations
3. **Use specific patterns** - reduces unnecessary invocations
4. **Handle errors gracefully** - check before accessing fields
5. **Add doc comments** - explain what the hook does

## See Also

- [[Help/Extending/Custom Handlers]] - Advanced handler development
- [[Help/Extending/MCP Gateway]] - External tool integration
- [[Help/Lua/Language Basics]] - Lua syntax
- [[Extending Crucible]] - All extension points
