# Rune Hooks

**Status**: Implemented
**System**: plugins
**Related**: [event-system.md](./event-system.md), [discovery.md](./discovery.md)

## Overview

Rune hooks are event handlers written in the Rune scripting language that integrate with Crucible's unified event system. They provide a lightweight way to extend functionality without recompiling the application.

**Key Features**:
- **Declarative syntax** via `#[hook(...)]` attributes
- **Pattern matching** with glob support for selective event handling
- **Priority control** for deterministic execution order
- **Hot reload** support for rapid development
- **Full access** to event payloads and context

## Hook Attribute Syntax

### Basic Syntax

```rune
/// Hook description (optional but recommended)
#[hook(event = "event:type", pattern = "glob", priority = 50)]
pub fn my_hook(ctx, event) {
    // Process event
    event  // Return modified event
}
```

### Attribute Parameters

| Parameter | Required | Type | Default | Description |
|-----------|----------|------|---------|-------------|
| `event` | ✅ Yes | String | - | Event type to handle (e.g., "tool:after") |
| `pattern` | ❌ No | String | `"*"` | Glob pattern for identifier matching |
| `priority` | ❌ No | Integer | `100` | Execution order (lower = earlier) |
| `description` | ❌ No | String | Doc comment | Human-readable description |
| `enabled` | ❌ No | Boolean | `true` | Whether hook is active |

### Event Types

Valid values for the `event` parameter:

**Tool Events**:
- `tool:before` - Before tool execution (can cancel)
- `tool:after` - After successful execution
- `tool:error` - Tool execution failed
- `tool:discovered` - New tool registered

**Note Events**:
- `note:parsed` - Note parsing complete
- `note:created` - New note created
- `note:modified` - Note content changed

**MCP Events**:
- `mcp:attached` - Upstream MCP server connected

**Custom Events**:
- `custom` - User-defined events

See [event-system.md](./event-system.md) for complete event type documentation.

### Pattern Matching

Patterns use glob syntax to match event identifiers:

```rune
// Match all events
#[hook(event = "tool:after", pattern = "*")]

// Match specific prefix
#[hook(event = "tool:after", pattern = "just_*")]

// Match specific suffix
#[hook(event = "tool:after", pattern = "*_test")]

// Match GitHub search tools
#[hook(event = "tool:after", pattern = "gh_search_*")]

// Exact match
#[hook(event = "tool:after", pattern = "just_build")]

// Single character wildcard
#[hook(event = "note:parsed", pattern = "daily/2024-??-??.md")]
```

### Priority Values

Lower numbers run earlier:

| Range | Purpose | Examples |
|-------|---------|----------|
| 0-9 | Critical (validation, security) | Permission checks |
| 10-49 | Early processing | Filtering, enrichment |
| 50-99 | Normal processing | Transformation |
| 100-149 | Late processing (default) | General hooks |
| 150-199 | Post-processing | Cleanup |
| 200+ | Audit and logging | Event recording |

## Hook Function Signature

All hook functions must have this signature:

```rune
pub fn hook_name(ctx, event) -> event
```

### Parameters

**`ctx`** (EventContext):
- Mutable context object
- Methods:
  - `ctx.set(key, value)` - Store metadata
  - `ctx.get(key)` - Retrieve metadata
  - `ctx.remove(key)` - Remove metadata
  - `ctx.contains(key)` - Check if key exists
  - `ctx.emit(event)` - Emit new event
  - `ctx.emit_custom(name, payload)` - Emit custom event

**`event`** (Event):
- Event object with fields:
  - `event.event_type` - Event type string
  - `event.identifier` - Event identifier (tool name, note path, etc.)
  - `event.payload` - Event data (JSON object)
  - `event.timestamp_ms` - Unix timestamp in milliseconds
  - `event.cancelled` - Whether event is cancelled
  - `event.source` - Event source (optional)

### Return Value

Must return the event (possibly modified):

```rune
pub fn my_hook(ctx, event) {
    // Modify event
    event.payload.processed = true;

    // Return modified event
    event
}
```

To cancel an event (only for `tool:before`):

