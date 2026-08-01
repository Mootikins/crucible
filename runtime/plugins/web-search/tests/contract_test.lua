--- The one test that stops the tool's output shape drifting by provider.
---
--- Each provider's *recorded* payload is turned into rows the way its adapter
--- will, then handed to `contract.normalise`, and the results are compared to
--- each other rather than only to a literal. If a future provider widens the
--- shape, the cross-provider assertions fail before anything ships.

local contract = require("contract")
local fixtures = require("web-search.tests.fixtures")

-- ============================================================================
-- Fixture → rows. Real transport and parsing live in the provider adapters;
-- these are the smallest extraction that gets each recorded payload to the
-- contract boundary, and exist so the fixtures are exercised rather than
-- merely stored.
-- ============================================================================

local function searxng_rows()
    local raw = fixtures.json("searxng_search.json")
    -- SearXNG hits are already keyed the way the contract reads them
    -- (`content` is one of the snippet aliases), so the adapter forwards them
    -- whole and lets the contract's allowlist discard the other 18 fields.
    return raw.results, raw.unresponsive_engines
end

local function exa_rows()
    local envelope = fixtures.json("exa_jsonrpc.json")
    local payload = cru.json.decode(envelope.result.content[1].text)
    local rows = {}
    for _, r in ipairs(payload.results) do
        -- Exa's `score` is a 0–1 neural relevance, not SearXNG's cross-engine
        -- agreement sum. Publishing both under one key would invite the model
        -- to compare numbers that do not share a scale, so the adapter drops it.
        rows[#rows + 1] = { title = r.title, url = r.url, text = r.text }
    end
    return rows
end

local function unescape(s)
    s = s:gsub("<[^>]->", "")
    s = s:gsub("&nbsp;", " "):gsub("&mdash;", "\u{2014}"):gsub("&middot;", "\u{00B7}")
    s = s:gsub("&amp;", "&"):gsub("&lt;", "<"):gsub("&gt;", ">"):gsub("&quot;", '"')
    return s
end

local function ddg_rows()
    local html = fixtures.raw("ddg_lite.html")
    local rows, pending = {}, nil
    for chunk in html:gmatch("<tr.->.-</tr>") do
        if chunk:find('class="result%-sponsored"') then
            pending = nil -- an ad and its snippet are never results
        else
            local href, title = chunk:match('href="([^"]+)".-class="result%-link"[^>]*>(.-)</a>')
            if href then
                -- lite wraps some hits in /l/?uddg=<percent-encoded target>
                local target = href:match("uddg=([^&]+)")
                if target then
                    href = target:gsub("%%(%x%x)", function(h)
                        return string.char(tonumber(h, 16))
                    end)
                end
                pending = { title = unescape(title), url = href }
                rows[#rows + 1] = pending
            else
                local snippet = chunk:match('class="result%-snippet"[^>]*>(.-)</td>')
                if snippet and pending then
                    pending.snippet = unescape(snippet)
                    pending = nil
                end
            end
        end
    end
    return rows
end

local function keys_of(t)
    local out = {}
    for k in pairs(t) do out[#out + 1] = k end
    table.sort(out)
    return out
end

--- assert.truthy discards a second argument, so a validation failure would
--- report "expected truthy" and swallow the reason. Surface the reason.
local function assert_valid(payload, label)
    local ok, err = contract.validate(payload)
    if not ok then
        error(string.format("%s: %s", label or "payload", tostring(err)), 2)
    end
end

describe("web-search contract", function()
    describe("searxng", function()
        local rows, unresponsive = searxng_rows()
        local payload = contract.normalise({
            query = "rust prompt caching",
            provider = "searxng",
            results = rows,
            degraded = unresponsive,
        })

        it("normalises to a valid payload", function()
            assert.is_not_nil(payload)
            assert_valid(payload, "searxng")
        end)

        it("keeps only the five contract fields out of SearXNG's 23", function()
            assert.deep_equal(
                { "engines", "score", "snippet", "title", "url" },
                keys_of(payload.results[1])
            )
        end)

        it("carries cross-engine agreement, which single-provider APIs cannot", function()
            assert.equal(2.67, payload.results[1].score)
            assert.deep_equal({ "duckduckgo", "google" }, payload.results[1].engines)
        end)

        it("reports unresponsive engines as degraded names", function()
            assert.deep_equal({ "brave", "startpage" }, payload.degraded)
        end)

        it("still returns results when engines were unresponsive", function()
            assert.truthy(#payload.results >= 3)
        end)

        it("collapses whitespace out of snippets", function()
            local forum = payload.results[4]
            assert.truthy(forum.snippet:find("does. Is there a crate", 1, true))
            assert.falsy(forum.snippet:find("\n", 1, true))
            assert.falsy(forum.snippet:find("\t", 1, true))
        end)
    end)

    describe("exa", function()
        local payload = contract.normalise({
            query = "rust prompt caching",
            provider = "exa",
            results = exa_rows(),
        })

        it("normalises the JSON-RPC envelope's inner payload", function()
            assert_valid(payload, "exa")
            assert.equal("Prompt caching with Claude", payload.results[2].title)
        end)

        it("reads the snippet from Exa's `text` key", function()
            assert.truthy(payload.results[1].snippet:find("Prompt caching lets you reuse", 1, true))
        end)

        it("omits score and engines", function()
            assert.deep_equal({ "snippet", "title", "url" }, keys_of(payload.results[1]))
        end)

        it("still reports degraded, as an empty table", function()
            assert.is_not_nil(payload.degraded)
            assert.equal(0, #payload.degraded)
        end)
    end)

    describe("ddg", function()
        local payload = contract.normalise({
            query = "rust prompt caching",
            provider = "ddg",
            results = ddg_rows(),
        })

        it("normalises the scraped lite page", function()
            assert_valid(payload, "ddg")
            assert.equal(4, #payload.results)
        end)

        it("drops sponsored rows before the contract sees them", function()
            for _, r in ipairs(payload.results) do
                assert.falsy(r.url:find("example%-ads"))
            end
        end)

        it("unwraps the /l/?uddg= redirect", function()
            assert.equal("https://www.anthropic.com/news/prompt-caching", payload.results[2].url)
        end)

        it("still reports degraded, as an empty table", function()
            assert.deep_equal({}, payload.degraded)
        end)
    end)

    describe("shape is identical across providers", function()
        local payloads = {
            searxng = contract.normalise({
                query = "q",
                provider = "searxng",
                results = select(1, searxng_rows()),
                degraded = select(2, searxng_rows()),
            }),
            exa = contract.normalise({ query = "q", provider = "exa", results = exa_rows() }),
            ddg = contract.normalise({ query = "q", provider = "ddg", results = ddg_rows() }),
        }

        it("has the same top-level keys whichever provider answered", function()
            for name, p in pairs(payloads) do
                assert.deep_equal({ "degraded", "provider", "query", "results" }, keys_of(p))
                assert.is_string(name)
            end
        end)

        it("names its provider in the payload", function()
            for name, p in pairs(payloads) do
                assert.equal(name, p.provider)
            end
        end)

        it("gives every result the same required fields", function()
            for _, p in pairs(payloads) do
                for _, r in ipairs(p.results) do
                    assert.is_string(r.title)
                    assert.is_string(r.url)
                    assert.is_string(r.snippet)
                end
            end
        end)

        it("validates under the same rules", function()
            for name, p in pairs(payloads) do
                assert_valid(p, name)
            end
        end)
    end)

    describe("caps", function()
        it("truncates a long snippet to the per-result cap", function()
            local rows = select(1, searxng_rows())
            local payload = contract.normalise({
                query = "q",
                provider = "searxng",
                results = rows,
            })
            local long = payload.results[2]
            assert.truthy(#long.snippet <= contract.MAX_SNIPPET)
            assert.equal("...", long.snippet:sub(-3))
        end)

        it("never truncates through a UTF-8 codepoint", function()
            local payload = contract.normalise({
                query = "q",
                provider = "test",
                results = { { title = "t", url = "https://x.test", snippet = string.rep("é", 400) } },
            })
            local s = payload.results[1].snippet
            assert.truthy(#s <= contract.MAX_SNIPPET)
            -- Every source character is the two-byte "é", so the kept text must
            -- consist of nothing but whole "é"s. A cut between a lead byte and
            -- its continuation would leave a stray byte behind — invalid UTF-8,
            -- which breaks JSON encoding downstream.
            local body = s:sub(1, #s - 3)
            assert.equal("...", s:sub(-3))
            assert.equal("", (body:gsub("\u{00E9}", "")))
        end)

        it("keeps the serialised payload inside the spill budget", function()
            local rows = {}
            for i = 1, contract.MAX_RESULTS do
                rows[i] = {
                    title = string.format("Result number %d about caching", i),
                    url = string.format("https://example.test/articles/%d/prompt-caching", i),
                    snippet = string.rep("padding ", 60),
                }
            end
            local payload = contract.normalise({
                query = "q",
                provider = "test",
                results = rows,
                max_results = contract.MAX_RESULTS,
            })
            assert.truthy(contract.encoded_size(payload) <= contract.MAX_PAYLOAD_BYTES)
            -- The byte cap, not max_results, is the real bound: 25 × 300 chars
            -- cannot fit, so asking for the maximum returns fewer.
            assert.truthy(#payload.results < contract.MAX_RESULTS)
            assert.truthy(#payload.results > 1)
        end)

        it("never trims the result list to empty", function()
            -- Titles and URLs are not capped — truncating a URL makes it
            -- unfollowable, which is worse than a large one. So a single
            -- pathological hit can exceed the budget, and the floor keeps it:
            -- an empty result list would read as "no matches", a different and
            -- wrong answer. Spilling that one row is the better failure.
            local payload = contract.normalise({
                query = "q",
                provider = "test",
                results = {
                    { title = "t", url = "https://x.test/" .. string.rep("a", 9000), snippet = "s" },
                },
            })
            -- Dropped rather than kept: a 9KB URL is unfollowable whether it
            -- is truncated or not, and keeping it spilled the whole payload to
            -- a file. The loss is stated in `degraded` so an empty result list
            -- cannot be misread as "no matches".
            assert.equal(0, #payload.results)
            assert.equal(1, #payload.degraded)
            -- A short label, not a sentence: `degraded` is comma-joined into
            -- the one-line display summary beside engine names like "brave".
            assert.truthy(payload.degraded[1]:find("url-too-long", 1, true))
            assert.truthy(contract.encoded_size(payload) <= contract.MAX_PAYLOAD_BYTES)
        end)

        it("defaults to 8 results and clamps a caller's request", function()
            local rows = {}
            for i = 1, 40 do
                rows[i] = { title = "t" .. i, url = "https://x.test/" .. i, snippet = "s" }
            end
            assert.equal(8, #contract.normalise({ query = "q", provider = "p", results = rows }).results)
            assert.equal(
                contract.MAX_RESULTS,
                #contract.normalise({
                    query = "q",
                    provider = "p",
                    results = rows,
                    max_results = 999,
                }).results
            )
            assert.equal(
                1,
                #contract.normalise({ query = "q", provider = "p", results = rows, max_results = 0 }).results
            )
        end)
    end)

    describe("normalise", function()
        it("always emits degraded, even with no degraded input", function()
            local payload = contract.normalise({ query = "q", provider = "p", results = {} })
            assert.is_table(payload.degraded)
            assert.equal(0, #payload.degraded)
        end)

        it("accepts degraded as bare names as well as name/reason pairs", function()
            local pairs_form = contract.normalise({
                query = "q",
                provider = "p",
                results = {},
                degraded = { { "brave", "Too many requests" } },
            })
            local names_form = contract.normalise({
                query = "q",
                provider = "p",
                results = {},
                degraded = { "brave" },
            })
            assert.deep_equal(pairs_form.degraded, names_form.degraded)
        end)

        it("drops hits with no url or no title", function()
            local payload = contract.normalise({
                query = "q",
                provider = "p",
                results = {
                    { title = "keep", url = "https://x.test", snippet = "s" },
                    { title = "no url", snippet = "s" },
                    { url = "https://y.test", snippet = "s" },
                },
            })
            assert.equal(1, #payload.results)
            assert.equal("keep", payload.results[1].title)
        end)

        it("gives a missing snippet an empty string rather than a hole", function()
            local payload = contract.normalise({
                query = "q",
                provider = "p",
                results = { { title = "t", url = "https://x.test" } },
            })
            assert.equal("", payload.results[1].snippet)
        end)

        it("rejects a call with no query or no provider", function()
            local ok, err = contract.normalise({ provider = "p", results = {} })
            assert.is_nil(ok)
            assert.truthy(err:find("query"))
            ok, err = contract.normalise({ query = "q", results = {} })
            assert.is_nil(ok)
            assert.truthy(err:find("provider"))
        end)
    end)

    describe("validate", function()
        local function base()
            return {
                query = "q",
                provider = "p",
                results = { { title = "t", url = "https://x.test", snippet = "s" } },
                degraded = {},
            }
        end

        it("accepts the minimal valid payload", function()
            assert.truthy(contract.validate(base()))
        end)

        it("rejects an absent degraded, because absence must not be a signal", function()
            local p = base()
            p.degraded = nil
            local ok, err = contract.validate(p)
            assert.falsy(ok)
            assert.truthy(err:find("degraded"))
        end)

        it("rejects a field a provider tried to smuggle through", function()
            local p = base()
            p.results[1].thumbnail = "https://x.test/thumb.png"
            local ok, err = contract.validate(p)
            assert.falsy(ok)
            assert.truthy(err:find("unknown key"))
        end)

        it("rejects an extra top-level key", function()
            local p = base()
            p.number_of_results = 29
            local ok, err = contract.validate(p)
            assert.falsy(ok)
            assert.truthy(err:find("unknown payload key"))
        end)

        it("rejects an over-long snippet", function()
            local p = base()
            p.results[1].snippet = string.rep("x", contract.MAX_SNIPPET + 1)
            local ok, err = contract.validate(p)
            assert.falsy(ok)
            assert.truthy(err:find("snippet"))
        end)

        it("rejects a payload over the spill budget", function()
            local p = base()
            for i = 1, 40 do
                p.results[i] = {
                    title = "t" .. i,
                    url = "https://x.test/" .. i,
                    snippet = string.rep("x", contract.MAX_SNIPPET),
                }
            end
            local ok, err = contract.validate(p)
            assert.falsy(ok)
            assert.truthy(err:find("cap is"))
        end)

        it("rejects a non-string degraded entry", function()
            local p = base()
            p.degraded = { { "brave", "Too many requests" } }
            local ok, err = contract.validate(p)
            assert.falsy(ok)
            assert.truthy(err:find("degraded"))
        end)

        it("rejects a non-numeric score and an empty engines list", function()
            local p = base()
            p.results[1].score = "2.67"
            assert.falsy(contract.validate(p))
            p.results[1].score = nil
            p.results[1].engines = {}
            assert.falsy(contract.validate(p))
        end)
    end)
end)
