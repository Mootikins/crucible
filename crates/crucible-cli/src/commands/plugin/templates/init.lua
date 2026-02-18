--- {{name}} Plugin
--- A starter plugin template for Crucible

local M = {}

--- Example tool that demonstrates the @tool annotation format
-- @tool name="greet" description="Greet someone with a message"
-- @param name string "The name to greet"
-- @param greeting string "Custom greeting (optional)"
function M.greet(args)
    local name = args.name or "World"
    local greeting = args.greeting or "Hello"
    
    return {
        message = greeting .. ", " .. name .. "!",
        timestamp = os.time()
    }
end

--- Called when a session starts
-- This hook runs once when the plugin is loaded in a session
local function on_session_start(session)
    cru.log("info", "{{name}} plugin loaded for session: " .. session.id)
end

-- Return the plugin spec
return {
    name = "{{name}}",
    version = "0.1.0",
    description = "A Crucible plugin",
    
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
    
    -- Hooks for lifecycle events
    hooks = {
        on_session_start = on_session_start,
    },
}
