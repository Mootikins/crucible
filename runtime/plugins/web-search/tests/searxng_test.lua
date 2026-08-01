--- The SearXNG provider, driven entirely off the recorded fixture and the
--- stdlib HTTP mock. No test here touches the network.
---
--- The mock (`test_mocks.setup{ http = { responses = { [url] = … } } }`) is
--- keyed by the *exact* URL, so every test that stubs a response also pins the
--- request the provider is expected to build. Note the mock derives `ok` from
--- the fixture's own `ok` field rather than from the status — the provider
--- reads the status for exactly that reason, so its production and test paths
--- are the same path.

local searxng = require("providers.searxng")
local contract = require("contract")
local fixtures = require("web-search.tests.fixtures")

local INSTANCE = "http://localhost:8888"
local QUERY = "rust prompt caching"
local URL = INSTANCE .. "/search?q=rust%20prompt%20caching&format=json"

--- Install a single canned response and clear the recorded calls.
local function respond(resp, url)
    test_mocks.setup({ http = { responses = { [url or URL] = resp } } })
end

local function json_ok(body)
    return { status = 200, ok = true, body = body, headers = { ["content-type"] = "application/json" } }
end

local function html(body, status)
    return {
        status = status or 403,
        ok = false,
        body = body,
        headers = { ["content-type"] = "text/html; charset=utf-8" },
    }
end

