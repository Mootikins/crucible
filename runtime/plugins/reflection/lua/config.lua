--- reflection configuration
--- Reads from cru.config["reflection"], falling back to setup() defaults.

local M = {}

local defaults = {
    -- Master switch. Reflection only runs when true.
    enabled = true,
    -- Auxiliary model the reflection subagent runs on. Keep this cheap — it
    -- reviews the whole transcript and should never burden the main session.
    model = nil,
    -- Optional provider override (e.g. "anthropic", "openai"). When nil the
    -- daemon resolves the provider from the model / global config.
    provider = nil,
    -- Skip reflection on trivial sessions: require at least this many user
    -- turns before a pass is worthwhile.
    min_turns = 3,
    -- Cap how many proposals a single session may stage.
    max_proposals = 5,
    -- Seconds to wait for the reflection subagent to respond.
    timeout = 120,
}

--- Merge user-provided config into defaults. Called by setup() in init.lua.
function M.init(cfg)
    if not cfg then return end
    for k, v in pairs(cfg) do
        defaults[k] = v
    end
end

function M.get(key, fallback)
    local cfg = cru.config and cru.config["reflection"] or {}
    local val = cfg[key]
    if val ~= nil then return val end
    if fallback ~= nil then return fallback end
    return defaults[key]
end

return M
