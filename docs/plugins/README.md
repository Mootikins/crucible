---
title: plugins
description: Plugin Examples
tags:
  - plugins
---

# Plugin Examples

> **Example status:** the plugins under `docs/plugins/` (daily-notes,
> todo-list) are documentation examples. Their test files are NOT run by CI —
> only `runtime/plugins/` suites are gated (`shipped_plugin_lua_suite_passes`).

Example Lua plugins demonstrating tools and hooks for the Crucible plugin system.

## Installation

Install plugins into the global plugin directory:

```bash
cp -r my-plugin/ ~/.config/crucible/plugins/
```

(Kiln-local plugin directories are deliberately not loaded — see
[[Help/Extending/Creating Plugins]] for why.) Restart the daemon to load, or
enable `[plugins] watch = true` for hot-reload; `cru plugin reload <name>`
reloads one on demand.

## Plugin Structure

### Single-File Plugin

A single `.lua` file returns a spec table; handlers register with
`crucible.on` at load:

```lua
crucible.on("pre_tool_call", function(ctx, event)
    cru.log("info", "Tool called: " .. event.tool)
end)

return {
    name = "my-plugin",
    tools = {
        my_tool = {
            desc = "Does something useful",
            params = { { name = "query", type = "string", desc = "Search query" } },
            fn = function(args)
                return { result = "success" }
            end,
        },
    },
}
```

### Module Plugin

For complex plugins, use a directory with `init.lua`:

```
my_plugin/
├── init.lua     # Entry point
├── helpers.lua  # Helper module
└── types.lua    # Type definitions
```

## Writing Plugins

### Tool Template

```lua
tools = {
    my_tool = {
        desc = "What this tool does",
        params = {
            { name = "query", type = "string", desc = "Search query to execute" },
            { name = "limit", type = "number", desc = "Maximum results", optional = true },
        },
        fn = function(args)
            local results = cru.kiln.search(args.query)
            return { count = #results, items = results }
        end,
    },
}
```

### Handler Template

```lua
--- Brief description
-- @handler event="tool:after" pattern="*" priority=100
function my_handler(ctx, event)
    -- `event` is one flat table: payload fields at the top level alongside
    -- event.type. Modify fields as needed.
    -- Use ctx:set/get for cross-handler state
    -- Use ctx:emit for new events
    return event  -- Must return event
end
```

### Handler Patterns

```lua
-- Match specific tools
-- @handler event="tool:after" pattern="search_*" priority=10

-- Match all tools
-- @handler event="tool:after" pattern="*" priority=50

-- Very early processing (validation, security)
-- @handler event="tool:before" pattern="*" priority=5

-- Very late processing (audit, logging)
-- @handler event="tool:after" pattern="*" priority=200
```

### Priority Levels

Lower numbers run earlier:
- `priority = 5` - Very early (validation, security)
- `priority = 10` - Early (filtering, enrichment)
- `priority = 50` - Normal (transformation)
- `priority = 100` - Late (default)
- `priority = 200` - Very late (audit, logging)

## Testing

Each example plugin includes a `tests/` directory with test files. Run them with:

```bash
# Test a specific plugin
cru plugin test docs/plugins/todo-list

# Test with verbose output
cru plugin test docs/plugins/todo-list --verbose
```

### Test File Structure

Tests use `describe`/`it` blocks with a built-in assertion library:

```lua
-- tests/init_test.lua

describe("my_tool", function()
    before_each(function()
        test_mocks.setup({
            kiln = { search = function() return {} end },
        })
    end)

    after_each(function()
        test_mocks.reset()
    end)

    it("returns expected result", function()
        local plugin = require("init")
        local result = plugin.tools.my_tool.fn({ query = "test" })
        assert.equal(result.count, 0)
    end)
end)
```

### Mocking

The `test_mocks` global lets you stub `cru.*` APIs so tests don't need a running Crucible instance:

```lua
test_mocks.setup({
    kiln = {
        search = function(query)
            return {{ title = "Mock Note", score = 0.95 }}
        end,
    },
})
```

Call `test_mocks.reset()` in `after_each` to clean up between tests.

## Health Checks

Plugins can include a `health.lua` file that reports diagnostic information. This helps users verify that a plugin's dependencies and configuration are correct.

```lua
-- health.lua

local function check()
    cru.health.start("my-plugin")

    if cru.kiln then
        cru.health.ok("Kiln API available")
    else
        cru.health.error("Kiln API missing", {
            "Add 'kiln' to capabilities in plugin.yaml",
        })
    end

    cru.health.info("Version 1.0.0")
    return cru.health.get_results()
end

return { check = check }
```

Run health checks with:

```bash
cru plugin health docs/plugins/todo-list
```

## Fennel Support

Crucible also supports Fennel (Lisp syntax that compiles to Lua):

```fennel
;; my-plugin.fnl
(fn my-tool [args]
  "Tool that does something"
  {:result (.. "Hello, " args.name)})

{:my_tool my-tool}
```

Place `.fnl` files in the same plugin directories. Fennel test files (`*_test.fnl`) are also supported by the test runner.

## Documentation

- **Creating Plugins**: `/docs/Help/Extending/Creating Plugins.md`
- **Lua Configuration**: `/docs/Help/Lua/Configuration.md`
- **Event Hooks**: `/docs/Help/Extending/Event Hooks.md`
