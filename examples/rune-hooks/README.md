# Rune Hook Examples

This directory contains example Rune hook scripts demonstrating common patterns and use cases for the Crucible event system.

## Installation

To use these hooks:

1. **Global installation** (applies to all kilns):
   ```bash
   cp *.rn ~/.crucible/hooks/
   ```

2. **Kiln-specific installation** (applies only to current kiln):
   ```bash
   cp *.rn KILN/.crucible/hooks/
   ```

3. **Restart Crucible** to load the hooks, or wait for hot-reload if enabled.

## Available Hooks

### filter_test_output.rn

**Purpose**: Filter verbose test output to show only summaries and failures.

**Event**: `tool:after`
**Pattern**: `just_test*`
**Priority**: 10 (early processing)

**What it does**:
- Detects test framework (Cargo, pytest, Jest, Go)
- Extracts summary information (X passed, Y failed)
- Includes failure details (limited to prevent token overflow)
- Filters out verbose individual test logs

**Benefits**:
- Reduces token usage for LLM processing
- Improves comprehension of test results
- Highlights failures without noise

**Example output transformation**:
```
Before:
  running 42 tests
  test foo::test_one ... ok
  test foo::test_two ... ok
  ... (40 more lines)
  test result: ok. 42 passed; 0 failed

After:
  running 42 tests
  test result: ok. 42 passed; 0 failed
```

### log_tool_calls.rn

**Purpose**: Comprehensive audit logging for all tool executions.

**Events**: `tool:after`, `tool:error`, `tool:discovered`
**Pattern**: `*` (all tools)
**Priority**: 200 (late, for audit)

**What it does**:
- Logs every tool execution with metadata
- Tracks errors separately with detailed context
- Records tool discovery events
- Emits custom audit events for external processing

**Benefits**:
- Complete audit trail of tool usage
- Error tracking and monitoring
- Integration with external logging systems
- Debugging and troubleshooting

**Emitted events**:
- `audit:tool_executed` - Normal execution
- `audit:tool_error` - Error occurred
- `audit:tool_discovered` - New tool registered

### enrich_recipes.rn

**Purpose**: Automatically categorize and tag Just recipes.

**Event**: `tool:discovered`
**Pattern**: `just_*`
**Priority**: 5 (very early)

**What it does**:
- Categorizes recipes by name pattern:
  - `testing`: test*, *-test
  - `build`: build*, release*
  - `quality`: fmt*, clippy*, check*, lint*
  - `documentation`: *doc*
  - `ci`: ci, *-ci
  - `web`: *web*, *vite*
  - `maintenance`: *clean*
- Adds relevant tags (ci, quick, build, doc, etc.)
- Assigns priority based on category

**Benefits**:
- Organized tool listings in MCP clients
- Enables filtering by category/tags
- Provides execution priority hints
- Automatic metadata without manual annotation

**Example enrichment**:
```json
{
  "name": "just_test",
  "category": "testing",
  "tags": ["ci"],
  "priority": 20
}
```

## Customization

### Modifying Patterns

Change the `pattern` parameter to match different tools:

```rune
// Match only integration tests
#[hook(event = "tool:after", pattern = "just_test_integration*", priority = 10)]

// Match all GitHub tools
#[hook(event = "tool:after", pattern = "gh_*", priority = 50)]

// Match specific tool
#[hook(event = "tool:after", pattern = "just_build", priority = 50)]
```

### Adjusting Priority

Lower numbers run earlier:

```rune
// Run very early (validation, security)
#[hook(..., priority = 5)]

// Run early (filtering, enrichment)
#[hook(..., priority = 10)]

// Run normally (transformation)
#[hook(..., priority = 50)]

// Run late (default)
#[hook(..., priority = 100)]

// Run very late (audit, logging)
#[hook(..., priority = 200)]
```

### Disabling Hooks

Set `enabled = false`:

```rune
#[hook(event = "tool:after", pattern = "*", priority = 10, enabled = false)]
pub fn disabled_hook(ctx, event) {
    event
}
```

Or delete/rename the file.

## Creating Your Own Hooks

### Template

```rune
/// Brief description of what this hook does
///
/// Detailed explanation:
/// - What it processes
/// - How it transforms data
/// - Why it's useful
#[hook(
    event = "tool:after",      // Event type to handle
    pattern = "*",             // Pattern to match identifiers
    priority = 100,            // Execution order (lower = earlier)
    description = "Optional"   // Override doc comment
)]
pub fn my_hook(ctx, event) {
    // Process event
    // Modify event.payload as needed
    // Use ctx.set/get for cross-handler state
    // Use ctx.emit_custom() for new events

    event  // Must return event
}
```

### Common Patterns

**Filtering/cancellation**:
```rune
#[hook(event = "tool:before", pattern = "dangerous_*", priority = 5)]
pub fn block_dangerous(ctx, event) {
    event.cancelled = true;  // Cancel execution
    event
}
```

**Data extraction**:
```rune
#[hook(event = "note:parsed", pattern = "*", priority = 50)]
pub fn extract_metadata(ctx, event) {
    let tags = event.payload.tags;
    ctx.set("extracted_tags", tags);  // Store for other hooks
    event
}
```

**Result transformation**:
```rune
#[hook(event = "tool:after", pattern = "gh_*", priority = 50)]
pub fn transform_github(ctx, event) {
    let result = event.payload.result;
    result.content[0].text = summarize(result.content[0].text);
    event
}
```

**Conditional processing**:
```rune
#[hook(event = "tool:after", pattern = "*", priority = 100)]
pub fn conditional(ctx, event) {
    if event.payload.some_field == "value" {
        // Process only if condition met
        event.payload.processed = true;
    }
    event
}
```

## Testing Hooks

1. **Enable verbose logging** to see hook execution
2. **Trigger an event** by executing a tool or creating a note
3. **Check logs** for hook invocation and any errors
4. **Verify behavior** by inspecting tool output or emitted events

## Troubleshooting

### Hook not executing

- Check pattern matches the event identifier
- Verify event type is correct
- Ensure `enabled = true`
- Check for compilation errors in logs

### Hook errors

- Review Rune syntax
- Check payload structure before accessing fields
- Use `if let Some(x) = ...` for optional fields
- Look for error messages in logs

### Unexpected behavior

- Check hook priority (order matters)
- Verify pattern matching
- Look for other hooks that might interfere
- Add debug output with `ctx.set("debug", value)`

## Documentation

For complete documentation on the event system and hook writing:

- **Event System**: `/openspec/specs/plugins/event-system.md`
- **Hook Guide**: `/openspec/specs/plugins/hooks.md`
- **Discovery**: `/openspec/specs/plugins/discovery.md`
- **MCP Gateway**: `/openspec/specs/agents/mcp-gateway.md`

## Contributing

To contribute new example hooks:

1. Create a well-documented `.rn` file
2. Add description to this README
3. Test thoroughly
4. Submit PR with example and documentation

Good example hooks demonstrate:
- Clear, focused functionality
- Proper error handling
- Comprehensive documentation
- Real-world utility