```rune
pub fn my_hook(ctx, event) {
    if should_cancel(event) {
        event.cancelled = true;
    }
    event
}
```

## Writing Hooks

### Example 1: Filter Test Output

```rune
/// Filter verbose test output to show only summary
#[hook(event = "tool:after", pattern = "just_test*", priority = 10)]
pub fn filter_test_output(ctx, event) {
    let result = event.payload.result;

    if let Some(content) = result.content {
        // Extract first text block
        if let Some(first) = content.get(0) {
            if first.type == "text" {
                let text = first.text;

                // Filter to summary lines only
                let filtered = filter_cargo_test(text);

                // Update content
                content[0].text = filtered;
            }
        }
    }

    event
}

fn filter_cargo_test(output) {
    let lines = output.split("\n");
    let summary_lines = [];

    for line in lines {
        if line.contains("running ") || line.contains("test result:") {
            summary_lines.push(line);
        }
    }

    summary_lines.join("\n")
}
```

### Example 2: Log Tool Calls

```rune
/// Log all tool executions to audit trail
#[hook(event = "tool:after", pattern = "*", priority = 200)]
pub fn log_tool_calls(ctx, event) {
    // Emit custom audit event
    ctx.emit_custom("audit:tool_executed", #{
        tool: event.identifier,
        timestamp: event.timestamp_ms,
        source: event.source,
        duration: event.payload.duration_ms,
    });

    // Pass through unchanged
    event
}
```

### Example 3: Enrich Recipe Metadata

```rune
/// Add category and tags to Just recipes
#[hook(event = "tool:discovered", pattern = "just_*", priority = 5)]
pub fn enrich_recipes(ctx, event) {
    let tool_name = event.identifier;
    let recipe_name = tool_name.replace("just_", "").replace("_", "-");

    // Categorize by name
    let category = categorize(recipe_name);
    let tags = get_tags(recipe_name, category);

    // Add to payload
    event.payload.category = category;
    event.payload.tags = tags;

    event
}

fn categorize(name) {
    if name.starts_with("test") { "testing" }
    else if name.starts_with("build") || name.starts_with("release") { "build" }
    else if name.contains("fmt") || name.contains("clippy") { "quality" }
    else if name.contains("doc") { "documentation" }
    else { "other" }
}

fn get_tags(name, category) {
    let tags = [];

    if category == "testing" || category == "quality" {
        tags.push("ci");
    }

    if category == "quality" {
        tags.push("quick");
    }

    tags
}
```

### Example 4: Tool Selector (Whitelist)

```rune
/// Only allow specific upstream tools
#[hook(event = "tool:discovered", pattern = "gh_*", priority = 5)]
pub fn filter_github_tools(ctx, event) {
    let allowed = [
        "gh_search_code",
        "gh_search_repositories",
        "gh_get_file_contents",
    ];

    let tool_name = event.identifier;

    // Cancel if not in whitelist
    if !allowed.contains(tool_name) {
        event.cancelled = true;
    }

    event
}
```

### Example 5: Add Default Arguments

```rune
/// Add default values to tool arguments
#[hook(event = "tool:before", pattern = "gh_search_*", priority = 20)]
pub fn add_search_defaults(ctx, event) {
    let args = event.payload;

    // Add default per_page if not specified
    if !args.contains("per_page") {
        args.per_page = 10;
    }

    // Add default sort if not specified
    if !args.contains("sort") {
        args.sort = "stars";
    }

    event
}
```

### Example 6: Transform Output Format

```rune
/// Convert tool output to TOON format
#[hook(event = "tool:after", pattern = "gh_*", priority = 50)]
pub fn toon_transform(ctx, event) {
    let result = event.payload.result;

    if let Some(content) = result.content {
        let transformed = to_toon(content);
        result.content = [#{
            type: "text",
            text: transformed,
        }];
    }

    event
}

fn to_toon(content) {
    // TOON transformation logic
    // (simplified example)
    let text = content[0].text;
    let lines = text.split("\n");
    let toon_lines = [];

    for line in lines {
        if !line.is_empty() {
            toon_lines.push("- " + line);
        }
    }

    toon_lines.join("\n")
}
```

