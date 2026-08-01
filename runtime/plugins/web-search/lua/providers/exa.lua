--- web-search provider: Exa, over its JSON-RPC-on-HTTPS endpoint.
---
--- `https://mcp.exa.ai/mcp` speaks JSON-RPC and answers `tools/call`
--- anonymously. This is *not* an MCP server registration: no server config, no
--- process lifecycle, no tool-namespace entry, nothing in `cru mcp list`. It is
--- one `http.post` behind one provider function, indistinguishable from the
--- other providers at the config surface.
---
--- Exa is a third party that sees the raw query, so this provider is opt-in and
--- never in the shipped default chain.
---
---     local exa = require("providers.exa")
---     local payload, err = exa("rust prompt caching", { max_results = 5 })
---
--- Returns the contract payload, or `nil` plus a structured error. Never a bare
--- nil, and never a raised error: the chain must be able to name which provider
--- failed and why in order to move on to the next one.

local contract = require("contract")

local NAME = "exa"
local ENDPOINT = "https://mcp.exa.ai/mcp"

-- Exa's tool name on that endpoint. Sent verbatim; a rename shows up as a
-- JSON-RPC "method not found"-shaped error rather than empty results.
local TOOL = "web_search_exa"

local DEFAULT_TIMEOUT = 15

-- Honest identification, no browser spoofing: an operator who wants to block or
-- rate-limit Crucible should be able to. The *binary* version is not visible
-- from Lua (there is no `cru.version` binding), so this is the plugin's own
-- version unless the caller passes a better one as `opts.user_agent`.
local VERSION = "0.1.0"

-- ============================================================================
-- Helpers
-- ============================================================================

--- Structured failure. `provider` is what the chain matches on; `error` is what
--- a human or a model reads, and it repeats the provider name because the
--- string is often all that survives to the surface.
local function fail(reason)
    return nil, { provider = NAME, error = NAME .. ": " .. reason }
end

--- Strip a configured key out of text before surfacing it. Nothing we build
--- carries the key into a message, but an endpoint can echo it back in an error
--- body, and error strings reach both the model's context and the daemon's log.
local function scrub(text, key)
    if type(text) ~= "string" then return nil end
    if type(key) == "string" and #key >= 8 then
        text = text:gsub((key:gsub("%W", "%%%0")), "<redacted>")
    end
    return text
end

--- One line of at most 160 bytes, safe to embed in an error message. Response
--- bodies can be an HTML error page; the first line of it is diagnostic, the
--- rest is noise the model would pay for.
local function brief(text, key)
    local s = scrub(text, key)
    if not s or s == "" then return nil end
    s = s:gsub("%s+", " "):gsub("^ ", ""):gsub(" $", "")
    if #s > 160 then s = s:sub(1, 157) .. "..." end
    return s ~= "" and s or nil
end

