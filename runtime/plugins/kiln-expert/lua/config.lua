--- kiln-expert configuration
--- Reads from cru.config["kiln-expert"]

local M = {}

local defaults = {
    kilns = {},
    timeout = 30,
}

--- Merge user-provided config into defaults.
--- Called by setup() in init.lua.
function M.init(cfg)
    if not cfg then return end
    for k, v in pairs(cfg) do
        defaults[k] = v
    end
end

function M.get(key, fallback)
    local cfg = cru.config and cru.config["kiln-expert"] or {}
    local val = cfg[key]
    if val ~= nil then return val end
    if fallback ~= nil then return fallback end
    return defaults[key]
end

--- Get the configured kilns table { label = path }
function M.kilns()
    return M.get("kilns", {})
end

--- Resolve a label to a kiln path, or nil
function M.resolve(label)
    local kilns = M.kilns()
    return kilns[label]
end

--- List available kiln labels
function M.labels()
    local result = {}
    for label, _ in pairs(M.kilns()) do
        result[#result + 1] = label
    end
    table.sort(result)
    return result
end

return M
