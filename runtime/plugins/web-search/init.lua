--- web-search — one `web_search` tool over a configurable provider chain.
---
---   [plugins.web-search]
---   providers   = ["searxng", "ddg"]      # order = preference; [] disables search
---   searxng_url = "http://localhost:8888" # unset → searxng is skipped
---   timeout     = 15
---
--- or, from the user's init.lua (which runs after plugins load, so it wins):
---
---   require("web-search").setup({ providers = { "searxng", "ddg" } })
---
--- The chain is walked in the configured order, each provider is skipped when
--- the config it needs is absent, and the first success is returned. Config is
--- the whole policy: there are no hidden fallbacks, and a provider that is not
--- named in `providers` is never called — including via the tool's `provider`
--- argument, so the model cannot reach a destination the user did not enable.
---
--- **The shipped default sends queries to DuckDuckGo**, a third party. Consent
--- for that is the existing tool permission gate, not a bespoke prompt; see
--- README.md.
---
--- Every provider ends in `contract.normalise`, so the model sees one result
--- shape regardless of which one answered. What the *user* sees is the extra
--- `tool:display_complete` summary line — a separate channel, already wired.

-- `ws_config`, not `config`: three shipped plugins have a lua/config.lua, and
-- package.loaded is keyed on the bare module name. Whichever plugin loaded last
-- owns "config", so requiring it by that name binds a foreign module.
local config = require("ws_config")
local contract = require("contract")

local TOOL = "web_search"
local PLUGIN = "web-search"
local VERSION = "0.1.0"

local M = {}

-- ============================================================================
-- The chain
-- ============================================================================

--- One entry per provider Crucible ships an adapter for.
---
---   module    where `require` finds it
---   needs     returns a reason string when the provider's config is absent,
---             nil when it is ready. A provider that needs nothing returns nil
---             unconditionally — that is what "keyless" means here.
---   opts      the adapter's option table, built from resolved config
---
--- `needs` is separate from the adapter's own "unconfigured" error on purpose:
--- the chain has to distinguish "skip this, the user never set it up" from
--- "this was set up and it broke" *before* spending a network call, and both
--- have to appear in the total-failure message for different reasons.
local PROVIDERS = {
    searxng = {
        module = "providers.searxng",
        needs = function(cfg)
            if not cfg.searxng_url then
                return "no `searxng_url` is configured. Point it at an instance you "
                    .. "host; there is no default and no shipped public instance list, "
                    .. "because all nine tested public instances rate-limit or "
                    .. "challenge automated queries."
            end
        end,
        opts = function(cfg, limit)
            return { url = cfg.searxng_url, timeout = cfg.timeout, max_results = limit }
        end,
    },

    ddg = {
        module = "providers.ddg",
        needs = function() return nil end,
        opts = function(cfg, limit)
            return { timeout = cfg.timeout, max_results = limit }
        end,
    },

    exa = {
        module = "providers.exa",
        -- Exa answers anonymously, so a key is never *required* — it only buys
        -- rate limit. Exa is kept out of the shipped default chain instead:
        -- being opt-in is about the third party seeing the raw query, not about
        -- whether a credential exists.
        needs = function() return nil end,
        opts = function(cfg, limit)
            return { api_key = cfg.exa_api_key, timeout = cfg.timeout, max_results = limit }
        end,
    },
}

--- The adapters ship in two shapes: a bare function (`searxng`, `exa`) and a
--- table with `.search` (`ddg`, which needs its parse seam reachable from tests
--- and a Lua function cannot carry fields). Rather than record the shape per
--- provider — a field that would be wrong exactly once, silently — take
--- whichever is there.
local function callable(mod)
    if type(mod) == "function" then return mod end
    if type(mod) == "table" and type(mod.search) == "function" then return mod.search end
    return nil
end

--- Flatten the three error shapes the adapters return into one sentence.
--- `searxng` returns `{reason, message, …}`, `ddg` returns `{message}`, `exa`
--- returns `{error}`; a caller that had to know which is which would be a
--- fourth place to update when a provider is added.
local function reason_of(err)
    if type(err) == "string" and err ~= "" then return err end
    if type(err) == "table" then
        for _, key in ipairs({ "message", "error", "reason" }) do
            if type(err[key]) == "string" and err[key] ~= "" then return err[key] end
        end
    end
    return "failed without saying why"
end

