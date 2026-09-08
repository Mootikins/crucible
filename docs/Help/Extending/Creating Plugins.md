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

A plugin's own directory is also a runtime root, so a plugin may ship its own
`skills/`, `agents/` and `themes/` beside its manifest — they are found with no
registration. Crucible's own `crucible-help` plugin ships the documentation
this way. A plugin's contributions rank below every root you named yourself, so
they never shadow your own.

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
extracted on first run. Every one of them loads **enabled by default**, except
`consolidation`:

| Plugin | What it adds |
|--------|--------------|
| `auto-title` | Names a session after its opening exchange |
| `consolidation` | Periodic pass that proposes pattern notes; off until `[plugins.consolidation] enabled = true` |
| `daily-notes` | `daily_create`, `daily_open`, `daily_list`, `/daily` |
| `discord` | Discord gateway + REST integration |
| `oci` | Routes workspace tools into containers |
| `reflection` | Post-session review that proposes notes, note updates and skills |
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

## Plugin Language

Plugins are written in Luau, in `.luau` or `.lua` files. `cru plugin new`
writes `.luau` — it is what Luau's own editor tooling recognises — and `.lua`
keeps working for good, so every plugin already on disk is unaffected.

A directory holding both `init.luau` and `init.lua` is refused rather than
resolved: an edit to the wrong one would appear to do nothing. The refusal is
reported by discovery and by `cru plugin check`, naming both files.

**You do not have to annotate anything.** Write plain Lua and the checker
already catches a misspelled namespace, a wrong argument count and a wrong
argument type on all 182 `cru.*` functions, and your editor completes them.
That comes from the generated declarations, not from anything in your file:
a plain file and a `--!strict` file produce identical diagnostics for host API
misuse.

`--!strict` adds checks on **your own** code — your locals, your tables, your
own functions. It is worth turning on, and it is a choice. Every plugin
Crucible ships carries it; see [[Help/Lua/Language Basics]] for what it costs
and the idioms that make it painless.

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
description: Task management tools
author: Your Name

# Optional: declare dependencies. Matched by NAME only — a `version:`
# constraint here is parsed but never checked, so don't write one.
dependencies:
  - name: core-utils

# Optional: declare that this plugin takes tool calls over.
#
# The ONE declaration the host checks. It is not a sandbox claim — plugin Lua
# runs in the daemon VM with `io` and `os`, so installing a plugin is the real
# trust decision. It is a COMPOSITION claim: a handler returning
# `handled = true` takes another component's tool call and returns BEFORE the
# permission gate. Without this the daemon ignores the takeover and dispatches
# normally, and logs that it did.
#
# It replaced a ten-name `capabilities:` list of which nine names were never
# checked anywhere and could not have been.
intercept_tools: true
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
            local kiln = cru.kiln.active
            if not kiln then return {} end
            local vector = cru.embed(kiln, args.query)
            return { results = cru.kiln.search(kiln, vector, args.limit or 10) }
        end,
    },
}
```

Tools register when the plugin loads; a name colliding with a built-in tool
is rejected, not shadowed.

## Providing Hooks

Register handlers with `cru.on()` at the top level of your `init.lua` —
registration happens once at plugin load, and each handler resolves its
session via `ctx.session_id`:

```lua
-- Log all tool calls
cru.on("pre_tool_call", function(ctx, event)
    cru.log("info", "Tool called: " .. event.tool)
end)

