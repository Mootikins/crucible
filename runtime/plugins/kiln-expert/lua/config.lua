--- kiln-expert configuration
---
--- Resolution order, highest priority first:
---   1. `[plugins.kiln-expert]` in config.toml, read through
---      `crucible.config.get("kiln-expert.<key>")`. Note this is NOT
---      `cru.config[...]` — `cru.config` is the *app* config store and holds
---      only `get`/`set` functions; it is not indexable by plugin name.
---   2. values passed to `setup({...})` — the daemon calls it with the TOML
---      section at load time, and a user may call it again with more.
---   3. the `defaults` table below.
---   4. the caller's `fallback` argument, as a last resort for keys this
---      module does not declare.

local PLUGIN = "kiln-expert"

local M = {}

local defaults = {
    kilns = {},
    timeout = 30,
}

-- setup() values. Kept in their own table so the order above stays
-- observable — folding them into `defaults` would make the two layers
-- indistinguishable.
local configured = {}

--- Merge user-provided config into the setup layer.
--- Called by setup() in init.lua.
function M.init(cfg)
    if not cfg then return end
    for k, v in pairs(cfg) do
        configured[k] = v
    end
end

--- Read `[plugins.kiln-expert].<key>`. Returns nil when unset, and also when
--- `crucible.config` is absent — the plugin test runner has no daemon behind
--- it, and the spec-extraction sandbox stubs `crucible` with a metatable
--- (hence rawget, which sees through neither).
local function from_toml(key)
    local crucible = rawget(_G, "crucible")
    if type(crucible) ~= "table" then return nil end
    local cfg = rawget(crucible, "config")
    if type(cfg) ~= "table" then return nil end
    local get = rawget(cfg, "get")
    if type(get) ~= "function" then return nil end
    local ok, val = pcall(get, PLUGIN .. "." .. key)
    if ok then return val end
    return nil
end

function M.get(key, fallback)
    local val = from_toml(key)
    if val ~= nil then return val end
    if configured[key] ~= nil then return configured[key] end
    if defaults[key] ~= nil then return defaults[key] end
    return fallback
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