--- Run one provider. Returns `payload` or `nil, reason`.
---
--- pcall because a provider is the only code here that touches the network:
--- `http.get`/`http.post` raise on an argument the host refuses to convert,
--- and an adapter indexing a nil response raises too. Either would
--- take down the whole search from inside the second-choice provider, which is
--- the one failure mode a chain exists to prevent.
--- Seconds the whole chain may take, across every provider it tries.
---
--- The daemon gives each tool call a hard 30s dispatch timeout
--- (`agent_manager/messaging/tool_call.rs`), and on expiry the model receives
--- "timed out after 30 seconds" — not this plugin's failure text. The shipped
--- chain is two providers at 15s each, which lands exactly on 30 and made the
--- carefully-built "here is every provider I tried and why each failed"
--- message unreachable in precisely the slow-failure case it exists for.
---
--- So the chain gets a budget under that ceiling and hands each provider a
--- slice of what is left. Running out is reported as a skip, which means the
--- failure text is always produced with time to spare.
local CHAIN_BUDGET_SECONDS = 25

--- Below this, a provider has no realistic chance of completing a request, so
--- spending the remainder on it buys nothing and costs the failure message.
local MIN_PROVIDER_SECONDS = 3

--- Escape a literal string for use as a Lua pattern.
local function escape_pattern(literal)
    return (literal:gsub("[%^%$%(%)%%%.%[%]%*%+%-%?]", "%%%1"))
end

--- Remove configured secrets from text on its way to model context.
---
--- Providers scrub the messages they construct, but a raise escaping an adapter
--- after it has built an `Authorization` header is stringified generically, and
--- that path had no scrubbing at all.
local function scrub_secrets(text, cfg)
    if type(text) ~= "string" then return tostring(text) end
    for _, key in ipairs({ "exa_api_key", "brave_api_key", "tavily_api_key" }) do
        local secret = cfg and cfg[key]
        -- A short value would match half the string; a real key is long.
        if type(secret) == "string" and #secret >= 8 then
            text = text:gsub(escape_pattern(secret), "<redacted>")
        end
    end
    return text
end

local function attempt(name, entry, query, cfg, limit, seconds)
    local loaded, mod = pcall(require, entry.module)
    if not loaded then
        return nil, "adapter " .. entry.module .. " failed to load: " .. tostring(mod)
    end

    local search = callable(mod)
    if not search then
        return nil, "adapter " .. entry.module .. " exports no callable search"
    end

    -- The slice, not the configured per-provider timeout: the chain shares one
    -- budget (see CHAIN_BUDGET_SECONDS).
    local opts = entry.opts(cfg, limit)
    if seconds then opts.timeout = seconds end
    local ok, payload, err = pcall(search, query, opts)
    if not ok then
        -- Scrubbed: this is the one place an arbitrary raise is stringified
        -- into text that reaches model context. Adapters scrub the messages
        -- they build themselves, but a raise escaping after a provider has
        -- assembled an `Authorization` header lands here carrying it.
        return nil, "raised an error: " .. scrub_secrets(tostring(payload), cfg)
    end
    if type(payload) ~= "table" then
        return nil, reason_of(err)
    end

    -- A payload naming a different provider means the adapter constructed the
    -- result itself instead of going through `contract.normalise`, and the
    -- model would be told the wrong destination for the query.
    if payload.provider ~= name then
        return nil, string.format(
            "returned a payload labelled `%s`, so it did not go through the contract",
            tostring(payload.provider)
        )
    end

    -- `contract.validate` describes itself as the check that catches an adapter
    -- silently widening the shape — and nothing called it outside the test
    -- suite, so the allowlist was enforced only by construction inside
    -- `normalise`. An adapter that builds its own table, or mutates one after
    -- normalising, walked straight through. Checking here covers all three
    -- providers at the single point they funnel into.
    local valid, why = contract.validate(payload)
    if not valid then
        return nil, "returned a payload the contract rejects: " .. tostring(why)
    end

    return payload
end

-- ============================================================================
-- Failure
-- ============================================================================