-- Block dangerous operations
cru.on("pre_tool_call", { pattern = "*delete*", priority = 5 }, function(ctx, event)
    return { cancel = true, reason = "Deletes are blocked" }
end)
```

(`@handler` doc-comment annotations and the spec-table `handlers` field
appear in older material; neither is dispatched for plugins — `cru.on`
is the contract. Declaring spec-table handlers logs a warning at load.)

See [[Help/Extending/Event Hooks]] for event types, return values, and patterns.

## Providing Services

A service is a long-running background task — a gateway connection, a poll
loop. Declare it in the spec table; the daemon spawns each declared service
as an independent async task when the plugin loads:

```lua
services = {
    gateway = {
        desc = "WebSocket gateway connection",
        fn = function()
            while true do
                poll_upstream()
                cru.timer.sleep(30)
            end
        end,
    },
}
```

**Cancellation contract:** when your plugin is reloaded, disabled, or
removed, its running service tasks are **aborted at their next `await`
point** — there is no stop callback and no drain period. Write services
cancel-safe: do work in idempotent steps, hold no state that must be
flushed on exit, and let external resources (sockets, subprocesses) be
closed by drop. The old generation's abort is requested before a reload
spawns the new one and takes effect at its next `await` point, so
generations never accumulate — but a task deep in synchronous work may
overlap the new generation briefly before it lands.

## Hot Reload

TUI `:reload <name>` (or the `plugin.reload` RPC — there is no `cru plugin
reload` CLI subcommand) re-executes a plugin's
`init.lua`, replacing its tools, commands, and handlers, and aborting its
running service tasks before the new generation spawns. Reloading a plugin
that fails to execute returns an error and leaves the plugin fully inert —
see [[#Lifecycle States]]. To reload automatically when plugin files change
on disk, enable the watcher:

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
| `Error` | Failed to load or execute. Guaranteed inert: nothing of the plugin's is registered or running. `plugin.list` still shows what it declares, plus `last_error` |

### Lifecycle Callbacks: `on_load` / `on_unload`

The spec table may carry two optional lifecycle functions:

```lua
return {
    name = "my-plugin",
    on_load = function()
        cru.log("info", "my-plugin loaded")
    end,
    on_unload = function()
        -- flush state, drop caches
    end,
    -- ... tools, commands, services ...
}
```

`on_load` runs when the plugin is loaded, `on_unload` when it is unloaded — a
reload runs `on_unload` for the old generation, then `on_load` for the new.
Both are called with no arguments; an error in either is logged (and recorded
in the plugin's error log) but does not abort the load or unload. For
per-*session* work, use `cru.on_session_start` / `on_session_end`
instead — see [[Help/Extending/Event Hooks]].

## Shell Commands

Plugins can execute shell commands using `cru.shell.exec()` (the module itself
is not callable):

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
local result = cru.shell.exec("cargo", {"build"}, {
    cwd = "/path/to/project",      -- Working directory
    env = { RUST_LOG = "debug" },  -- Environment variables
    stdin = "input data",          -- Data piped to stdin (optional)
})

-- result.success, result.stdout, result.stderr, result.exit_code
```

There is no timeout by default — commands run to completion, so builds and
long-running processes are never silently killed. Pass `timeout` (in SECONDS)
in the options table for a per-call deadline. A shell policy may set its own,
and the shorter of the two wins: a plugin may shorten its deadline and may not
lengthen the sandbox's.

## Typed Plugins

A plugin declares types where they earn their keep — the tool arguments and
the values a tool returns:

```lua
--!strict
-- ~/.config/crucible/plugins/greet.lua

type GreetArgs = { name: string }

local function greet(args: GreetArgs): { message: string }
    return { message = "Hello, " .. args.name .. "!" }
end

return {
    name = "greet",
    version = "0.1.0",
    tools = {
        greet = {
            desc = "A friendly greeting tool",
            params = { { name = "name", type = "string", desc = "who to greet" } },
            fn = greet,
        },
    },
}
```

`cru plugin check <dir>` typechecks the plugin against the generated `cru.*`
declarations, so a call with the wrong argument type or a tool returning the
wrong shape is a build failure rather than a runtime surprise. `cru plugin
stubs` writes those declarations; add `--offline` to build them from your
working tree rather than from a running daemon. When no generated set exists,
`cru plugin check` builds one into a temporary directory for the check, so the
command needs no setup step.

Pass `--definitions <file>` to check against a set you name.

The checker is `luau-lsp`. `cru plugin check` looks in three places, in this
order:

1. `CRUCIBLE_LUAU_ANALYZE`, for a binary under another name or outside PATH.
   A path that names no file is a failure, not a skip.
2. `target/tools/luau-lsp` beside the running `cru`. This is the pinned build
   that `just luau-lsp` fetches, so a `cru` built from the checkout needs no
   install. The location is read relative to the BINARY, never to the
   directory you run in: a check you point at code you downloaded must not
   take its type checker from that code.
3. `luau-lsp` on PATH, then `luau-analyze`.

The command prints which one it used. The daemon's typecheck gates read the
same three places, so the command and the gates agree about what checked what.

