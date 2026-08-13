--- {{name}} Plugin
--- A starter plugin template for Crucible

local M = {}

--- Greet someone.
---
--- Tools are declared in the spec table returned at the bottom of this file.
--- Doc-comment annotations of the "at-tool" / "at-param" kind are NOT
--- discovered: `parse_tools` has no caller on any live path, and a plugin that
--- returns a spec table has its exports read from that table and nothing else.
--- A tool declared only by annotation never reaches an agent.
function M.greet(args)
    local name = args.name or "World"
    local greeting = args.greeting or "Hello"

    return {
        message = greeting .. ", " .. name .. "!",
        timestamp = os.time(),
    }
end

--- Lifecycle hooks register by CALLING an api at load time. A `hooks = {...}`
--- field in the spec table below would be silently ignored: the recognised
--- fields are name, version, tools, commands, handlers, views and setup.
---
--- `crucible.on(event, opts, handler)` is the other half of this — it takes one
--- of eleven event names (`pre_tool_call`, `tool_result`, …) and can cancel or
--- replace a tool call. See docs/Help/Extending/Event Hooks.md.
crucible.on_session_start(function(session)
    cru.log("info", "{{name}} plugin loaded for session: " .. session.id)
end)

-- Return the plugin spec
return {
    name = "{{name}}",
    version = "0.1.0",
    description = "A Crucible plugin",

    --- Called at load with this plugin's `[plugins.{{name}}]` config section,
    --- or an empty table when there is none. Delete if you read no config.
    setup = function(cfg) end,

    -- Tools exported to agents
    tools = {
        greet = {
            desc = "Greet someone with a message",
            params = {
                { name = "name", type = "string", desc = "The name to greet" },
                { name = "greeting", type = "string", desc = "Custom greeting (optional)", optional = true },
            },
            fn = M.greet,
        },
    },
}
