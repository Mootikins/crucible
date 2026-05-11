--- session-digest configuration
---
--- Defaults match `plugin.yaml` config schema. Mirrors the kiln-expert
--- pattern: `setup()` merges user-provided values; reads at runtime fall
--- back through TOML (`cru.config["session-digest"]`) and then defaults.

local M = {}

local defaults = {
    enabled = true,
    model = nil, -- nil → caller picks cheapest available chat backend
    dedupe_threshold = 0.80,
    min_session_turns = 3,
    truncate_strategy = "last_n_turns",
    last_n_turns = 40,
}

--- Merge user-provided config into defaults.
--- Called by setup() in init.lua.
function M.init(cfg)
    if not cfg then return end
    for k, v in pairs(cfg) do
        defaults[k] = v
    end
end

--- Read a config value, preferring TOML/user setup over compiled defaults.
function M.get(key, fallback)
    local cfg = (cru and cru.config and cru.config["session-digest"]) or {}
    if cfg[key] ~= nil then return cfg[key] end
    if defaults[key] ~= nil then return defaults[key] end
    return fallback
end

--- Return the full effective config snapshot. Useful for tests + logging.
function M.snapshot()
    local out = {}
    for k, v in pairs(defaults) do out[k] = v end
    local cfg = (cru and cru.config and cru.config["session-digest"]) or {}
    for k, v in pairs(cfg) do out[k] = v end
    return out
end

--- Reset to defaults — test-only escape hatch so suites don't leak state
--- across test cases.
function M.reset()
    defaults = {
        enabled = true,
        model = nil,
        dedupe_threshold = 0.80,
        min_session_turns = 3,
        truncate_strategy = "last_n_turns",
        last_n_turns = 40,
    }
end

return M
