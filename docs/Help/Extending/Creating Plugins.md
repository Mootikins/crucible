---
description: Build plugins to extend Crucible with tools, hooks, workflows, and more
status: implemented
tags:
  - extending
  - plugins
  - lua
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
│       │   ├── init.lua     # Main module
│       │   ├── parser.lua   # Helper module
│       │   └── README.md    # Documentation
│       └── quick-tag.lua    # Single-file plugin
```

Plugins are also discovered from global config:
- Linux: `~/.config/crucible/plugins/`
- macOS: `~/Library/Application Support/crucible/plugins/`
- Windows: `%APPDATA%\crucible\plugins\`

## Plugin Languages

Plugins can be written in:

| Language | Extension | Status |
|----------|-----------|--------|
| Lua | `.lua` | Implemented |
| Fennel | `.fnl` | Implemented (compiles to Lua) |

File extension determines the runtime. All languages use the same discovery and registration system.

## Single-File Plugin

The simplest plugin is a single `.lua` file:

```lua
-- .crucible/plugins/greet.lua

--- A friendly greeting tool
-- @tool name="greet" description="Say hello to someone"
-- @param name string "Name to greet"
function greet(args)
    return { message = "Hello, " .. args.name .. "!" }
end
```

This registers one tool. Agents can now call `greet`.

## Directory Plugin

For complex plugins, use a directory with a manifest and entry point:

```
plugins/tasks/
├── plugin.yaml     # Plugin manifest (required)
├── init.lua        # Entry point, exports public items
├── parser.lua      # TASKS.md format parser
├── commands.lua    # Command handlers
└── README.md       # Usage documentation
```

### Plugin Manifest

Every directory plugin needs a `plugin.yaml` (or `plugin.yml`, `manifest.yaml`, `manifest.yml`):

```yaml
name: tasks
version: 1.0.0
main: init.lua
description: Task management tools
author: Your Name

# Optional: declare dependencies
dependencies:
  - name: core-utils
    version: ">=1.0.0"

# Optional: request capabilities
capabilities:
  - filesystem
  - kiln
```

See [[Help/Extending/Plugin Manifest]] for the complete manifest specification.

```lua
-- init.lua - Main module that exports everything

local parser = require("parser")
local commands = require("commands")

--- List all tasks with status
-- @tool name="tasks_list" description="List all tasks"
-- @param path string "Path to TASKS.md"
function tasks_list(args)
    local tasks = parser.parse_tasks(args.path)
    return commands.list_tasks(tasks)
end

--- Get next available task
-- @tool name="tasks_next" description="Get the next available task"
-- @param path string "Path to TASKS.md"
function tasks_next(args)
    local tasks = parser.parse_tasks(args.path)
    return commands.next_task(tasks)
end

-- Export tools
return {
    tasks_list = tasks_list,
    tasks_next = tasks_next
}
```

## Providing Tools

Use doc comment annotations to expose functions as MCP tools:

```lua
--- Search notes by content
-- @tool name="search_notes" description="Search notes by content"
-- @param query string "Search query"
-- @param limit number "Maximum results (default: 10)"
function search_notes(args)
    local query = args.query
    local limit = args.limit or 10
    local results = crucible.search(query, { limit = limit })
    return { results = results }
end
```

Tools are automatically registered when the plugin loads.

## Providing Hooks

Use `@handler` to react to events:

```lua
--- Log all tool calls
-- @handler event="tool:after" pattern="*" priority=100
function log_tools(ctx, event)
    crucible.log("info", "Tool called: " .. event.tool_name)
    return event
end

--- Block dangerous operations
-- @handler event="tool:before" pattern="*delete*" priority=5
function block_deletes(ctx, event)
    event.cancelled = true
    return event
