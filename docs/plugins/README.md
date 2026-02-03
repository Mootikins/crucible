# Plugin Examples

Example Lua plugins demonstrating tools and hooks for the Crucible plugin system.

## Installation

Plugins can be installed at three levels:

1. **Global personal** (applies to all kilns):
   ```bash
   cp *.lua ~/.config/crucible/plugins/
   ```

2. **Kiln personal** (kiln-specific, gitignored):
   ```bash
   cp *.lua KILN/.crucible/plugins/
   ```

3. **Kiln shared** (version-controlled with kiln):
   ```bash
   cp *.lua KILN/plugins/
   ```

Restart Crucible to load plugins, or wait for hot-reload if enabled.

## Plugin Structure

### Single-File Plugin

Most plugins are single `.lua` files with tool annotations:

```lua
--- Description of what this plugin does
-- @tool name="my_tool" description="Does something useful"
-- @param query string "Search query"
function my_tool(args)
    -- Implementation
    return { result = "success" }
end

--- Event handler example
-- @handler event="tool:after" pattern="*" priority=100
function on_tool_complete(ctx, event)
    cru.log("info", "Tool completed: " .. event.tool_name)
    return event
end
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
--- Tool description shown to agents
-- @tool name="my_tool" description="What this tool does"
-- @param query string "Search query to execute"
-- @param limit number "Maximum results (optional)"
function my_tool(args)
    local query = args.query
    local limit = args.limit or 10
    
    local results = cru.kiln.search(query)
    return {
        count = #results,
        items = results
    }
end
```

### Handler Template

```lua
--- Brief description
-- @handler event="tool:after" pattern="*" priority=100
function my_handler(ctx, event)
    -- Modify event.payload as needed
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

## Fennel Support

Crucible also supports Fennel (Lisp syntax that compiles to Lua):

```fennel
;; my-plugin.fnl
(fn my-tool [args]
  "Tool that does something"
  {:result (.. "Hello, " args.name)})

{:my_tool my-tool}
```

Place `.fnl` files in the same plugin directories.

## Documentation

- **Creating Plugins**: `/docs/Help/Extending/Creating Plugins.md`
- **Lua Configuration**: `/docs/Help/Lua/Configuration.md`
- **Event Hooks**: `/docs/Help/Extending/Event Hooks.md`
