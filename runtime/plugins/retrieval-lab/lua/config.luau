--!strict
--- retrieval-lab configuration
---
--- Resolution order, highest priority first (Lua beats TOML, the Neovim
--- convention — the daemon seeds setup() with the TOML section at load, and
--- a user's later `setup{}` call overrides it):
---   1. values passed to `setup({...})` — last call wins per key.
---   2. `[plugins.retrieval-lab]` in config.toml, read through
---      `cru.plugin.config.get("retrieval-lab.<key>")`.
---   3. the `defaults` table below.
---   4. the caller's `fallback` argument.

local PLUGIN = "retrieval-lab"

local M = {}

-- The keys mirror plugin.yaml. Every embedded content block joins a pair.
-- A heading is a label, and the few long enough to embed cost the
-- adversarial corpus without a gain (lab runs 3 and 4). `transition` is
-- excluded so that a synthetic row an earlier `arc_pre` index wrote never
-- counts as a neighbour.
local defaults = {
    enabled = false,
    strategy = "points",
    pair_cosine_min = 0.5,
    curve_window = 3,
    curve_samples = 8,
    exclude_kinds = { "heading", "transition" },
    -- meta_pre: `{meta}` is the title, the heading and the description;
    -- `{text}` is the block's own text.
    meta_format = "{meta}\n\n{text}",
    -- ppr_post: the teleport mass of a hit note is its best dense score
    -- times the seed weight. The rest of the mass teleports uniformly.
    ppr_damping = 0.5,
    ppr_blend = 0.1,
    ppr_extra = 3,
    -- slate_post: the quota plan, in order, then the rest by point score.
    slate = { { type = "point", n = 2 }, { type = "arc", n = 2 }, { type = "hop", n = 2 } },
    hop_from = 2,
}

-- setup() values, kept apart from `defaults` so the order above stays
-- observable.
local configured = {}

--- Merge user-provided config into the setup layer.
function M.init(cfg)
    if not cfg then return end
    for k, v in pairs(cfg) do
        configured[k] = v
    end
end

--- Read `[plugins.retrieval-lab].<key>`. Returns nil when unset, and also when
--- `cru.plugin.config` is absent — the plugin test runner has no daemon behind
--- it, and the spec-extraction sandbox stubs `cru` with a metatable (hence
--- rawget, which sees through neither).
local function from_toml(key)
    local cru_table = rawget(_G, "cru")
    if type(cru_table) ~= "table" then return nil end
    local plugin = rawget(cru_table, "plugin")
    if type(plugin) ~= "table" then return nil end
    local cfg = rawget(plugin, "config")
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

--- Clear the setup layer. Tests only.
function M.reset()
    configured = {}
end

return M