--- The total-failure message.
---
--- Every provider that was in the chain appears with its own reason, skips
--- included. A tool error reaches the model now, and "search failed" sends it
--- into a retry loop: it cannot tell that retrying is pointless because the one
--- configured provider was never set up, and it cannot tell the user what to
--- fix. The length is worth it — this is the rare error a human acts on.
local function failure_text(query, attempts)
    local lines = {
        string.format('web_search found nothing for "%s": every provider failed.', query),
    }
    for _, a in ipairs(attempts) do
        lines[#lines + 1] = string.format("  %s — %s: %s", a.provider, a.status, a.reason)
    end
    lines[#lines + 1] = "Change the chain with `[plugins.web-search].providers` "
        .. "or `require(\"web-search\").setup{ providers = { … } }`."
    return table.concat(lines, "\n")
end

-- ============================================================================
-- Display
-- ============================================================================

-- Beyond this many degraded engines the list stops being scannable and starts
-- being a wall; the count carries the rest.
local MAX_DEGRADED_SHOWN = 3

--- The user-facing one-liner, e.g.
---   searxng · 8 results · brave, startpage unavailable
---
--- Deliberately *not* what the model receives: the model gets the normalised
--- rows, and this is the separate `tool:display_complete` channel. It names the
--- provider because a network call must never be invisible.
---
--- Returns nil when `payload` is not a web_search result, so the hook can leave
--- an unrecognised result alone rather than mislabelling it.
function M.summarise(payload)
    if type(payload) ~= "table" then return nil end

    if type(payload.tried) == "table" and #payload.tried > 0 then
        return string.format("no results · %s failed", table.concat(payload.tried, ", "))
    end

    if type(payload.provider) ~= "string" or type(payload.results) ~= "table" then
        return nil
    end

    local count = #payload.results
    local parts = {
        payload.provider,
        count == 1 and "1 result" or string.format("%d results", count),
    }

    local degraded = type(payload.degraded) == "table" and payload.degraded or {}
    if #degraded > 0 then
        local shown = {}
        for i = 1, math.min(#degraded, MAX_DEGRADED_SHOWN) do
            shown[i] = degraded[i]
        end
        local text = table.concat(shown, ", ")
        if #degraded > MAX_DEGRADED_SHOWN then
            text = string.format("%s +%d", text, #degraded - MAX_DEGRADED_SHOWN)
        end
        parts[#parts + 1] = text .. " unavailable"
    end

    return table.concat(parts, " · ")
end

--- Register the display hook, once, at load.
---
--- No `pattern` opt, and the tool name is checked inside the handler instead.
--- `execute_tool_display_complete_hooks` calls `runtime_handlers_for(event,
--- None)`, and a handler carrying a pattern is filtered out when the caller
--- passes no identifier (registry.rs:290) — so `{ pattern = "web_search" }`
--- would compile, register, and never once fire.
---
--- rawget because the spec-extraction sandbox stubs `crucible` with a
--- metatable whose __index answers every lookup with a function, and the plugin
--- test runner has no `crucible` global at all.
local function register_display_hook()
    local crucible = rawget(_G, "crucible")
    local on = type(crucible) == "table" and rawget(crucible, "on") or nil
    if type(on) ~= "function" then return false end

    on("tool:display_complete", function(_ctx, event)
        if type(event) ~= "table" or event.name ~= TOOL then return nil end

        -- `event.result` is the serialised tool result, or the error text when
        -- the call failed; on failure it is not JSON and decoding gives up,
        -- which is the right outcome — the raw error is already the best thing
        -- to show.
        local ok, payload = pcall(cru.json.decode, event.result or "")
        if not ok then return nil end

        local summary = M.summarise(payload)
        if not summary then return nil end
        return { summary = summary }
    end)

    return true
end

-- ============================================================================
-- Tool
-- ============================================================================

local function trimmed(value)
    if type(value) ~= "string" then return nil end
    local s = (value:gsub("^%s+", ""):gsub("%s+$", ""))
    return s ~= "" and s or nil
end

local function sorted_names(t)
    local out = {}
    for name in pairs(t) do
        out[#out + 1] = name
    end
    table.sort(out)
    return out
end

--- Which providers this call may use, in order.
---
--- `args.provider` narrows the configured chain; it never widens it. The model
--- picks the arguments, so letting it name a provider absent from `providers`
--- would make the config a suggestion rather than the policy — and the whole
--- transparency claim rests on the config being readable as the complete list
--- of who sees the query.
local function resolve_chain(cfg, requested)
    if #cfg.providers == 0 then
        return nil,
            "web search is disabled: `[plugins.web-search].providers` is empty. "
                .. "Add a provider (known: "
                .. table.concat(sorted_names(PROVIDERS), ", ")
                .. ")."
    end

    if not requested then return cfg.providers end

    for _, name in ipairs(cfg.providers) do
        if name == requested then return { name } end
    end

    return nil,
        string.format(
            "provider '%s' is not in the configured chain (%s). Add it to "
                .. "`[plugins.web-search].providers` first.",
            requested,
            table.concat(cfg.providers, ", ")
        )
end

--- Search the web.
---
--- Returns the contract payload on success. On total failure it returns
--- `{ error = …, tried = { … } }` rather than raising: a raised Lua error
--- reaches the model with a stack traceback attached (`format_lua_error` keeps
--- frames that mention a .lua file), and this message is long and load-bearing
--- enough already. `tried` is the structured half the display hook reads.
function M.web_search(args)
    args = args or {}

    local query = trimmed(args.query)
    if not query then
        return { error = "web_search: `query` is required and must be a non-empty string" }
    end

    local limit = tonumber(args.max_results) or contract.DEFAULT_RESULTS
    limit = math.max(1, math.min(math.floor(limit), contract.MAX_RESULTS))

    local cfg = config.snapshot()
    local chain, chain_err = resolve_chain(cfg, trimmed(args.provider))
    if not chain then
        return { error = "web_search: " .. chain_err }
    end

    local attempts = {}
    local started = os.time()
    for _, name in ipairs(chain) do
        local entry = PROVIDERS[name]
        if not entry then
            -- A typo in `providers` is a configuration bug that would otherwise
            -- present as "search is worse than it used to be".
            attempts[#attempts + 1] = {
                provider = name,
                status = "unknown",
                reason = "not a provider this plugin ships (known: "
                    .. table.concat(sorted_names(PROVIDERS), ", ")
                    .. ")",
            }
        else
            local missing = entry.needs(cfg)
            if missing then
                attempts[#attempts + 1] =
                    { provider = name, status = "skipped", reason = missing }
            else
                -- First success wins, empty results included: a provider that
                -- ran and found nothing has answered the question, and falling
                -- through would make "the web has nothing on this" impossible
                -- to express.
                local remaining = CHAIN_BUDGET_SECONDS - (os.time() - started)
                if remaining < MIN_PROVIDER_SECONDS then
                    attempts[#attempts + 1] = {
                        provider = name,
                        status = "skipped",
                        reason = "chain time budget spent by earlier providers; "
                            .. "raise providers' timeout or shorten the chain",
                    }
                else
                    local slice = math.min(cfg.timeout, remaining)
                    local payload, why = attempt(name, entry, query, cfg, limit, slice)
                    if payload then return payload end
                    attempts[#attempts + 1] =
                        { provider = name, status = "failed", reason = why }
                end
            end
        end
    end

    local tried = {}
    for i, a in ipairs(attempts) do
        tried[i] = a.provider
    end

    return { error = failure_text(query, attempts), tried = tried }
end

-- Registered at load, not in setup(): setup() is called once per plugin with
-- the TOML section and again from the user's init.lua, and a hook registered
-- there would be installed twice.
--
-- `package.loaded` below is what stops a second registration: without it, the
-- documented `require("web-search").setup{}` from a user's init.lua re-ran the
-- whole module and installed a second, unowned handler.
--
-- A _G sentinel was tried here and was WORSE than the problem. `clear_plugin_handlers`
-- drops this plugin's handlers on reload and then re-executes the module — but a
-- global outlives the clearing, so the guard skipped re-registration and the display
-- summary was gone until the daemon restarted. State that has to be reset in step with
-- the handler registry does not belong in a namespace nothing resets.
M.display_hook_registered = register_display_hook()

-- ============================================================================
-- Plugin spec
-- ============================================================================

-- Publish under the name the README tells users to require, so that
-- `require("web-search")` returns this module instead of re-running the file.
package.loaded[PLUGIN] = M

return {
    name = PLUGIN,
    version = VERSION,
    description = "Web search over a configurable provider chain",
    capabilities = { "network", "config" },

    tools = {
        web_search = {
            desc = "Search the web. Returns titles, URLs and snippets from the first "
                .. "provider in the configured chain that answers; the result names "
                .. "which provider that was.",
            params = {
                { name = "query", type = "string", desc = "Search query" },
                {
                    name = "max_results",
                    type = "number",
                    desc = "Max results (default: 8, cap: 25)",
                    optional = true,
                },
                {
                    name = "provider",
                    type = "string",
                    desc = "Use only this provider. Must already be in the configured chain.",
                    optional = true,
                },
            },
            fn = M.web_search,
        },
    },

    setup = function(cfg)
        config.init(cfg)
    end,

    -- Not part of the spec grammar (unrecognised keys are ignored by
    -- `load_plugin_spec`), but the display hook's text is worth asserting on
    -- and the hook itself is unreachable from a suite with no daemon behind it.
    summarise = M.summarise,
}
