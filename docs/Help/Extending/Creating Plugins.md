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

Plugins are discovered from these directories (highest priority first):

| Location | Source | Use Case |
|----------|--------|----------|
| `CRUCIBLE_PLUGIN_PATH` dirs | EnvPath | Development, CI |
| `~/.config/crucible/plugins/` | User | Personal plugins |
| `<entry>/plugins/` for each `runtimepath` entry | Runtime | Opt-in extra trees |
| `$CRUCIBLE_RUNTIME/plugins/`, else exe-relative | Runtime | Bundled with Crucible |

Same-name plugins at higher priority shadow lower ones.

**Plugins are user-scoped.** Nothing loads from a kiln, project or workspace on
its own. Two reasons, and the second is the one that does not go away:

1. A plugin directory that auto-loaded on `cd` would turn `git clone` into
   arbitrary code execution inside a long-lived daemon shared by every session.
2. A plugin registers daemon-global handlers, tools and services into a VM no
   session owns. `RuntimeHandler` has no session, workspace or kiln dimension,
   so a plugin loaded "for" one workspace fires its `pre_tool_call` in every
   other workspace's sessions — and `pre_tool_call` can cancel or replace a
   tool call. There is also no unload-on-leave. A trust prompt answers "should
   this code run?"; it does not answer "which sessions does it apply to?", and
   that second question currently has no answer.

Until plugin state is session- or workspace-scoped, per-workspace loading stays
out. This is a stated constraint with a named precondition, not a deferral.

### Loading another tree deliberately

`runtimepath` is the opt-in. It is your own config naming the tree, so consent
is explicit and needs no prompt in a headless daemon:

```toml
# ~/.config/crucible/config.toml
runtimepath = ["~/kilns/work"]   # loads ~/kilns/work/plugins/
```

Entries **add to** the shipped runtime rather than replacing it, and they rank
above it, so a plugin there can shadow a bundled one by name. Everything a
`runtimepath` tree loads is still daemon-global — point 2 above applies
unchanged, which is why this is a deliberate act and not a default.

```
~/.config/crucible/plugins/
├── tasks/               # Directory plugin
│   ├── init.lua         # Main module
│   ├── lua/parser.lua   # Helper modules
│   └── plugin.yaml      # Manifest (optional)
└── quick-tag.lua        # Single-file plugin
```

All plugin directories are also added to Lua's `package.path`, so `require("tasks")` works from anywhere — your init.lua, other plugins, or the built-in defaults. Note the module name is the **directory name**, not `init`; that is what a plugin's own test suite must require too.

### What ships

`runtime/plugins/` in the repo is the bundled set, compiled into the binary and
extracted on first run. Every one of them loads **enabled by default**:

| Plugin | What it adds |
|--------|--------------|
| `daily-notes` | `daily_create`, `daily_open`, `daily_list`, `/daily` |
| `discord` | Discord gateway + REST integration |
| `graph-view` | `graph_links`, `graph_stats`, `/graph` (Fennel) |
| `oci` | Routes workspace tools into containers |
| `reflection` | Post-session retrospective notes |
| `review` | `review_*` tools over the attributed diff |
| `todo-list` | `tasks_list`, `tasks_add`, `tasks_complete`, `tasks_next`, `/tasks` |
| `web-search` | Search over a provider chain |
| `worktree` | Run a session against a git worktree |

Turn one off with `[plugins.<name>] enabled = false` in `config.toml`. That is
the only durable lever — editing the extracted `plugin.yaml` does not survive,
because the runtime tree is re-stamped from the binary whenever the build
changes.

Their **test suites are not extracted** (`plugins/*/tests/**` is excluded from
the embed), so `cru plugin test` against a bundled plugin on an installed
Crucible finds nothing. Run those from a checkout.

## The Setup Pattern

Plugins export a module table with an optional `setup()` function. Users configure plugins in their `init.lua`:

```lua
-- ~/.config/crucible/init.lua
require("reflection").setup({
  enabled = true,
  timeout = 60,
})
```

Bundled plugins (in `runtime/plugins/`) load with defaults automatically. Your `setup()` call overrides those defaults. To skip a bundled plugin entirely, don't call `require()` for it.

Configuration precedence, highest first — **Lua beats TOML**, the Neovim convention:

1. `setup({...})` calls — last call wins per key. The daemon evaluates `~/.config/crucible/init.lua` *after* plugins load, so your calls land after the TOML seed.
2. `[plugins.<name>]` in `config.toml` — the daemon passes this section to each plugin's `setup()` at load, so TOML is the base configuration.
3. The plugin's own declared defaults.

A broken init.lua is warned about and skipped (the daemon runs with TOML-only config); it never blocks startup.

A plugin's `setup()` merges user config into its defaults:

```lua
-- In your plugin's init.lua
local config = require("config")

return {
    name = "my-plugin",
    -- ... tools, commands, handlers ...

    setup = function(cfg)
        if cfg then config.init(cfg) end
    end,
}
```

```lua
-- In your plugin's lua/config.lua
local M = {}
local defaults = { timeout = 30, verbose = false }

function M.init(cfg)
    for k, v in pairs(cfg) do defaults[k] = v end
end

function M.get(key, fallback)
    local val = defaults[key]
    if val ~= nil then return val end
    return fallback
end

return M
```

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
-- ~/.config/crucible/plugins/greet.lua

