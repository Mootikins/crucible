--- The plugin entry point: the chain, the config surface, and the display
--- summary. Loads the REAL init.lua and drives it through the real provider
--- adapters, with the stdlib HTTP mock standing in for the network — so a
--- provider that stops honouring the contract fails here rather than in
--- production.
---
--- The mock's default answer for an unstubbed URL is `200` with an empty body,
--- which every adapter correctly rejects. That is used deliberately as the
--- "this provider failed" case: it costs no fixture and exercises the same
--- failure path a real outage would.

local plugin = require("web-search")
local config = require("ws_config")
local ddg = require("providers.ddg")
local fixtures = require("web-search.tests.fixtures")

local search = plugin.tools.web_search.fn

local QUERY = "rust prompt caching"
local INSTANCE = "http://localhost:8888"
local SEARXNG_URL = INSTANCE .. "/search?q=rust%20prompt%20caching&format=json"

--- Start every test from a known state: no setup() layer, no canned responses,
--- no recorded calls. Without the reset the suite would depend on file order.
local function configure(cfg)
    test_mocks.setup()
    config.reset()
    config.init(cfg or {})
end

local function respond(url, resp)
    test_mocks.setup({ http = { responses = { [url] = resp } } })
end

--- Configure, then stub one URL. `test_mocks.setup` replaces the whole fixture
--- set, so the order matters: responses must be installed last.
local function configure_with(cfg, url, resp)
    config.reset()
    config.init(cfg or {})
    respond(url, resp)
end

local function searxng_ok()
    return {
        status = 200,
        ok = true,
        body = fixtures.raw("searxng_search.json"),
        headers = { ["content-type"] = "application/json" },
    }
end

local function ddg_ok()
    return { status = 200, ok = true, body = fixtures.raw("ddg_lite.html") }
end