--- Pull the JSON document out of a response body.
---
--- Streamable-HTTP JSON-RPC endpoints may answer a POST with an SSE stream
--- instead of a JSON body — same envelope, wrapped in `event:`/`data:` lines —
--- and which one you get depends on content negotiation. Decoding only the
--- plain form would fail with "not JSON" against a server that is working
--- perfectly, so unwrap the frames when they are there.
local function decode_envelope(body)
    if type(body) ~= "string" or body == "" then return nil end

    local function try(s)
        local ok, decoded = pcall(cru.json.decode, s)
        if ok and type(decoded) == "table" then return decoded end
        return nil
    end

    if body:match("^%s*[{%[]") then return try(body) end

    local data = {}
    for line in body:gmatch("[^\r\n]+") do
        local payload = line:match("^data:%s?(.*)$")
        if payload then data[#data + 1] = payload end
    end
    if #data == 0 then return nil end

    -- A single event's data may be split over several lines (joined with "\n"),
    -- or the stream may carry several events. Try the whole thing first, then
    -- the last event, which is the one that answers our request.
    return try(table.concat(data, "\n")) or try(data[#data])
end

--- The text block of a JSON-RPC `tools/call` result. Exa returns its payload as
--- a JSON *string* inside that block rather than as structured content.
local function result_text(result)
    local content = type(result) == "table" and result.content or nil
    if type(content) ~= "table" then return nil end
    for _, block in ipairs(content) do
        if type(block) == "table" and type(block.text) == "string" and block.text ~= "" then
            return block.text
        end
    end
    return nil
end

-- ============================================================================
-- Provider
-- ============================================================================

--- Search Exa. `opts`:
---   max_results  number, clamped to the contract's ceiling
---   timeout      seconds, default 15
---   api_key      optional; Exa answers anonymously, a key buys rate limit
---   user_agent   overrides the default identification
return function(query, opts)
    opts = opts or {}

    if type(query) ~= "string" or query:match("^%s*$") then
        return fail("query must be a non-empty string")
    end

    local key = type(opts.api_key) == "string" and opts.api_key ~= "" and opts.api_key or nil
    local limit = tonumber(opts.max_results) or contract.DEFAULT_RESULTS
    limit = math.max(1, math.min(math.floor(limit), contract.MAX_RESULTS))

    -- http.request takes whole seconds; a fractional timeout would fail to
    -- convert on the Rust side rather than rounding.
    local timeout = math.max(1, math.floor(tonumber(opts.timeout) or DEFAULT_TIMEOUT))

    local headers = {
        ["Content-Type"] = "application/json",
        -- Accept both because a streamable-HTTP endpoint may reject a client
        -- that will not take an SSE reply; `decode_envelope` handles either.
        ["Accept"] = "application/json, text/event-stream",
        ["User-Agent"] = opts.user_agent or ("crucible/" .. VERSION),
    }
    if key then
        headers["Authorization"] = "Bearer " .. key
    end

    local body = cru.json.encode({
        jsonrpc = "2.0",
        id = 1,
        method = "tools/call",
        params = { name = TOOL, arguments = { query = query, numResults = limit } },
    })

    -- pcall because a raised error would escape the chain entirely: an argument
    -- the Rust side refuses to convert must still come back as this provider
    -- failing, not as the whole search dying.
    local ok, resp = pcall(http.post, ENDPOINT, {
        headers = headers,
        body = body,
        timeout = timeout,
    })
    if not ok then
        return fail("request could not be sent: " .. (brief(tostring(resp), key) or "unknown"))
    end
    if type(resp) ~= "table" then
        return fail("no response from " .. ENDPOINT)
    end

    if not resp.ok then
        local why = brief(resp.error or resp.body, key)
        local status = tonumber(resp.status) or 0
        if status > 0 then
            return fail(string.format(
                "%s returned HTTP %d%s",
                ENDPOINT,
                status,
                why and (": " .. why) or ""
            ))
        end
        return fail("request to " .. ENDPOINT .. " failed: " .. (why or "no response"))
    end

    local envelope = decode_envelope(resp.body)
    if not envelope then
        return fail("response was not JSON-RPC: " .. (brief(resp.body, key) or "empty body"))
    end

    if type(envelope.error) == "table" then
        local message = brief(envelope.error.message, key) or "no message"
        local code = tonumber(envelope.error.code)
        return fail(string.format("JSON-RPC error%s: %s", code and (" " .. code) or "", message))
    end

    local text = result_text(envelope.result)
    if type(envelope.result) == "table" and envelope.result.isError then
        return fail(TOOL .. " reported an error: " .. (brief(text, key) or "no detail"))
    end
    if not text then
        return fail("response carried no result content")
    end

    local payload_ok, inner = pcall(cru.json.decode, text)
    if not payload_ok or type(inner) ~= "table" or type(inner.results) ~= "table" then
        return fail("result content was not an Exa search payload")
    end

    local rows = {}
    for _, r in ipairs(inner.results) do
        if type(r) == "table" then
            -- Exa's `score` is a 0–1 neural relevance; SearXNG's is a
            -- cross-engine agreement sum. Publishing both under one key would
            -- invite the model to compare numbers that share no scale, so it is
            -- dropped here rather than at the contract.
            rows[#rows + 1] = { title = r.title, url = r.url, text = r.text }
        end
    end

    -- Exa has no equivalent of SearXNG's `unresponsive_engines`, so `degraded`
    -- comes back empty rather than absent — the contract's job, not this file's.
    -- normalise is the only constructor; validate is deliberately not re-run,
    -- because it rejects the single oversize row normalise keeps on purpose.
    local payload, norm_err = contract.normalise({
        query = query,
        provider = NAME,
        results = rows,
        max_results = limit,
    })
    if not payload then
        return fail("could not normalise results: " .. tostring(norm_err))
    end
    return payload
end
