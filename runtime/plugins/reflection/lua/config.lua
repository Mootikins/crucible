--- reflection configuration
---
--- Resolution order, highest priority first (Lua beats TOML, the Neovim
--- convention — the daemon seeds setup() with the TOML section at load, and
--- a user's later `setup{}` call, e.g. from ~/.config/crucible/init.lua
--- which runs after plugins load, overrides it):
---   1. values passed to `setup({...})` — last call wins per key.
---   2. `[plugins.reflection]` in config.toml, read through
---      `crucible.config.get("reflection.<key>")` — covers keys read before
---      any setup() call lands. Note this is NOT `cru.config[...]` —
---      `cru.config` is the *app* config store and holds only `get`/`set`
---      functions; it is not indexable by plugin name.
---   3. the `defaults` table below.
---   4. the caller's `fallback` argument, as a last resort for keys this
---      module does not declare.

local PLUGIN = "reflection"

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

--- Read `[plugins.reflection].<key>`. Returns nil when unset, and also when
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
    if configured[key] ~= nil then return configured[key] end
    local val = from_toml(key)
    if val ~= nil then return val end
    if defaults[key] ~= nil then return defaults[key] end
    return fallback
end

return M