local function last_get()
    local calls = test_mocks.get_calls("http", "get")
    return calls[#calls]
end

local function encode_json(value)
    return cru.json.encode(value)
end

describe("searxng provider", function()
    describe("the request", function()
        it("builds {url}/search?q=…&format=json with the query percent-encoded", function()
            respond(json_ok(fixtures.raw("searxng_search.json")))
            searxng(QUERY, { url = INSTANCE })
            assert.equal(URL, last_get()[1])
        end)

        it("tolerates a trailing slash on the configured URL", function()
            respond(json_ok(fixtures.raw("searxng_search.json")))
            searxng(QUERY, { url = INSTANCE .. "/" })
            assert.equal(URL, last_get()[1])
        end)

        it("percent-encodes non-ASCII a byte at a time", function()
            local url = INSTANCE .. "/search?q=caf%C3%A9&format=json"
            respond(json_ok('{"results":[]}'), url)
            searxng("café", { url = INSTANCE })
            assert.equal(url, last_get()[1])
        end)

        it("honours a timeout, defaulting to 15s", function()
            respond(json_ok('{"results":[]}'))
            searxng(QUERY, { url = INSTANCE })
            assert.equal(15, last_get()[2].timeout)

            respond(json_ok('{"results":[]}'))
            searxng(QUERY, { url = INSTANCE, timeout = 3 })
            assert.equal(3, last_get()[2].timeout)
        end)

        it("names itself in the User-Agent rather than spoofing a browser", function()
            respond(json_ok('{"results":[]}'))
            searxng(QUERY, { url = INSTANCE })
            local ua = last_get()[2].headers["User-Agent"]
            assert.truthy(ua:find("crucible", 1, true))
            assert.falsy(ua:lower():find("mozilla", 1, true))
        end)
    end)

    describe("a successful search", function()
        local payload, err
        respond(json_ok(fixtures.raw("searxng_search.json")))
        payload, err = searxng(QUERY, { url = INSTANCE })

        it("returns a payload that satisfies the contract", function()
            assert.is_nil(err)
            assert.is_not_nil(payload)
            local ok, why = contract.validate(payload)
            if not ok then error("payload invalid: " .. tostring(why), 2) end
        end)

        it("names the provider and echoes the query", function()
            assert.equal("searxng", payload.provider)
            assert.equal(QUERY, payload.query)
        end)

        it("maps unresponsive_engines onto degraded", function()
            assert.deep_equal({ "brave", "startpage" }, payload.degraded)
        end)

        it("still returns results when engines were unresponsive", function()
            assert.truthy(#payload.results >= 3)
            assert.equal("anthropic_sdk::caching - Rust", payload.results[1].title)
        end)

        it("keeps score and engines, drops SearXNG's other 18 fields", function()
            local keys = {}
            for k in pairs(payload.results[1]) do keys[#keys + 1] = k end
            table.sort(keys)
            assert.deep_equal({ "engines", "score", "snippet", "title", "url" }, keys)
        end)

        it("forwards max_results to the contract", function()
            respond(json_ok(fixtures.raw("searxng_search.json")))
            local capped = searxng(QUERY, { url = INSTANCE, max_results = 2 })
            assert.equal(2, #capped.results)
        end)
    end)

    describe("an empty result set", function()
        it("is an answer, not an error, and still reports degraded", function()
            respond(json_ok(encode_json({
                results = {},
                unresponsive_engines = { { "brave", "Too many requests" } },
            })))
            local payload, err = searxng(QUERY, { url = INSTANCE })
            assert.is_nil(err)
            assert.equal(0, #payload.results)
            assert.deep_equal({ "brave" }, payload.degraded)
            assert.truthy(contract.validate(payload))
        end)
    end)

    describe("format=json disabled", function()
        -- The single most common SearXNG setup failure: `search.formats`
        -- ships as `[html]`, so the JSON endpoint 403s with an HTML error
        -- page. "parse failed" would send the user hunting in the wrong place.
        local body = "<!DOCTYPE html><html><body><h1>403 Forbidden</h1></body></html>"

        it("says the JSON API is disabled and names the settings.yml key", function()
            respond(html(body))
            local payload, err = searxng(QUERY, { url = INSTANCE })
            assert.is_nil(payload)
            assert.equal("json_disabled", err.reason)
            assert.truthy(err.message:find("settings.yml", 1, true))
            assert.truthy(err.message:find("search.formats", 1, true) or err.message:find("formats", 1, true))
            assert.truthy(err.message:find("json", 1, true))
        end)

        it("sniffs the body when there is no content-type header", function()
            respond({ status = 200, ok = true, body = "<html><body>results</body></html>", headers = {} })
            local _, err = searxng(QUERY, { url = INSTANCE })
            assert.equal("json_disabled", err.reason)
        end)

        it("trusts a text/html content-type on a 200 with a body that does not look like markup", function()
            respond({
                status = 200,
                ok = true,
                body = "Forbidden",
                headers = { ["Content-Type"] = "text/html; charset=utf-8" },
            })
            local _, err = searxng(QUERY, { url = INSTANCE })
            assert.equal("json_disabled", err.reason)
        end)
    end)

    describe("an anti-bot challenge", function()
        it("is reported as blocked, not as a settings problem the user cannot fix", function()
            respond(html("<!DOCTYPE html><html><body>Verifying your browser…</body></html>", 200))
            local _, err = searxng(QUERY, { url = INSTANCE })
            assert.equal("blocked", err.reason)
            assert.falsy(err.message:find("settings.yml", 1, true))
        end)
    end)

    describe("failures are always structured", function()
        --- Every failure mode, so none of them can regress to a bare nil.
        local cases = {
            {
                reason = "unconfigured",
                run = function() return searxng(QUERY, {}) end,
            },
            {
                reason = "invalid_query",
                run = function() return searxng("   ", { url = INSTANCE }) end,
            },
            {
                reason = "transport",
                run = function()
                    respond({ status = 0, ok = false, body = "", error = "connection refused" })
                    return searxng(QUERY, { url = INSTANCE })
                end,
            },
            {
                reason = "http_error",
                run = function()
                    respond({ status = 429, ok = false, body = '{"error":"rate limited"}' })
                    return searxng(QUERY, { url = INSTANCE })
                end,
            },
            {
                reason = "bad_response",
                run = function()
                    respond({ status = 200, ok = true, body = "not json at all" })
                    return searxng(QUERY, { url = INSTANCE })
                end,
            },
            {
                reason = "bad_response",
                run = function()
                    -- Valid JSON, but not a search response.
                    respond(json_ok('{"detail":"Not Found"}'))
                    return searxng(QUERY, { url = INSTANCE })
                end,
            },
        }

        it("returns nil plus a table naming the provider and the reason", function()
            for _, case in ipairs(cases) do
                local payload, err = case.run()
                assert.is_nil(payload)
                assert.is_table(err)
                assert.equal("searxng", err.provider)
                assert.equal(case.reason, err.reason)
                assert.is_string(err.message)
                assert.truthy(#err.message > 0)
            end
        end)

        it("carries the HTTP status when there was a response", function()
            respond({ status = 429, ok = false, body = "slow down" })
            local _, err = searxng(QUERY, { url = INSTANCE })
            assert.equal(429, err.status)
            assert.truthy(err.message:find("429", 1, true))
        end)

        it("says which instance failed, so a chain error names a host", function()
            respond({ status = 0, ok = false, body = "", error = "dns error" })
            local _, err = searxng(QUERY, { url = INSTANCE })
            assert.equal(INSTANCE, err.instance)
        end)
    end)

    describe("secrets", function()
        it("never echoes a configured auth header back", function()
            respond({ status = 0, ok = false, body = "", error = "connection refused" })
            local _, err = searxng(QUERY, {
                url = INSTANCE,
                headers = { Authorization = "Bearer super-secret-token" },
            })
            assert.falsy(encode_json(err):find("super%-secret%-token"))
        end)

        it("sends the configured header even so", function()
            respond(json_ok('{"results":[]}'))
            searxng(QUERY, { url = INSTANCE, headers = { Authorization = "Bearer tok" } })
            assert.equal("Bearer tok", last_get()[2].headers.Authorization)
        end)

        it("redacts basic-auth credentials embedded in the instance URL", function()
            local creds = "http://admin:hunter2@localhost:8888"
            respond({
                status = 0,
                ok = false,
                body = "",
                -- Transport errors quote the URL back verbatim, credentials included.
                error = "error sending request for url (" .. creds .. "/search?q=x&format=json)",
            }, creds .. "/search?q=rust%20prompt%20caching&format=json")

            local _, err = searxng(QUERY, { url = creds })
            local encoded = encode_json(err)
            assert.falsy(encoded:find("hunter2", 1, true))
            assert.falsy(encoded:find("admin", 1, true))
            assert.equal("http://localhost:8888", err.instance)
        end)
    end)
end)