### Example 7: Rate Limiting

```rune
/// Rate limit GitHub API calls
#[hook(event = "tool:before", pattern = "gh_*", priority = 5)]
pub fn rate_limit(ctx, event) {
    let tool_name = event.identifier;
    let now = event.timestamp_ms;

    // Check last call time
    if let Some(last_call) = ctx.get("last_" + tool_name) {
        let elapsed = now - last_call;
        let min_interval = 1000; // 1 second

        if elapsed < min_interval {
            // Too soon - cancel
            event.cancelled = true;
            return event;
        }
    }

    // Record this call
    ctx.set("last_" + tool_name, now);

    event
}
```

### Example 8: Error Recovery

```rune
/// Retry failed GitHub searches
#[hook(event = "tool:error", pattern = "gh_search_*", priority = 50)]
pub fn retry_on_error(ctx, event) {
    let error = event.payload.error;

    // Check if retriable error
    if error.contains("rate limit") || error.contains("timeout") {
        // Store retry info in context
        ctx.set("should_retry", true);
        ctx.set("retry_tool", event.identifier);
        ctx.set("retry_delay_ms", 5000);
    }

    event
}
```

## Advanced Patterns

### Multi-Stage Processing

```rune
/// Stage 1: Extract data
#[hook(event = "note:parsed", pattern = "*", priority = 10)]
pub fn extract_metadata(ctx, event) {
    let metadata = #{
        tags: event.payload.tags,
        links: event.payload.wikilinks.len(),
        words: event.payload.metadata.word_count,
    };

    // Store in context for next stage
    ctx.set("extracted_metadata", metadata);

    event
}

/// Stage 2: Process extracted data
#[hook(event = "note:parsed", pattern = "*", priority = 20)]
pub fn process_metadata(ctx, event) {
    // Retrieve from previous stage
    if let Some(metadata) = ctx.get("extracted_metadata") {
        // Process metadata
        if metadata.words > 1000 {
            event.payload.category = "long-form";
        } else {
            event.payload.category = "short-form";
        }
    }

    event
}
```

### Conditional Processing

```rune
/// Only process during work hours
#[hook(event = "tool:after", pattern = "*", priority = 100)]
pub fn business_hours_only(ctx, event) {
    let hour = get_hour(event.timestamp_ms);

    if hour >= 9 && hour < 17 {
        // Process during business hours
        ctx.emit_custom("metrics:tool_call", #{
            tool: event.identifier,
            hour: hour,
        });
    }

    event
}

fn get_hour(timestamp_ms) {
    // Convert timestamp to hour (simplified)
    ((timestamp_ms / 1000 / 60 / 60) % 24) as i64
}
```

### Chained Events

```rune
/// Emit follow-up events based on results
#[hook(event = "note:created", pattern = "daily/*", priority = 50)]
pub fn process_daily_note(ctx, event) {
    // Emit custom event for daily note processing
    ctx.emit_custom("workflow:daily_note_created", #{
        path: event.identifier,
        date: extract_date(event.identifier),
    });

    event
}

fn extract_date(path) {
    // Extract date from path like "daily/2024-12-05.md"
    path.replace("daily/", "").replace(".md", "")
}
```

## Error Handling

### Graceful Failures

Hooks use fail-open semantics by default. If a hook throws an error:
1. Error is logged
2. Original event is returned (unchanged)
3. Pipeline continues with next handler

```rune
pub fn my_hook(ctx, event) {
    // This will be caught and logged
    let risky_operation = event.payload.maybe_missing_field;

    // Better: check first
    if let Some(field) = event.payload.get("field") {
        // Process field
    }

    event
}
```

### Validation

```rune
#[hook(event = "tool:before", pattern = "*", priority = 5)]
pub fn validate_args(ctx, event) {
    let args = event.payload;

    // Validate required fields
    if !args.contains("required_field") {
        event.cancelled = true;
        ctx.set("error", "Missing required field");
        return event;
    }

    // Validate types
    if let Some(count) = args.get("count") {
        if count < 0 || count > 100 {
            event.cancelled = true;
            ctx.set("error", "Count must be between 0 and 100");
            return event;
        }
    }

    event
}
```

