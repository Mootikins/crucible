--!strict
--- web-search configuration.
---
--- The order is the one of `cru.settings.new`, highest priority first:
---
---   0. `$CRUCIBLE_WEB_SEARCH_<KEY>` — declared secrets only.
---   1. values passed to `setup({...})` — last call wins per key.
---   2. the `plugins.web-search` section.
---   3. the `defaults` table below.
---   4. the caller's `fallback` argument, for keys this module does not declare.
---
--- A secret comes from the environment before setup() and the config. That
--- lets a user keep a key out of a config file that gets committed, and a key
--- written into the config anyway still works as the lower-priority fallback.

local PLUGIN = "web-search"

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

local settings = cru.settings.new(PLUGIN, defaults, { secrets = SECRETS })

M.init = settings.init
M.get = settings.get
M.reset = settings.reset

-- ============================================================================
-- Typed accessors
-- ============================================================================

local function trimmed(value: any): string?
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
function M.providers(): { string }
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
function M.snapshot(): { [string]: any }
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
