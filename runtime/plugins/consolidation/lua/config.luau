--!strict
--- consolidation configuration
---
--- Resolution order, highest priority first (Lua beats TOML, the Neovim
--- convention — the daemon seeds setup() with the TOML section at load, and
--- a user's later `setup{}` call, e.g. from ~/.config/crucible/init.lua
--- which runs after plugins load, overrides it):
---   1. values passed to `setup({...})` — last call wins per key.
---   2. `[plugins.consolidation]` in config.toml, read through
---      `cru.plugin.config.get("consolidation.<key>")` — covers keys read
---      before any setup() call lands. Note this is NOT `cru.config[...]` —
---      `cru.config` is the *app* config store and holds only `get`/`set`
---      functions; it is not indexable by plugin name.
---   3. the `defaults` table below.
---   4. the caller's `fallback` argument, as a last resort for keys this
---      module does not declare.

local PLUGIN = "consolidation"

local M = {}

local defaults = {
    -- Master switch. A pass spends model calls with no user present,
    -- so a user must turn it on.
    enabled = false,
    -- The kiln the pass reads and stages proposals in. The pass has no
    -- session of its own, so nothing else names a kiln for it.
    kiln = nil,
    -- Auxiliary model the consolidation reviewer runs on. Keep this cheap:
    -- it reads several transcripts at once.
    model = nil,
    -- Optional provider override (e.g. "anthropic", "openai"). When nil the
    -- daemon resolves the provider from the model / global config.
    provider = nil,
    -- Seconds between passes. Six hours.
    interval = 21600,
    -- Most sessions with tool errors or rejected edits in one pass.
    max_problem = 5,
    -- Most clean sessions in one pass.
    max_clean = 3,
    -- Fewest user turns a session needs. The reflection reviewer's own aux
    -- sessions have one user turn, so this keeps them out of the sample.
    min_turns = 2,
    -- Cut each session's transcript at this many characters, from the front.
    session_chars = 15000,
    -- Seconds to wait for the consolidation reviewer. It reads more than the
    -- reflection reviewer does, so it gets more time.
    timeout = 240,
    -- Upper bound on the reviewer's tool loop. The reviewer searches the
    -- kiln before it proposes, so it needs more than three turns.
    max_iterations = 12,
    -- How many recent rejections the reviewer is told about, so it does not
    -- propose a note a human already refused.
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
--- when `cru.plugin.config` is absent — the plugin test runner has no daemon
--- behind it, and the spec-extraction sandbox stubs `crucible` with a
--- metatable (hence rawget, which sees through neither).
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