Whichever answers must PROVE it checks types before the check trusts it: it is
handed a file that assigns a string to a `number` and must complain. A binary
that runs and reports nothing is refused by name rather than reported as a
passing typecheck.

Without a checker, `cru plugin check` still proves that every file parses and
every declared tool parameter type is readable, and reports the typecheck as
SKIPPED rather than as a pass.

### What the types do not catch

A misspelled key in an **all-optional** options table. Luau reads
`cru.oil.text("x", { bld = true })` against `{ bold: boolean? }` as a table
that omits every field, which is legal, so nothing reports it. A typo in a
**required** field is caught, because the field then reads as missing. Nearly
every `cru.*` options table is all-optional, so treat an option name as
something to check by reading, the way you would without types.

Two other things no type states: a unit (`cru.timer.sleep` takes SECONDS —
the parameter name is the only thing that says so), and whether a function
raises rather than returning nil.

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
    return oil.col(
        oil.text("Graph View", { bold = true }),
        oil.divider(),
        oil.text("Nodes: 42, Edges: 128")
    )
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
            kiln = { notes = {} },
        })
    end)

    after_each(function()
        test_mocks.reset()
    end)

    it("returns empty list when no tasks exist", function()
        local result = plugin.tools.tasks_list.fn({ file = "nonexistent.md" })
        expect.equal(0, result.count)
    end)

    it("filters completed tasks when show_completed is false", function()
        local result = plugin.tools.tasks_list.fn({
            file = "TASKS.md",
            show_completed = false,
        })
        expect.equal("table", type(result.tasks))
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

The test runner provides a rich assertion library. **Expected value comes
first** — failures report `Expected: <first>` / `Actual: <second>`:

```lua
expect.equal(expected, actual)       -- Strict equality (==); alias: expect.equals
expect.deep_equal(expected, actual)  -- Deep table comparison
expect.truthy(value)                 -- Not nil and not false
expect.falsy(value)                  -- nil or false
expect.has_error(function()          -- Expects the function to throw
    error("boom")
end)
```

### Mocking Crucible APIs

Tests run in a sandbox where `cru.*` APIs are replaced with mocks. `test_mocks.setup()` takes **data fixtures**, not replacement functions — the mock implementations are fixed, and your overrides feed them the data they answer with:

```lua
before_each(function()
    test_mocks.setup({
        kiln = {
            -- kiln.search/get/list answer from these notes
            notes = {
                { path = "note1.md", title = "Note 1", content = "rust things" },
                { path = "note2.md", title = "Note 2", content = "more rust" },
            },
        },
        http = {
            -- http.get/post/... answer from responses keyed by URL
            responses = {
                ["https://api.example.com/data"] = { status = 200, body = '{"ok": true}' },
            },
        },
        fs = {
            files = { ["config.toml"] = "key = 'value'" },
        },
    })
end)

after_each(function()
    test_mocks.reset()
end)
```

Mockable modules and their fixture keys: `kiln`/`graph` (`notes`, `outlinks`,
`backlinks`, `neighbors`), `http` (`responses`), `fs` (`files`, `dirs`),
`paths` (`kiln`, `workspace`, `session`, `state` — set one to `false` to make
its accessor raise "not configured"), `session` (`temperature`, `model`,
`mode`, ...), and `sessions` (`info`, `messages`, `response_parts`).

After a test runs, you can inspect what the mocks recorded:

```lua
it("calls search with the right query", function()
    plugin.tools.my_search.fn({ query = "rust" })
    local calls = test_mocks.get_calls("kiln", "search")
    expect.equal(1, #calls)
    expect.equal("rust", calls[1][1])
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
            "Check the plugin loaded: `cru doctor`",
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

Crucible clears the plugin's module cache, re-reads the source files, and re-registers tools and hooks. If the reload fails (syntax error, a raising `setup()`, missing dependency), the reload reports the error and the plugin ends up **inert** in state `Error`: none of its tools, commands, handlers, or option declarations stay registered, and its service tasks are aborted. The previous version does not keep running — fix the file and reload again.

### Automatic File Watching

Enable watch mode in `config.toml` to reload plugins whenever their files change on disk:

```toml
[plugins]
watch = true
```

With this enabled, saving a `.lua` file inside any plugin directory triggers an automatic reload. Changes are debounced per-plugin, so rapid saves don't cause repeated reloads.

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
    "runtime.version": "Lua 5.1",
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