return {
    name = "greet",
    tools = {
        greet = {
            desc = "Say hello to someone",
            params = {
                { name = "name", type = "string", desc = "Name to greet" },
            },
            fn = function(args)
                return { message = "Hello, " .. (args.name or "world") .. "!" }
            end,
        },
    },
}
```

This registers one tool. Agents can now call `greet`. (Doc-comment `@tool`
annotations appear in older examples; the daemon does not discover them from
plugins — the returned spec table is the contract.)

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

# Optional: declared capabilities — INFORMATIONAL. All plugins share one
# Lua VM, so per-plugin module gating is not enforced; treat this as
# documentation of what the plugin touches. Valid values: filesystem,
# network, shell, kiln, agent, ui, config, system, websocket. An invalid
# value fails manifest parsing and the plugin never loads.
capabilities:
  - filesystem
  - kiln
```

See [[Help/Extending/Plugin Manifest]] for the complete manifest specification.

```lua
-- init.lua - Main module: return the plugin spec table

local parser = require("parser")
local commands = require("commands")

return {
    name = "tasks",
    tools = {
        tasks_list = {
            desc = "List all tasks",
            params = { { name = "path", type = "string", desc = "Path to TASKS.md" } },
            fn = function(args)
                return commands.list_tasks(parser.parse_tasks(args.path))
            end,
        },
        tasks_next = {
            desc = "Get the next available task",
            params = { { name = "path", type = "string", desc = "Path to TASKS.md" } },
            fn = function(args)
                return commands.next_task(parser.parse_tasks(args.path))
            end,
        },
    },
}
```

## Providing Tools

Declare tools in the spec table your `init.lua` returns:

```lua
tools = {
    search_notes = {
        desc = "Search notes by content",
        params = {
            { name = "query", type = "string", desc = "Search query" },
            { name = "limit", type = "number", desc = "Maximum results", optional = true },
        },
        fn = function(args)
            return { results = cru.kiln.search(args.query, { limit = args.limit or 10 }) }
        end,
    },
}
```

Tools register when the plugin loads; a name colliding with a built-in tool
is rejected, not shadowed.

## Providing Hooks

Register handlers with `crucible.on()` at the top level of your `init.lua` —
registration happens once at plugin load, and each handler resolves its
session via `ctx.session_id`:

```lua
-- Log all tool calls
crucible.on("pre_tool_call", function(ctx, event)
    cru.log("info", "Tool called: " .. event.tool)
end)

-- Block dangerous operations
crucible.on("pre_tool_call", { pattern = "*delete*", priority = 5 }, function(ctx, event)
    return { cancel = true, reason = "Deletes are blocked" }
end)
```

(`@handler` doc-comment annotations and the spec-table `handlers` field
appear in older material; neither is dispatched for plugins — `crucible.on`
is the contract. Declaring spec-table handlers logs a warning at load.)

See [[Help/Extending/Event Hooks]] for event types, return values, and patterns.

## Hot Reload

`cru plugin reload <name>` (TUI `:reload <name>`) re-executes a plugin's
`init.lua`, replacing its tools, commands, and handlers. To reload
automatically when plugin files change on disk, enable the watcher:

```toml
[plugins]
watch = true
```

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
tools = {
    run_tests = {
        desc = "Run the test suite",
        fn = function(args)
            local result = cru.shell.exec("cargo", { "test" })
            return { stdout = result.stdout, exit_code = result.exit_code }
        end,
    },
}
```

### Security Model

Shell commands are **deny by default**. Commands must be whitelisted at the workspace or global level to execute.

When a plugin tries a non-whitelisted command, the user is prompted to allow or deny it, with options to save the decision.

Common commands (`git`, `cargo`, `npm`, `docker`, etc.) are whitelisted by default.

### Project Shell Policy

```toml title=".crucible/project.toml"
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
;; ~/.config/crucible/plugins/greet.fnl

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
commands = {
    tasks = {
        desc = "Manage tasks",
        hint = "[add|list|done] <args>",
        fn = function(args)
            return "tasks: " .. (args and args.input or "list")
        end,
    },
}
```

A command's `fn` receives the argument table and returns any
JSON-representable value; the TUI shows it as a system message. Commands
surface as `/name` with autocomplete (tagged `(plugin)`).

## Providing Views

> **Not yet consumed.** Spec-table `views` are parsed and counted but no
> client renders them — plugin-declared UI is the next arc (a declarative
> slot vocabulary shared by TUI and web). Declaring views today does
> nothing beyond the count in `plugin.list`.

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

Load the plugin under test the way the daemon does: by its **directory name**,
not by `init`. The runner's `package.path` mirrors the loader exactly
(`<plugins-parent>/?/init.lua`, plus the plugin's own `lua/?.lua`), so
`require("init")` resolves nothing — a suite written that way fails to load
rather than failing an assertion.

```lua
-- tests/init_test.lua   (in a plugin directory named `tasks/`)

describe("tasks_list", function()
    local plugin = require("tasks")

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

Enable watch mode in `config.toml` to reload plugins whenever their files change on disk:

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
