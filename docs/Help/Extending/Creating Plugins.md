---
title: Creating Plugins
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
    local results = cru.kiln.search(query, { limit = limit })
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
    cru.log("info", "Tool called: " .. event.tool_name)
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

Plugins can execute shell commands using `cru.shell()`:

```lua
--- Run project tests
-- @tool name="run_tests" description="Run the test suite"
function run_tests(args)
    local result = cru.shell("cargo", {"test"})
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

### Project Shell Policy

```toml
# .crucible/project.toml
[security.shell]
whitelist = ["aws", "terraform"]  # Allow these commands
blacklist = ["docker run"]         # Block these (prefix match)
```

See [[Help/Config/workspaces]] for full security configuration.

### Shell Options

```lua
local result = cru.shell("cargo", {"build"}, {
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

## Testing Plugins

Crucible ships a built-in test runner based on `describe`/`it` blocks. Tests live in a `tests/` directory inside your plugin and follow the `*_test.lua` naming convention.

### Writing Tests

```lua
-- tests/init_test.lua

describe("tasks_list", function()
    local plugin = require("init")

    before_each(function()
        test_mocks.setup({
            kiln = {
                search = function() return {} end,
            },
        })
    end)

    after_each(function()
        test_mocks.reset()
    end)

    it("returns empty list when no tasks exist", function()
        local result = plugin.tools.tasks_list.fn({ file = "nonexistent.md" })
        assert.equal(result.count, 0)
    end)

    it("filters completed tasks when show_completed is false", function()
        local result = plugin.tools.tasks_list.fn({
            file = "TASKS.md",
            show_completed = false,
        })
        assert.equal(type(result.tasks), "table")
    end)
end)
```

### Running Tests

```bash
# Test a specific plugin
cru plugin test path/to/my-plugin

# Filter to specific tests
cru plugin test path/to/my-plugin --filter "tasks_list"

# Verbose output
cru plugin test path/to/my-plugin --verbose
```

### Assert API

The test runner provides a rich assertion library:

```lua
assert.equal(actual, expected)       -- Strict equality (==)
assert.deep_equal(actual, expected)  -- Deep table comparison
assert.truthy(value)                 -- Not nil and not false
assert.falsy(value)                  -- nil or false
assert.error(function()              -- Expects the function to throw
    error("boom")
end)
```

### Mocking Crucible APIs

Tests run in a sandbox where `cru.*` APIs are replaced with mocks. Use `test_mocks` to configure what the mocks return:

```lua
before_each(function()
    test_mocks.setup({
        kiln = {
            search = function(query)
                return {
                    { title = "Note 1", score = 0.9 },
                    { title = "Note 2", score = 0.7 },
                }
            end,
        },
        http = {
            get = function(url)
                return { status = 200, body = '{"ok": true}' }
            end,
        },
    })
end)

after_each(function()
    test_mocks.reset()
end)
```

After a test runs, you can inspect what the mocks recorded:

```lua
it("calls search with the right query", function()
    plugin.tools.my_search.fn({ query = "rust" })
    local calls = test_mocks.get_calls("kiln", "search")
    assert.equal(#calls, 1)
    assert.equal(calls[1][1], "rust")
end)
```

### Pending Tests

Mark tests you plan to write later with `pending`:

```lua
pending("should handle unicode task names")
```

These show up in the test output as skipped, not failed.

## Health Checks

Health checks let your plugin report its own status. They're useful for verifying that dependencies exist, APIs are reachable, and configuration is valid.

### Writing health.lua

Create a `health.lua` file in your plugin directory:

```lua
-- health.lua

local function check()
    cru.health.start("my-plugin")

    -- Verify required APIs
    if cru.kiln then
        cru.health.ok("Kiln API available")
    else
        cru.health.error("Kiln API missing", {
            "Ensure the plugin has 'kiln' in its capabilities",
        })
    end

    -- Check configuration
    local config = cru.config and cru.config.get("my-plugin")
    if config and config.api_key then
        cru.health.ok("API key configured")
    else
        cru.health.warn("No API key set", {
            "Set api_key in plugin config for full functionality",
        })
    end

    -- Informational
    cru.health.info("Using default cache size (100)")

    return cru.health.get_results()
end

return { check = check }
```

### Health API

Four reporting levels, each with an optional advice table:

| Function | Effect | Use For |
|----------|--------|---------|
| `cru.health.ok(msg)` | Pass | Confirming something works |
| `cru.health.warn(msg, advice?)` | Warning | Non-critical issues |
| `cru.health.error(msg, advice?)` | Fail (sets `healthy = false`) | Missing requirements |
| `cru.health.info(msg)` | Informational | Version info, config values |

### Running Health Checks

```bash
# Check a specific plugin
cru plugin health path/to/my-plugin

# Check all installed plugins
cru plugin health
```

The output groups results by plugin and highlights errors and warnings.

## Hot Reload

During development, you don't need to restart Crucible every time you change a plugin file.

### Manual Reload

From the TUI, use the `:reload` command:

```
:reload my-plugin    # Reload a specific plugin
:reload              # Reload all plugins
```

Crucible clears the plugin's module cache, re-reads the source files, and re-registers tools and hooks. If the reload fails (syntax error, missing dependency), the previous version stays active and you'll see an error notification.

### Automatic File Watching

Enable watch mode in `crucible.toml` to reload plugins whenever their files change on disk:

```toml
[plugins]
watch = true
```

With this enabled, saving a `.lua` or `.fnl` file inside any plugin directory triggers an automatic reload. Changes are debounced per-plugin, so rapid saves don't cause repeated reloads.

Watch mode pairs well with a split terminal: editor on one side, Crucible TUI on the other. Save your file, see the effect immediately.

## IDE Setup

Type-aware editors (VS Code, Neovim with lua-language-server, etc.) can provide autocompletion and diagnostics for the `cru.*` API if you generate stub files.

### Generating Stubs

```bash
# Generate to the default location (~/.config/crucible/stubs/)
cru plugin stubs

# Generate to a custom directory
cru plugin stubs --output ./my-stubs/
```

This creates a `cru.lua` stub file with type annotations for every module in the Crucible Lua API (`cru.kiln`, `cru.health`, `cru.shell`, etc.) and a `cru-docs.json` companion with documentation metadata.

### Configuring lua-language-server

Add a `.luarc.json` to your plugin directory (or your kiln root):

```json
{
    "workspace.library": [
        "~/.config/crucible/stubs"
    ],
    "runtime.version": "Lua 5.4",
    "diagnostics.globals": [
        "cru",
        "describe",
        "it",
        "before_each",
        "after_each",
        "pending",
        "test_mocks"
    ]
}
```

The `cru plugin new` scaffold command generates this file automatically. If you're adding it to an existing plugin, the key parts are:

- **workspace.library** points to wherever you generated stubs
- **diagnostics.globals** suppresses "undefined global" warnings for the test runner and `cru` API

After this, your editor should offer completions for `cru.kiln.search(`, `cru.health.ok(`, and all other API surfaces.

## Best Practices

1. **One concern per plugin** - Keep plugins focused
2. **Document with README.md** - Explain what it does and how to use it
3. **Use descriptive tool names** - `tasks_list` not `list`
4. **Handle errors gracefully** - Return error tables with helpful messages
5. **Provide param descriptions** - Help agents understand your tools
6. **Minimize shell usage** - Prefer Crucible APIs over shelling out
7. **Declare capabilities** - Only request what you need in manifest
8. **Write tests** - Use `describe`/`it` blocks in a `tests/` directory
9. **Add health checks** - Help users diagnose configuration problems
10. **Generate stubs** - Run `cru plugin stubs` for editor autocompletion

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