describe("web-search plugin", function()
    describe("the tool declaration", function()
        it("is named web_search, the name the permission gate matches on", function()
            assert.is_not_nil(plugin.tools.web_search)
            assert.is_function(plugin.tools.web_search.fn)
        end)

        it("declares query required and max_results/provider optional", function()
            local params = {}
            for _, p in ipairs(plugin.tools.web_search.params) do
                params[p.name] = p
            end
            assert.falsy(params.query.optional)
            assert.truthy(params.max_results.optional)
            assert.truthy(params.provider.optional)
        end)

        it("declares the network capability it uses", function()
            local caps = {}
            for _, c in ipairs(plugin.capabilities) do
                caps[c] = true
            end
            assert.truthy(caps.network)
        end)
    end)

    describe("arguments", function()
        it("rejects a missing query rather than searching for nothing", function()
            configure()
            local result = search({})
            assert.truthy(result.error)
            assert.truthy(result.error:find("query", 1, true))
        end)

        it("rejects a whitespace-only query", function()
            configure()
            assert.truthy(search({ query = "   " }).error)
        end)

        it("clamps max_results to the contract ceiling", function()
            configure_with({ providers = { "ddg" } }, ddg.ENDPOINT, ddg_ok())
            local result = search({ query = QUERY, max_results = 999 })
            assert.truthy(#result.results <= 25)
        end)
    end)

    describe("the chain", function()
        it("skips searxng when no instance URL is configured", function()
            configure_with({ providers = { "searxng", "ddg" } }, ddg.ENDPOINT, ddg_ok())
            local result = search({ query = QUERY })

            assert.equal("ddg", result.provider)
            assert.equal(0, #test_mocks.get_calls("http", "get"))
        end)

        it("returns the first success and never calls the rest", function()
            configure_with(
                { providers = { "searxng", "ddg" }, searxng_url = INSTANCE },
                SEARXNG_URL,
                searxng_ok()
            )
            local result = search({ query = QUERY })

            assert.equal("searxng", result.provider)
            assert.equal(0, #test_mocks.get_calls("http", "post"))
        end)

        it("falls through to the next provider when one fails", function()
            -- searxng is configured but its instance answers with an empty
            -- body, so the chain must move on rather than give up.
            configure_with(
                { providers = { "searxng", "ddg" }, searxng_url = INSTANCE },
                ddg.ENDPOINT,
                ddg_ok()
            )
            local result = search({ query = QUERY })

            assert.equal("ddg", result.provider)
            assert.equal(1, #test_mocks.get_calls("http", "get"))
        end)

        it("honours the configured order", function()
            configure_with(
                { providers = { "ddg", "searxng" }, searxng_url = INSTANCE },
                ddg.ENDPOINT,
                ddg_ok()
            )
            assert.equal("ddg", search({ query = QUERY }).provider)
        end)

        it("treats an empty provider list as search being disabled", function()
            configure({ providers = {} })
            local result = search({ query = QUERY })

            assert.truthy(result.error)
            assert.truthy(result.error:find("disabled", 1, true))
            assert.equal(0, #test_mocks.get_calls("http", "post"))
        end)
    end)

    describe("the provider override", function()
        it("narrows the chain to the named provider", function()
            configure_with(
                { providers = { "searxng", "ddg" }, searxng_url = INSTANCE },
                ddg.ENDPOINT,
                ddg_ok()
            )
            local result = search({ query = QUERY, provider = "ddg" })

            assert.equal("ddg", result.provider)
            assert.equal(0, #test_mocks.get_calls("http", "get"))
        end)

        it("refuses a provider the user has not enabled", function()
            -- The model chooses the arguments. If `provider` could widen the
            -- chain, the config would stop being the complete list of who sees
            -- the query, which is the entire transparency claim.
            configure({ providers = { "ddg" } })
            local result = search({ query = QUERY, provider = "exa" })

            assert.truthy(result.error)
            assert.truthy(result.error:find("not in the configured chain", 1, true))
            assert.equal(0, #test_mocks.get_calls("http", "post"))
        end)
    end)

    describe("total failure", function()
        it("names every provider tried and why each one failed", function()
            configure({ providers = { "searxng", "ddg" } })
            local result = search({ query = QUERY })

            assert.truthy(result.error)
            -- Both halves matter: a skip and a real failure are different
            -- problems with different fixes, and a model told only "search
            -- failed" retries instead of reporting either.
            assert.truthy(result.error:find("searxng", 1, true))
            assert.truthy(result.error:find("searxng_url", 1, true))
            assert.truthy(result.error:find("ddg", 1, true))
            assert.truthy(result.error:find("skipped", 1, true))
            assert.truthy(result.error:find("failed", 1, true))
        end)

        it("quotes the query back so a retry loop is visible in the transcript", function()
            configure({ providers = { "ddg" } })
            assert.truthy(search({ query = QUERY }).error:find(QUERY, 1, true))
        end)

        it("lists the providers it tried in a structured field", function()
            configure({ providers = { "searxng", "ddg" } })
            local result = search({ query = QUERY })

            assert.deep_equal({ "searxng", "ddg" }, result.tried)
        end)

        it("reports a provider name this plugin does not ship", function()
            configure({ providers = { "gogle" } })
            local result = search({ query = QUERY })

            assert.truthy(result.error:find("gogle", 1, true))
            assert.truthy(result.error:find("not a provider this plugin ships", 1, true))
        end)

        it("does not raise, so the error keeps its shape instead of a traceback", function()
            configure({ providers = { "ddg" } })
            local ok, result = pcall(search, { query = QUERY })

            assert.truthy(ok)
            assert.is_string(result.error)
        end)
    end)

    describe("the result", function()
        it("is the contract payload, unchanged", function()
            configure_with(
                { providers = { "searxng" }, searxng_url = INSTANCE },
                SEARXNG_URL,
                searxng_ok()
            )
            local result = search({ query = QUERY })

            assert.is_string(result.query)
            assert.equal("searxng", result.provider)
            assert.is_table(result.results)
            assert.is_table(result.degraded)
        end)

        it("carries degraded engines through from the provider", function()
            configure_with(
                { providers = { "searxng" }, searxng_url = INSTANCE },
                SEARXNG_URL,
                searxng_ok()
            )
            assert.truthy(#search({ query = QUERY }).degraded > 0)
        end)
    end)

    describe("setup()", function()
        it("merges rather than replacing, so two calls compose", function()
            config.reset()
            plugin.setup({ searxng_url = INSTANCE })
            plugin.setup({ providers = { "searxng" } })
            respond(SEARXNG_URL, searxng_ok())

            assert.equal("searxng", search({ query = QUERY }).provider)
        end)

        it("accepts an empty table, which is what an absent TOML section is", function()
            config.reset()
            plugin.setup({})
            respond(ddg.ENDPOINT, ddg_ok())

            -- The shipped default chain still applies.
            assert.equal("ddg", search({ query = QUERY }).provider)
        end)
    end)

    describe("the shipped default against live markup", function()
        -- The recorded fixture double-quotes its class attributes; the page
        -- DuckDuckGo actually serves single-quotes them. That difference cost
        -- every live search its snippets while the whole suite stayed green,
        -- because a row with a title and a URL is still a valid result. Pinned
        -- here in the shape the endpoint really returns.
        local LIVE_SHAPE = table.concat({
            '<form action="/lite/" method="post"></form>',
            "<table>",
            "<tr><td valign='top'>1.&nbsp;</td><td>",
            "<a rel=\"nofollow\" href=\"https://example.test/a\" class='result-link'>A title</a>",
            "</td></tr>",
            "<tr><td>&nbsp;</td><td class='result-snippet'>",
            "Context-aware <b>caching</b> proxy.",
            "</td></tr>",
            "</table>",
        }, "\n")

        it("reads snippets from single-quoted class attributes", function()
            configure_with(
                { providers = { "ddg" } },
                ddg.ENDPOINT,
                { status = 200, ok = true, body = LIVE_SHAPE }
            )
            local result = search({ query = QUERY })

            assert.equal("ddg", result.provider)
            assert.equal("Context-aware caching proxy.", result.results[1].snippet)
        end)
    end)

    describe("the chain's time budget", function()
        -- The daemon gives every tool call a hard 30s dispatch timeout, and on
        -- expiry the model receives "timed out after 30 seconds" rather than
        -- this plugin's failure text. Two providers at the default 15s land
        -- exactly on 30, so the message naming what was tried was unreachable
        -- in the slow-failure case it exists for.
        local function with_clock(step, fn)
            local real = os.time
            local now = real()
            os.time = function()
                now = now + step
                return now
            end
            local ok, err = pcall(fn)
            os.time = real
            if not ok then error(err, 0) end
        end

        it("skips a provider once the budget is spent, rather than running it", function()
            configure({ providers = { "searxng", "ddg" }, searxng_url = INSTANCE })
            local result
            -- 24s per tick: the first provider consumes almost the whole
            -- budget, leaving the second below the floor.
            with_clock(24, function()
                result = search({ query = QUERY })
            end)

            assert.truthy(result.error, "the chain must report, not hang")
            assert.truthy(
                result.error:find("budget", 1, true),
                "the failure must say the budget ran out: " .. tostring(result.error)
            )
        end)

        it("still names every provider it tried when the budget runs out", function()
            configure({ providers = { "searxng", "ddg" }, searxng_url = INSTANCE })
            local result
            with_clock(24, function()
                result = search({ query = QUERY })
            end)

            -- The whole point: this text is what the model reads instead of a
            -- bare "timed out", so it has to survive the case that produced it.
            assert.truthy(result.error:find("searxng", 1, true))
            assert.truthy(result.error:find("ddg", 1, true))
        end)

        it("leaves a fast chain untouched", function()
            configure_with({ providers = { "searxng" }, searxng_url = INSTANCE },
                SEARXNG_URL, searxng_ok())
            -- One tick per call, so nothing approaches the budget.
            local result
            with_clock(1, function()
                result = search({ query = QUERY })
            end)
            assert.is_nil(result.error)
            assert.equal("searxng", result.provider)
        end)
    end)

    describe("re-execution", function()
        -- `execute_plugin` evaluates init.lua without populating
        -- `package.loaded`, so the documented `require("web-search").setup{}`
        -- from a user's init.lua used to re-run the whole module — registering
        -- a SECOND display hook, outside any plugin's ownership, which
        -- `clear_plugin_handlers` could never reap.
        it("returns the same module rather than re-running the file", function()
            local again = require("web-search")
            assert.equal(plugin, again, "require must hit package.loaded, not re-execute")
        end)

        -- There is no global sentinel guarding the hook: one was tried and it
        -- outlived `clear_plugin_handlers`, so a plugin reload dropped the
        -- handler and then skipped re-registering it, losing the summary until
        -- the daemon restarted. `package.loaded` above is the whole fix.
    end)

    describe("the display summary", function()
        it("names the provider, the count and the degraded engines", function()
            local summary = plugin.summarise({
                provider = "searxng",
                results = { {}, {}, {} },
                degraded = { "brave", "startpage" },
            })
            assert.equal("searxng · 3 results · brave, startpage unavailable", summary)
        end)

        it("omits the degraded clause when nothing degraded", function()
            local summary = plugin.summarise({
                provider = "ddg",
                results = { {} },
                degraded = {},
            })
            assert.equal("ddg · 1 result", summary)
        end)

        it("caps a long degraded list with a count", function()
            local summary = plugin.summarise({
                provider = "searxng",
                results = {},
                degraded = { "a", "b", "c", "d", "e" },
            })
            assert.equal("searxng · 0 results · a, b, c +2 unavailable", summary)
        end)

        it("summarises a total failure from the providers it tried", function()
            assert.equal(
                "no results · searxng, ddg failed",
                plugin.summarise({ error = "…", tried = { "searxng", "ddg" } })
            )
        end)

        it("leaves a result it does not recognise alone", function()
            -- Returning nil is what makes the hook pass through; a summary
            -- invented here would mislabel another tool's output.
            assert.is_nil(plugin.summarise({ some = "other tool" }))
            assert.is_nil(plugin.summarise("not a table"))
        end)
    end)
end)
