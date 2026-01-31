--- Hermit configuration helpers
--- Reads from cru.config.hermit with sensible defaults

local M = {}

local defaults = {
    soul_file = "",
    auto_link = true,
    auto_digest = false,
    awareness_cache_ttl = 300,
    quiet_mode = false,
    enabled_reactions = "all",
}

function M.get(key, fallback)
    local cfg = cru.config and cru.config.hermit or {}
    local val = cfg[key]
    if val ~= nil then
        return val
    end
    if fallback ~= nil then
        return fallback
    end
    return defaults[key]
end

function M.reaction_enabled(name)
    local reactions = M.get("enabled_reactions", "all")
    if reactions == "all" then
        return true
    end
    if reactions == "none" then
        return false
    end
    for entry in reactions:gmatch("[^,]+") do
        if entry:match("^%s*(.-)%s*$") == name then
            return true
        end
    end
    return false
end

function M.quiet()
    return M.get("quiet_mode", false)
end

return M
