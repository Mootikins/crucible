---
description: Build plugins to extend Crucible with tools, hooks, workflows, and more
status: implemented
tags:
  - extending
  - plugins
  - rune
aliases:
  - Plugin Development
  - Writing Plugins
---

# Creating Plugins

Plugins are executable extensions that add capabilities to Crucible. A plugin can provide:

- **Tools** - MCP-compatible functions agents can call
- **Hooks** - React to events (tool calls, note changes)

> **Note:** Agents and workflows are defined separately as markdown templates in `.crucible/agents/` and `.crucible/workflows/`. They use the tools that plugins provide. See [[Help/Extending/Agent Cards]] and [[Help/Workflows/Index]].

## Plugin Location

Plugins live in `.crucible/plugins/`:

```
your-kiln/
├── .crucible/
│   └── plugins/
│       ├── tasks/           # Directory plugin
│       │   ├── mod.rn       # Main module
│       │   ├── parser.rn    # Helper module
│       │   └── README.md    # Documentation
│       └── quick-tag.rn     # Single-file plugin
```

Plugins are also discovered from global config:
- Linux: `~/.config/crucible/plugins/`
- macOS: `~/Library/Application Support/crucible/plugins/`
- Windows: `%APPDATA%\crucible\plugins\`

## Plugin Languages

Plugins can be written in:

| Language | Extension | Status |
|----------|-----------|--------|
| Rune | `.rn` | Implemented |
| Lua | `.lua` | Planned |
| WASM | `.wasm` | Future |

File extension determines the runtime. All languages use the same discovery and registration system.

## Single-File Plugin

The simplest plugin is a single `.rn` file:

```rune
// .crucible/plugins/greet.rn

/// A friendly greeting tool
#[tool(
    name = "greet",
    description = "Say hello to someone"
)]
pub fn greet(name) {
    Ok(format!("Hello, {}!", name))
}
```

This registers one tool. Agents can now call `greet`.

## Directory Plugin

For complex plugins, use a directory with `mod.rn`:

```
plugins/tasks/
├── mod.rn          # Entry point, exports public items
├── parser.rn       # TASKS.md format parser
├── commands.rn     # CLI command handlers
└── README.md       # Usage documentation
```

```rune
// mod.rn - Main module that exports everything

mod parser;
mod commands;

use parser::parse_tasks;
use commands::{list_tasks, next_task, pick_task, done_task};

/// List all tasks with status
#[tool(name = "tasks_list")]
pub fn list(path) {
    let tasks = parse_tasks(path)?;
    commands::list_tasks(tasks)
}

/// Get next available task
#[tool(name = "tasks_next")]
pub fn next(path) {
    let tasks = parse_tasks(path)?;
    commands::next_task(tasks)
}

// ... more tools
```

## Providing Tools

Use the `#[tool]` attribute to expose functions as MCP tools:

```rune
#[tool(
    name = "search_notes",
    description = "Search notes by content",
    schema = #{
        query: #{ type: "string", description: "Search query" },
        limit: #{ type: "integer", default: 10 }
    }
)]
pub fn search_notes(query, limit) {
    let results = crucible::search_by_content(query, limit)?;
    Ok(results)
}
```

Tools are automatically registered when the plugin loads.

## Providing Hooks

Use `#[hook]` to react to events:

```rune
/// Log all tool calls
#[hook(event = "tool:after", pattern = "*")]
pub fn log_tools(ctx, event) {
    println!("Tool called: {}", event.identifier);
    event
}

/// Block dangerous operations
#[hook(event = "tool:before", pattern = "*delete*", priority = 5)]
pub fn block_deletes(ctx, event) {
    event.cancelled = true;
    event
}
```

See [[Help/Extending/Event Hooks]] for event types and patterns.

## Plugin Lifecycle

1. **Discovery**: Crucible scans plugin directories on startup
2. **Loading**: Each plugin is compiled/loaded by its runtime
3. **Registration**: Tools, hooks, and other exports are registered
4. **Execution**: Components are invoked as needed

## Standalone Scripts

For one-off automation (not reusable tools), use `Scripts/`:

```rune
// Scripts/cleanup.rn - Run manually with: cru script Scripts/cleanup.rn

pub fn main() {
    let notes = crucible::search_by_properties(#{})?;
    // ... cleanup logic
    Ok(())
}
```

Scripts in `Scripts/` are not auto-registered as tools.

## Best Practices

1. **One concern per plugin** - Keep plugins focused
2. **Document with README.md** - Explain what it does and how to use it
3. **Use descriptive tool names** - `tasks_list` not `list`
4. **Handle errors gracefully** - Return `Err()` with helpful messages
5. **Provide schemas** - Help agents understand your tools

## Example: Tasks Plugin

See [[Help/Task Management]] for a complete example plugin that demonstrates:
- Programmatic tool generation
- File-as-state patterns
- Tools→workflow integration

## See Also

- [[Help/Rune/Language Basics]] - Rune syntax
- [[Help/Rune/Crucible API]] - Available functions
- [[Help/Rune/Tool Definition]] - Tool attribute details
- [[Help/Extending/Event Hooks]] - Hook system
- [[Help/Extending/Custom Tools]] - Tool deep dive
- [[Extending Crucible]] - All extension points
