--- web-search configuration.
---
--- Resolution order, highest priority first (Lua beats TOML, the Neovim
--- convention the daemon already implements — it seeds setup() with the TOML
--- section at load, and a later `require("web-search").setup{}` from the user's
--- init.lua lands afterwards and wins):
---
---   0. `$CRUCIBLE_WEB_SEARCH_<KEY>` — declared secrets only.
---   1. values passed to `setup({...})` — last call wins per key.
---   2. `[plugins.web-search]` in config.toml, via
---      `crucible.config.get("web-search.<key>")`.
---   3. the `defaults` table below.
---   4. the caller's `fallback` argument, for keys this module does not declare.
---
--- Two things about that order are load-bearing:
---
--- The env step is **implemented here, not inherited**. `schema.secret`
--- resolution lives in `cru.service.define` (lua_stdlib/stdlib.rs:255) and runs
--- only for services; `PluginManifest` has no `config` field at all, so a
--- plugin.yaml `config:` block is documentation that nothing parses. The env
--- var name deliberately matches the service convention so one rule covers
--- both surfaces.
---
--- Secrets are read from the environment **before** setup() and TOML, which
--- inverts the order used for everything else. That is the point: it is what
--- lets a user keep a key out of a config file that gets committed, and a key
--- written into config.toml anyway still works as the lower-priority fallback.

local PLUGIN = "web-search"
local ENV_PREFIX = "CRUCIBLE_WEB_SEARCH_"

local M = {}

local defaults = {
    -- Ships as searxng-then-ddg: SearXNG first because a user who hosts one
    -- gets better results and keeps their queries in-house, ddg second because
    -- it is the only keyless provider that needs no setup at all. Note ddg is
    -- a third party — see README; consent for it is the tool permission gate.
    providers = { "searxng" },

    -- Seconds, per provider. The chain moves on rather than hanging.
    timeout = 15,
}

-- Keys whose value is a credential. Declared here rather than inferred from the
-- name so that adding a provider is a deliberate act, not a `_key` suffix.
local SECRETS = { exa_api_key = true }

-- setup() values, kept separate from `defaults` so the two layers stay
-- distinguishable — folding them together would make the order above
-- unobservable.
local configured = {}

--- Merge user config into the setup layer. Called by setup() in init.lua.
function M.init(cfg)
    if type(cfg) ~= "table" then return end
    for k, v in pairs(cfg) do
        configured[k] = v
    end
end

--- Read `$CRUCIBLE_WEB_SEARCH_<KEY>`, for declared secrets only. Applying it to
--- every key would let the environment silently redirect `searxng_url`, which
--- is a network destination and belongs in config a user can read back.
local function from_env(key)
    if not SECRETS[key] then return nil end
    local suffix = (key:upper():gsub("[^A-Z0-9]", "_"))
    local val = os.getenv(ENV_PREFIX .. suffix)
    if val == nil or val == "" then return nil end
    return val
end

--- Read `[plugins.web-search].<key>`. Returns nil when unset and also when
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
    local env = from_env(key)
    if env ~= nil then return env end
    if configured[key] ~= nil then return configured[key] end
    local val = from_toml(key)
    if val ~= nil then return val end
    if defaults[key] ~= nil then return defaults[key] end
    return fallback
end

--- Reset the setup layer. Tests only — the daemon never unconfigures a plugin,
--- but a suite that could not clear setup() would have order-dependent tests.
function M.reset()
    configured = {}
end

-- ============================================================================
-- Typed accessors
-- ============================================================================

local function trimmed(value)
    if type(value) ~= "string" then return nil end
    local s = (value:gsub("^%s+", ""):gsub("%s+$", ""))
    return s ~= "" and s or nil
end

--- The provider chain, in preference order.
---
--- A bare string is accepted as a one-element chain: `providers = "ddg"` in
--- TOML is the obvious way to write "just this one", and silently ignoring it
--- would disable search with no message anywhere.
---
--- An explicitly empty list is a real setting — it disables search — so this
--- returns `{}` rather than falling back to the default.
function M.providers()
    local raw = M.get("providers")
    if type(raw) == "string" then raw = { raw } end
    if type(raw) ~= "table" then return {} end

    local out = {}
    for _, name in ipairs(raw) do
        local clean = trimmed(name)
        if clean then out[#out + 1] = clean:lower() end
    end
    return out
end

--- Everything a provider adapter needs, resolved once per call so a search
--- cannot see two different configurations halfway through the chain.
function M.snapshot()
    local timeout = tonumber(M.get("timeout")) or defaults.timeout
    if timeout < 1 then timeout = defaults.timeout end

    return {
        providers = M.providers(),
        searxng_url = trimmed(M.get("searxng_url")),
        exa_api_key = trimmed(M.get("exa_api_key")),
        timeout = math.floor(timeout),
    }
end

return M
