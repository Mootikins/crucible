--!strict
--- consolidation configuration
---
--- Resolution order, highest priority first. Lua beats TOML, as in Neovim.
--- The host calls `setup(opts)` once, after ~/.config/crucible/init.lua
--- finishes, with the merged `opts`. A `require("consolidation").setup{}`
--- call in init.lua runs first, and the host's call then lays over it.
---   1. values passed to `setup({...})` — last call wins per key.
---   2. the `plugins.consolidation` config key, read through
---      `cru.plugin.config.get("consolidation.<key>")` — covers keys read
---      before any setup() call lands. Note this is NOT `cru.config[...]` —
---      `cru.config` is the *app* config store and holds only `get`/`set`
---      functions; it is not indexable by plugin name.
---   3. the `defaults` table below.
---   4. the caller's `fallback` argument, as a last resort for keys this
---      module does not declare.

local PLUGIN = "consolidation"

local M = {}

-- The keys and what each one sets are documented once, in
-- docs/Help/Lua/Configuration.md ("Configuring Plugins"). `kiln` and
-- `model` have no default, and `enabled` is off: a pass spends model calls
-- with no user present.
local defaults = {
    enabled = false,
    kiln = nil,
    model = nil,
    provider = nil,
    interval = 21600,
    max_problem = 5,
    max_clean = 3,
    min_turns = 2,
    session_chars = 15000,
    timeout = 240,
    rejection_memory = 20,
}

-- setup() values. Kept in their own table so the order above stays
-- observable — folding them into `defaults` would make the two layers
-- indistinguishable.
local configured = {}

--- Merge user-provided config into the setup layer.
--- Called by setup() in init.luau.
function M.init(cfg)
    if not cfg then return end
    for k, v in pairs(cfg) do
        configured[k] = v
    end
end

--- Read `[plugins.consolidation].<key>`. Returns nil when unset, and also
--- when `cru.plugin.config` is absent: the plugin test runner has no daemon
--- behind it. The rawget calls read the globals as they are, with no
--- metatable in the way.
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

--- Clear the setup layer. Tests only: the daemon never unconfigures a
--- plugin, but a suite that cannot clear setup() has order-dependent tests.
function M.reset()
    configured = {}
end

return M