## Testing Hooks

### Local Testing

1. **Create test hook**: Write hook in `~/.crucible/hooks/test.rn`
2. **Restart Crucible**: Hooks are loaded on startup
3. **Trigger event**: Execute a tool or create a note
4. **Check logs**: Look for hook execution in logs
5. **Verify behavior**: Check that event was modified as expected

### Hot Reload

During development:
1. Modify hook script
2. Save file
3. Next event will use updated hook (if hot-reload enabled)

Note: Hot reload support varies by deployment. Check configuration.

### Debugging

Add debug output to hooks:

```rune
pub fn my_hook(ctx, event) {
    // Log to context for inspection
    ctx.set("debug_input", event.payload);

    // Process event
    let result = process(event);

    ctx.set("debug_output", result.payload);

    result
}
```

Then inspect context after event processing.

## Built-in Hooks

Crucible provides several built-in hooks (implemented in Rust):

| Hook | Event | Pattern | Purpose |
|------|-------|---------|---------|
| `builtin:test_filter` | `tool:after` | `just_test*` | Filter test output |
| `builtin:toon_transform` | `tool:after` | `*` | Transform to TOON format |
| `builtin:event_emit` | `tool:after` | `*` | Publish to webhooks |
| `builtin:tool_selector` | `tool:discovered` | `*` | Filter/namespace tools |
| `builtin:recipe_enrichment` | `tool:discovered` | `just_*` | Categorize recipes |

See `crates/crucible-rune/src/builtin_hooks.rs` for implementation details.

## Best Practices

### Performance

- **Keep hooks fast**: Avoid blocking operations
- **Use appropriate priority**: Lower priority = less impact on latency
- **Filter early**: Use specific patterns to reduce invocations
- **Batch processing**: Collect data in context, emit single event

### Maintainability

- **Add doc comments**: Explain what the hook does and why
- **Use descriptive names**: `filter_test_output` not `hook1`
- **Keep hooks focused**: One responsibility per hook
- **Version compatibility**: Check payload structure before accessing

### Security

- **Validate inputs**: Check event payloads before processing
- **Sanitize outputs**: Don't leak sensitive data in logs/events
- **Check sources**: Verify `event.source` for upstream events
- **Fail safely**: Use `event.cancelled = true` for security violations

## Common Pitfalls

### Infinite Loops

```rune
// DON'T: This creates an infinite loop
#[hook(event = "tool:after", pattern = "*")]
pub fn bad_hook(ctx, event) {
    // This emits tool:after which triggers this hook again!
    ctx.emit(Event::tool_after("my_tool", #{}));
    event
}

// DO: Use custom events instead
#[hook(event = "tool:after", pattern = "*")]
pub fn good_hook(ctx, event) {
    ctx.emit_custom("my_custom_event", #{
        tool: event.identifier
    });
    event
}
```

### Modifying Immutable Fields

```rune
// DON'T: Can't change event type or identifier
pub fn bad_hook(ctx, event) {
    event.event_type = "custom";  // Won't work
    event.identifier = "new_name";  // Won't work
    event
}

// DO: Modify payload only
pub fn good_hook(ctx, event) {
    event.payload.modified = true;  // OK
    event
}
```

### Missing Return

```rune
// DON'T: Forgetting to return event
pub fn bad_hook(ctx, event) {
    event.payload.processed = true;
    // Missing return!
}

// DO: Always return the event
pub fn good_hook(ctx, event) {
    event.payload.processed = true;
    event  // Return modified event
}
```

## See Also

- [event-system.md](./event-system.md) - Event types and payloads
- [discovery.md](./discovery.md) - How hooks are discovered
- `/examples/rune-hooks/` - Example hook scripts
- `crates/crucible-rune/src/hook_system.rs` - Hook registration
- `crates/crucible-rune/src/hook_types.rs` - Hook metadata parsing
