# Plugin Examples

Example Rune plugins demonstrating tools and hooks for the Crucible plugin system.

## Installation

Plugins can be installed at three levels:

1. **Global personal** (applies to all kilns):
   ```bash
   cp *.rn ~/.config/crucible/plugins/
   ```

2. **Kiln personal** (kiln-specific, gitignored):
   ```bash
   cp *.rn KILN/.crucible/plugins/
   ```

3. **Kiln shared** (version-controlled with kiln):
   ```bash
   cp *.rn KILN/plugins/
   ```

Restart Crucible to load plugins, or wait for hot-reload if enabled.

## Available Plugins

### filter_test_output.rn

**Type**: Hook
**Event**: `tool:after`
**Pattern**: `just_test*`
**Priority**: 10 (early)

Filters verbose test output to show only summaries and failures:
- Detects test framework (Cargo, pytest, Jest, Go)
- Extracts summary information
- Includes failure details
- Reduces token usage for LLM processing

### log_tool_calls.rn

**Type**: Hook
**Events**: `tool:after`, `tool:error`, `tool:discovered`
**Pattern**: `*` (all tools)
**Priority**: 200 (late)

Comprehensive audit logging:
- Logs every tool execution
- Tracks errors with context
- Records tool discovery
- Emits custom audit events

### enrich_recipes.rn

**Type**: Hook
**Event**: `tool:discovered`
**Pattern**: `just_*`
**Priority**: 5 (very early)

Auto-categorizes Just recipes:
- Categories: testing, build, quality, documentation, ci, web, maintenance
- Adds relevant tags
- Assigns priority based on category

### categorizer.rn

**Type**: Hook
**Event**: `tool:discovered`
**Pattern**: `just_*`
**Priority**: 5 (very early)

Recipe categorizer that auto-categorizes Just recipes by name patterns. Demonstrates the `#[hook(...)]` attribute pattern for event handling.

## Plugin Structure

### Single-File Plugin

Most plugins are single `.rn` files with `#[tool]` or `#[hook]` attributes:

```rune
/// Description of what this plugin does
#[hook(event = "tool:after", pattern = "*", priority = 100)]
pub fn my_hook(ctx, event) {
    // Process event
    event
}

#[tool(name = "my_tool", description = "Does something")]
pub fn my_tool(param) {
    Ok("result")
}
```

### Module Plugin

For complex plugins, use a directory with `mod.rn`:

```
my_plugin/
├── mod.rn       # Entry point
├── helpers.rn   # Helper module
└── types.rn     # Type definitions
```

## Customization

### Modifying Patterns

```rune
// Match only integration tests
#[hook(event = "tool:after", pattern = "just_test_integration*", priority = 10)]

// Match all GitHub tools
#[hook(event = "tool:after", pattern = "gh_*", priority = 50)]
```

### Adjusting Priority

Lower numbers run earlier:
- `priority = 5` - Very early (validation, security)
- `priority = 10` - Early (filtering, enrichment)
- `priority = 50` - Normal (transformation)
- `priority = 100` - Late (default)
- `priority = 200` - Very late (audit, logging)

### Disabling Hooks

```rune
#[hook(event = "tool:after", pattern = "*", priority = 10, enabled = false)]
pub fn disabled_hook(ctx, event) {
    event
}
```

## Creating Plugins

### Hook Template

```rune
/// Brief description
#[hook(
    event = "tool:after",
    pattern = "*",
    priority = 100
)]
pub fn my_hook(ctx, event) {
    // Modify event.payload as needed
    // Use ctx.set/get for cross-handler state
    // Use ctx.emit_custom() for new events
    event  // Must return event
}
```

### Tool Template

```rune
/// Tool description
#[tool(
    name = "my_tool",
    description = "What this tool does"
)]
pub fn my_tool(param) {
    // Implementation
    Ok(format!("Result: {}", param))
}
```

## Documentation

- **Plugin Discovery**: `/openspec/specs/plugins/discovery.md`
- **Hook Guide**: `/openspec/specs/plugins/hooks.md`
- **Creating Plugins**: `/docs/Help/Extending/Creating Plugins.md`