end
```

See [[Help/Extending/Event Hooks]] for event types and patterns.

## Plugin Lifecycle

1. **Discovery**: Crucible scans plugin directories for manifests
2. **Validation**: Manifests are validated (name, version, dependencies)
3. **Dependency Resolution**: Load order determined by dependencies
4. **Loading**: Each plugin is compiled/loaded by its runtime
5. **Registration**: Tools, hooks, commands, and views are registered
6. **Execution**: Components are invoked as needed
7. **Unloading**: Plugins can be disabled/unloaded at runtime

### Lifecycle States

| State | Description |
|-------|-------------|
| `Discovered` | Manifest found, not yet loaded |
| `Active` | Loaded and running |
| `Disabled` | Explicitly disabled by user |
| `Error` | Failed to load |

## Shell Commands

Plugins can execute shell commands using `crucible.shell()`:

```lua
--- Run project tests
-- @tool name="run_tests" description="Run the test suite"
function run_tests(args)
    local result = crucible.shell("cargo", {"test"})
    return { 
        stdout = result.stdout,
        exit_code = result.exit_code
    }
end
```

### Security Model

Shell commands are **deny by default**. Commands must be whitelisted at the workspace or global level to execute.

When a plugin tries a non-whitelisted command, the user is prompted to allow or deny it, with options to save the decision.

Common commands (`git`, `cargo`, `npm`, `docker`, etc.) are whitelisted by default.

### Workspace Shell Policy

```toml
# .crucible/workspace.toml
[security.shell]
whitelist = ["aws", "terraform"]  # Allow these commands
blacklist = ["docker run"]         # Block these (prefix match)
```

See [[Help/Config/workspaces]] for full security configuration.

### Shell Options

```lua
local result = crucible.shell("cargo", {"build"}, {
    cwd = "/path/to/project",      -- Working directory
    env = { RUST_LOG = "debug" },  -- Environment variables
    timeout = 60000,               -- Timeout in milliseconds
})

-- result.stdout, result.stderr, result.exit_code
```

## Fennel Support

For a Lisp-like experience with macros, use Fennel:

```fennel
;; .crucible/plugins/greet.fnl

(fn greet [args]
  "A friendly greeting tool"
  {:message (.. "Hello, " args.name "!")})

;; Export
{:greet greet}
```

Fennel files are compiled to Lua at load time. See [[Help/Lua/Language Basics]] for more on the Lua ecosystem.

## Providing Commands

Commands are slash-commands that users can invoke in the TUI:

```lua
--- List all tasks
-- @command name="tasks" hint="[add|list|done] <args>"
-- @param action string "Action to perform"
function M.tasks(ctx, args)
    if args.action == "list" then
        ctx:display_info("Listing tasks...")
    elseif args.action == "add" then
        ctx:display_info("Adding task: " .. (args[2] or ""))
    end
end
```

Commands receive a context object with display methods.

## Providing Views

Views are custom UI components rendered in the TUI:

```lua
--- Interactive graph visualization
-- @view name="graph"
function M.graph_view()
    local oil = cru.oil
    return oil.box({
        direction = "column",
        children = {
            oil.text("Graph View", { bold = true }),
            oil.divider(),
            oil.text("Nodes: 42, Edges: 128"),
        }
    })
end
```

See [[Help/Extending/Scripted UI]] for the `cru.oil` API.

## Best Practices

1. **One concern per plugin** - Keep plugins focused
2. **Document with README.md** - Explain what it does and how to use it
3. **Use descriptive tool names** - `tasks_list` not `list`
4. **Handle errors gracefully** - Return error tables with helpful messages
5. **Provide param descriptions** - Help agents understand your tools
6. **Minimize shell usage** - Prefer Crucible APIs over shelling out
7. **Declare capabilities** - Only request what you need in manifest

## Example: Tasks Plugin

See [[Help/Task Management]] for a complete example plugin that demonstrates:
- Programmatic tool generation
- File-as-state patterns
- Tools to workflow integration

## See Also

- [[Help/Extending/Plugin Manifest]] - Manifest format and programmatic API
- [[Help/Lua/Language Basics]] - Lua syntax
- [[Help/Lua/Configuration]] - Lua configuration
- [[Help/Extending/Event Hooks]] - Hook system
- [[Help/Extending/Custom Tools]] - Tool deep dive
- [[Help/Extending/Scripted UI]] - cru.oil UI building
- [[Help/Config/workspaces]] - Workspace and security configuration
- [[Extending Crucible]] - All extension points
