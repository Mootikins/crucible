--- Exa provider, against the recorded JSON-RPC envelope. No network: the
--- harness's `test_mocks` http mock answers by URL, which also lets the request
--- itself be asserted — the envelope Exa expects is as much a part of this
--- provider as the parsing is.

local contract = require("contract")
local exa = require("providers.exa")
local fixtures = require("web-search.tests.fixtures")

-- Pinned here rather than read from the module: the endpoint is the contract
-- with the third party, and a silent change to it should fail a test.
local URL = "https://mcp.exa.ai/mcp"

--- Point the http mock at `resp` for the Exa endpoint. Also clears recorded
--- calls, so `last_post()` always describes the request under test.
local function responds(resp)
    test_mocks.setup({ http = { responses = { [URL] = resp } } })
end

local function ok_body(body)
    responds({ status = 200, ok = true, body = body })
end

--- url, opts of the most recent POST.
local function last_post()
    local calls = test_mocks.get_calls("http", "post")
    local call = calls[#calls]
    assert.is_not_nil(call)
    return call[1], call[2]
end

local function sent_envelope()
    local _, opts = last_post()
    return cru.json.decode(opts.body)
end

local function keys_of(t)
    local out = {}
    for k in pairs(t) do out[#out + 1] = k end
    table.sort(out)
    return out
end

--- The recorded response, verbatim.
local function recorded()
    return fixtures.raw("exa_jsonrpc.json")
end

--- The same envelope with a different inner payload, for cases the recording
--- cannot express (empty results, tool-level error).
local function envelope_of(inner, is_error)
    return cru.json.encode({
        jsonrpc = "2.0",
        id = 1,
        result = {
            content = { { type = "text", text = type(inner) == "string" and inner or cru.json.encode(inner) } },
            isError = is_error or false,
        },
    })
end

describe("exa provider", function()
    describe("the recorded response", function()
        it("normalises to a valid contract payload", function()
            ok_body(recorded())
            local payload, err = exa("rust prompt caching")
            assert.is_nil(err)
            assert.is_not_nil(payload)
            local valid, why = contract.validate(payload)
            if not valid then error("payload off contract: " .. tostring(why), 2) end
            assert.equal("exa", payload.provider)
            assert.equal(3, #payload.results)
            assert.equal("Prompt caching with Claude", payload.results[2].title)
        end)

        it("reads the snippet from Exa's `text` key", function()
            ok_body(recorded())
            local payload = exa("rust prompt caching")
            assert.truthy(payload.results[1].snippet:find("Prompt caching lets you reuse", 1, true))
        end)

        it("drops Exa's score, which shares no scale with SearXNG's", function()
            ok_body(recorded())
            local payload = exa("rust prompt caching")
            assert.deep_equal({ "snippet", "title", "url" }, keys_of(payload.results[1]))
        end)

        it("reports degraded as empty rather than absent", function()
            ok_body(recorded())
            local payload = exa("rust prompt caching")
            assert.deep_equal({}, payload.degraded)
        end)

        it("truncates a long snippet to the contract's cap", function()
            ok_body(recorded())
            local payload = exa("rust prompt caching")
            -- The second hit's `text` runs well past the cap in the recording.
            assert.truthy(#payload.results[2].snippet <= contract.MAX_SNIPPET)
            assert.equal("...", payload.results[2].snippet:sub(-3))
        end)
    end)

    describe("the request", function()
        it("posts a tools/call envelope naming Exa's search tool", function()
            ok_body(recorded())
            exa("rust prompt caching")
            local url = last_post()
            assert.equal(URL, url)
            local sent = sent_envelope()
            assert.equal("2.0", sent.jsonrpc)
            assert.equal("tools/call", sent.method)
            assert.equal("web_search_exa", sent.params.name)
            assert.equal("rust prompt caching", sent.params.arguments.query)
        end)

        it("asks for the number of results the caller wanted", function()
            ok_body(recorded())
            exa("q", { max_results = 3 })
            assert.equal(3, sent_envelope().params.arguments.numResults)
        end)

        it("clamps numResults to the contract ceiling", function()
            ok_body(recorded())
            exa("q", { max_results = 999 })
            assert.equal(contract.MAX_RESULTS, sent_envelope().params.arguments.numResults)
        end)

        it("identifies itself honestly and takes an override", function()
            ok_body(recorded())
            exa("q")
            local _, opts = last_post()
            assert.truthy(opts.headers["User-Agent"]:find("^crucible/"))

            exa("q", { user_agent = "crucible/9.9.9" })
            local _, with_override = last_post()
            assert.equal("crucible/9.9.9", with_override.headers["User-Agent"])
        end)

        it("sends no Authorization header when no key is configured", function()
            ok_body(recorded())
            exa("q")
            local _, opts = last_post()
            assert.is_nil(opts.headers["Authorization"])
        end)

        it("sends the key as a header, never in the URL or body", function()
            ok_body(recorded())
            exa("q", { api_key = "exa-test-key-abcdef" })
            local url, opts = last_post()
            assert.equal("Bearer exa-test-key-abcdef", opts.headers["Authorization"])
            -- A key in the query string lands in every proxy and access log
            -- between here and Exa; a key in the body lands in request traces.
            assert.falsy(url:find("exa-test-key", 1, true))
            assert.falsy(opts.body:find("exa-test-key", 1, true))
        end)

        it("honours a timeout and defaults to one", function()
            ok_body(recorded())
            exa("q", { timeout = 5 })
            local _, opts = last_post()
            assert.equal(5, opts.timeout)

            exa("q")
            local _, defaulted = last_post()
            assert.is_number(defaulted.timeout)
            assert.truthy(defaulted.timeout > 0)
        end)
    end)

    describe("failures name the provider and why", function()
        it("surfaces an HTTP status", function()
            responds({ status = 429, ok = false, body = "rate limited" })
            local payload, err = exa("q")
            assert.is_nil(payload)
            assert.equal("exa", err.provider)
            assert.truthy(err.error:find("429", 1, true))
            assert.truthy(err.error:find("exa:", 1, true))
        end)

        it("surfaces a request that never reached the endpoint", function()
            responds({ status = 0, ok = false, body = "" })
            local payload, err = exa("q")
            assert.is_nil(payload)
            assert.equal("exa", err.provider)
            assert.truthy(err.error:find("failed", 1, true))
        end)

        it("surfaces a body that is not JSON-RPC", function()
            ok_body("<html><body>we are down</body></html>")
            local payload, err = exa("q")
            assert.is_nil(payload)
            assert.truthy(err.error:find("not JSON%-RPC"))
        end)

        it("surfaces a JSON-RPC error object", function()
            ok_body(cru.json.encode({
                jsonrpc = "2.0",
                id = 1,
                error = { code = -32601, message = "Method not found" },
            }))
            local payload, err = exa("q")
            assert.is_nil(payload)
            assert.truthy(err.error:find("-32601", 1, true))
            assert.truthy(err.error:find("Method not found", 1, true))
        end)

        it("surfaces a tool-level error rather than reading it as results", function()
            ok_body(envelope_of("query is required", true))
            local payload, err = exa("q")
            assert.is_nil(payload)
            assert.truthy(err.error:find("query is required", 1, true))
        end)

        it("surfaces a result block that is not an Exa search payload", function()
            ok_body(envelope_of({ items = {} }))
            local payload, err = exa("q")
            assert.is_nil(payload)
            assert.truthy(err.error:find("Exa search payload", 1, true))
        end)

        it("rejects an empty query without calling out", function()
            ok_body(recorded())
            local payload, err = exa("   ")
            assert.is_nil(payload)
            assert.equal("exa", err.provider)
            assert.truthy(err.error:find("query", 1, true))
            assert.equal(0, #test_mocks.get_calls("http", "post"))
        end)

        it("never returns a bare nil", function()
            -- Whatever the endpoint does, the chain gets something it can name
            -- the failed provider from and move on with.
            for _, resp in ipairs({
                { status = 500, ok = false, body = "" },
                { status = 200, ok = true, body = "" },
                { status = 200, ok = true, body = "null" },
                { status = 200, ok = true, body = '{"jsonrpc":"2.0","id":1,"result":{}}' },
            }) do
                responds(resp)
                local payload, err = exa("q")
                assert.is_nil(payload)
                assert.is_table(err)
                assert.equal("exa", err.provider)
                assert.is_string(err.error)
            end
        end)

        it("scrubs a key the endpoint echoed back", function()
            local key = "exa-live-0123456789abcdef"
            responds({
                status = 401,
                ok = false,
                body = '{"error":{"message":"invalid key: ' .. key .. '"}}',
            })
            local payload, err = exa("q", { api_key = key })
            assert.is_nil(payload)
            -- An error string reaches the model's context and the daemon log.
            assert.falsy(err.error:find(key, 1, true))
            assert.truthy(err.error:find("redacted", 1, true))
        end)
    end)

    describe("transport variations", function()
        it("accepts an SSE-framed reply, which streamable-HTTP endpoints send", function()
            local compact = cru.json.encode(fixtures.json("exa_jsonrpc.json"))
            ok_body("event: message\r\ndata: " .. compact .. "\r\n\r\n")
            local payload, err = exa("rust prompt caching")
            assert.is_nil(err)
            assert.equal(3, #payload.results)
            assert.equal("exa", payload.provider)
        end)

        it("treats no matches as an empty result set, not a failure", function()
            ok_body(envelope_of({ results = {} }))
            local payload, err = exa("q")
            assert.is_nil(err)
            assert.is_not_nil(payload)
            assert.equal(0, #payload.results)
            assert.truthy(contract.validate(payload))
        end)

        it("drops hits with no url, keeping the rest", function()
            ok_body(envelope_of({
                results = {
                    { title = "kept", url = "https://exa.test/a", text = "a" },
                    { title = "no url", text = "b" },
                },
            }))
            local payload = exa("q")
            assert.equal(1, #payload.results)
            assert.equal("kept", payload.results[1].title)
        end)
    end)
end)
