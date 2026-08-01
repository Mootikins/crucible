--- web-search — the normalised result contract.
---
--- Every provider adapter (searxng, ddg, exa, …) ends in `contract.normalise`,
--- and nothing else is allowed to construct the tool's result. The point is
--- that the model sees one shape regardless of which provider answered:
--- output whose shape depends on runtime provider selection is the worst
--- possible property for a model to reason against.
---
---     {
---       query    = "rust prompt caching",
---       provider = "searxng",
---       results  = {
---         { title = "…", url = "https://…", snippet = "…",
---           score = 2.67, engines = { "duckduckgo", "google" } },
---       },
---       degraded = { "brave", "startpage" },
---     }
---
--- `degraded` is ALWAYS present, empty when the provider has no way to report
--- partial failure. Only SearXNG has an equivalent (`unresponsive_engines`);
--- if the key were absent for DDG and Exa a model would read meaning into the
--- difference. **The absence of `degraded` must never be a signal.**
---
--- `score` and `engines` are the deliberate exception, and the rule above was
--- once written broadly enough to forbid them. They are SearXNG-only and
--- absent elsewhere, so their presence does identify the provider — but
--- `provider` is already in the payload, so nothing is leaked that the model
--- could not already read. Carrying them is worth it: cross-engine agreement
--- is real relevance signal no single-provider API can give. The rule they
--- must obey is different — a *comparable* one. Exa returns a 0–1 neural
--- relevance and SearXNG a cross-engine agreement sum; publishing both under
--- `score` would invite comparison across incompatible scales, so the Exa
--- adapter drops its own.
---
--- This module is pure: no network, no config, no globals beyond `cru.json` for
--- measuring the encoded size. Provider transport and parsing live elsewhere.

local M = {}

-- Per-result snippet cap. Titles and URLs are short and load-bearing; the
-- snippet is the only field that can run away.
M.MAX_SNIPPET = 300

--- A title is a label, not content. Left unbounded, one pathological hit could
--- push the payload past MAX_PAYLOAD_BYTES on its own — and the tail-trim below
--- keeps one result unconditionally, so there was no backstop.
M.MAX_TITLE = 200

--- URLs are NOT truncated: a cut URL is unfollowable, which is worse than no
--- result. One longer than this is dropped instead. The bound is generous —
--- real URLs sit far below it, and the shapes that exceed it are `data:` blobs
--- and tracking redirects carrying an entire payload in a query parameter.
M.MAX_URL = 2048

-- Total serialised budget. Tool output over ~10KB is spilled to a file and
-- replaced by a path reference, and the threshold is measured on the
-- JSON-*escaped* string, so real content spills nearer 7–9KB. Search results
-- are exactly the kind of result that must arrive inline, so stay well under:
-- 6KB leaves headroom for escaping in non-ASCII snippets.
M.MAX_PAYLOAD_BYTES = 6144

-- Ceiling on the `max_results` parameter. Note 25 × 300 chars does not fit in
-- MAX_PAYLOAD_BYTES — the byte cap is the real bound and will trim. This is the
-- limit on what a caller may *ask* for, not on what it will get.
M.MAX_RESULTS = 25

M.DEFAULT_RESULTS = 8

-- The complete set of keys a result row may carry. Anything else is dropped on
-- normalise and rejected on validate. SearXNG alone returns 23 fields per hit
-- (`audio_src`, `iframe_src`, `thumbnail`, `parsed_url`, `positions`, …);
-- forwarding those is the textbook "low-signal fields inflate token count"
-- mistake, and an allowlist is what stops a new provider quietly widening the
-- contract.
local RESULT_KEYS = { title = true, url = true, snippet = true, score = true, engines = true }
local PAYLOAD_KEYS = { query = true, provider = true, results = true, degraded = true }

-- Providers name the snippet field differently: SearXNG `content`, Exa `text`,
-- assorted REST APIs `description`. Aliasing here keeps the adapters to pure
-- transport instead of each inventing its own rename step.
local SNIPPET_KEYS = { "snippet", "content", "text", "description" }

-- ============================================================================
-- Helpers
-- ============================================================================

--- Collapse runs of whitespace and trim. HTML-scraped snippets arrive full of
--- newlines and indentation, which costs tokens and reads badly in a prompt.
local function squeeze(s)
    return (s:gsub("%s+", " "):gsub("^ ", ""):gsub(" $", ""))
end

--- Truncate to `limit` bytes without splitting a UTF-8 codepoint. A split
--- codepoint produces invalid UTF-8, which breaks JSON encoding downstream —
--- so back off over continuation bytes (0x80–0xBF) before appending the marker.
local function truncate(s, limit)
    if #s <= limit then return s end
    local cut = limit - 3
    while cut > 0 do
        local b = s:byte(cut + 1)
        if not b or b < 0x80 or b > 0xBF then break end
        cut = cut - 1
    end
    return s:sub(1, cut) .. "..."
end

--- Encoded size in bytes, measured the same way the spill threshold measures
--- it. Falls back to an estimate only where `cru.json` is absent (the
--- spec-extraction sandbox stubs the `cru` namespace); rawget sees through
--- neither a metatable nor a missing global.
local function encoded_size(payload)
    local cru = rawget(_G, "cru")
    local json = type(cru) == "table" and rawget(cru, "json") or nil
    local encode = type(json) == "table" and rawget(json, "encode") or nil
    if type(encode) == "function" then
        local ok, s = pcall(encode, payload)
        if ok and type(s) == "string" then return #s end
    end

    local n = 32 -- envelope braces and the query/provider/degraded keys
    n = n + #payload.query + #payload.provider
    for _, name in ipairs(payload.degraded) do n = n + #name + 3 end
    for _, r in ipairs(payload.results) do
        -- 48 covers the key names, quotes, commas and braces of one row;
        -- doubling the snippet is the pessimistic escaping allowance.
        n = n + 48 + #r.title + #r.url + (#r.snippet * 2)
        if r.engines then
            for _, e in ipairs(r.engines) do n = n + #e + 3 end
        end
    end
    return n
end

--- Pull the snippet out of whichever key this provider used.
local function snippet_of(row)
    for _, key in ipairs(SNIPPET_KEYS) do
        local v = row[key]
        if type(v) == "string" and v ~= "" then return v end
    end
    return ""
end

--- SearXNG reports `unresponsive_engines` as `{ {name, reason}, … }`; other
--- providers, when they report at all, use bare names. Both flatten to names:
--- the reason is diagnostic detail the model cannot act on, and the field's
--- job is to say "these did not answer, so coverage is partial".
local function degraded_names(raw)
    local out = {}
    if type(raw) ~= "table" then return out end
    for _, entry in ipairs(raw) do
        local name
        if type(entry) == "string" then
            name = entry
        elseif type(entry) == "table" then
            name = entry[1] or entry.name or entry.engine
        end
        if type(name) == "string" and name ~= "" then
            out[#out + 1] = squeeze(name)
        end
    end
    return out
end

--- Keep only string entries, and only when there is at least one.
local function engines_of(row)
    if type(row.engines) ~= "table" then return nil end
    local out = {}
    for _, e in ipairs(row.engines) do
        if type(e) == "string" and e ~= "" then out[#out + 1] = e end
    end
    return #out > 0 and out or nil
end

local function is_array(t)
    if type(t) ~= "table" then return false end
    local n = 0
    for k in pairs(t) do
        if type(k) ~= "number" then return false end
        n = n + 1
    end
    return n == #t
end

-- ============================================================================
-- The contract
-- ============================================================================

--- Mark a table as a JSON list.
---
--- Lua cannot distinguish an empty list from an empty map, and the encoder
--- resolves the ambiguity as a map. Without this, `results` and `degraded`
--- reach the model as `{}` when empty and `[...]` when populated — the JSON
--- type of the field tracking the data, which is exactly the instability this
--- contract exists to prevent. `cru.json.array` carries the intent on the
--- table, so it survives being empty.
---
--- Guarded: the spec-extraction sandbox and the bare Lua test harness have no
--- `cru`, and there the marking is a no-op that only affects encoding.
local function as_list(t)
    local cru_ns = rawget(_G, "cru")
    local json = cru_ns and rawget(cru_ns, "json")
    local array = json and rawget(json, "array")
    if type(array) == "function" then
        return array(t)
    end
    return t
end

--- Normalise one provider's parsed response into the contract shape.
---
--- `input.results` are rows the provider adapter has already decoded from its
--- wire format (JSON object, JSON-RPC envelope, scraped HTML). Rows may carry
--- any keys at all — only the five in RESULT_KEYS survive, with the snippet
--- taken from whichever alias the provider used.
---
--- Returns `payload` or `nil, err`.
function M.normalise(input)
    if type(input) ~= "table" then
        return nil, "normalise expects a table"
    end
    if type(input.query) ~= "string" or input.query == "" then
        return nil, "query is required"
    end
    if type(input.provider) ~= "string" or input.provider == "" then
        return nil, "provider is required"
    end

    local limit = tonumber(input.max_results) or M.DEFAULT_RESULTS
    limit = math.max(1, math.min(math.floor(limit), M.MAX_RESULTS))

    local payload = {
        query = squeeze(input.query),
        provider = input.provider,
        results = {},
        degraded = degraded_names(input.degraded),
    }
    local dropped_oversize = 0
    local dropped_scheme = 0

    for _, row in ipairs(input.results or {}) do
        if #payload.results >= limit then break end
        if type(row) == "table" then
            local title = type(row.title) == "string" and squeeze(row.title) or ""
            local url = type(row.url) == "string" and squeeze(row.url) or ""
            -- A hit with no URL is not a search result; a hit with no title is
            -- unciteable. Dropping beats emitting a row the model must guess at.
            -- An over-long URL is dropped rather than cut, for the same reason:
            -- a truncated URL looks usable and is not.
            -- Scheme allowlist. Nothing in the pipeline dereferences a result
            -- URL today, but the model reads them as citations and a future
            -- fetch tool would follow them, so `file://`, `data:` and
            -- `javascript:` must never survive normalisation. Anchored to the
            -- start so `https://x/?u=file:///etc` is unaffected.
            local scheme_ok = url:match("^https?://") ~= nil
            if title ~= "" and url ~= "" and not scheme_ok then
                dropped_scheme = dropped_scheme + 1
            elseif title ~= "" and url ~= "" and #url > M.MAX_URL then
                -- Dropped, not truncated — but silence would read as "no
                -- matches", which is a different and wrong answer. `degraded`
                -- is the channel for "the answer is partial and here is what
                -- was lost", so the loss is stated rather than inferred.
                dropped_oversize = dropped_oversize + 1
            elseif title ~= "" and url ~= "" then
                payload.results[#payload.results + 1] = {
                    title = truncate(title, M.MAX_TITLE),
                    url = url,
                    snippet = truncate(squeeze(snippet_of(row)), M.MAX_SNIPPET),
                    score = type(row.score) == "number" and row.score or nil,
                    engines = engines_of(row),
                }
            end
        end
    end

    -- Trim from the tail until the encoded payload fits. Results are ordered by
    -- provider relevance, so the last is the cheapest to lose. One result is
    -- kept unconditionally: an empty result list would read as "no matches",
    -- which is a different and wrong answer.
    while #payload.results > 1 and encoded_size(payload) > M.MAX_PAYLOAD_BYTES do
        table.remove(payload.results)
    end

    -- Short labels, not sentences. `degraded` is rendered into the one-line
    -- display summary as a comma-joined list beside engine names like "brave",
    -- so a full sentence here garbled it and put two different kinds of value
    -- in one array. A label reads correctly in both places.
    if dropped_oversize > 0 then
        payload.degraded[#payload.degraded + 1] = "url-too-long:" .. dropped_oversize
    end
    if dropped_scheme > 0 then
        payload.degraded[#payload.degraded + 1] = "non-http-url:" .. dropped_scheme
    end

    -- Last, so trimming is done and the marking survives to the encoder.
    payload.results = as_list(payload.results)
    payload.degraded = as_list(payload.degraded)

    return payload
end

--- Validate a payload against the contract. Returns `true` or `false, err`.
---
--- Unknown keys are an error, not a warning: silent widening is exactly how a
--- second provider drifts the shape, and this is the check that catches it.
function M.validate(payload)
    if type(payload) ~= "table" then
        return false, "payload must be a table"
    end
    for k in pairs(payload) do
        if not PAYLOAD_KEYS[k] then
            return false, "unknown payload key: " .. tostring(k)
        end
    end
    if type(payload.query) ~= "string" or payload.query == "" then
        return false, "query must be a non-empty string"
    end
    if type(payload.provider) ~= "string" or payload.provider == "" then
        return false, "provider must be a non-empty string"
    end
    if not is_array(payload.results) then
        return false, "results must be an array"
    end
    if not is_array(payload.degraded) then
        return false, "degraded must be an array (empty, never absent)"
    end
    for i, name in ipairs(payload.degraded) do
        if type(name) ~= "string" then
            return false, string.format("degraded[%d] must be a string", i)
        end
    end

    for i, r in ipairs(payload.results) do
        local function bad(msg) return false, string.format("results[%d]: %s", i, msg) end
        if type(r) ~= "table" then return bad("must be a table") end
        for k in pairs(r) do
            if not RESULT_KEYS[k] then return bad("unknown key: " .. tostring(k)) end
        end
        for _, k in ipairs({ "title", "url", "snippet" }) do
            if type(r[k]) ~= "string" then return bad(k .. " must be a string") end
        end
        if r.title == "" or r.url == "" then return bad("title and url must be non-empty") end
        if #r.snippet > M.MAX_SNIPPET then
            return bad(string.format("snippet exceeds %d bytes", M.MAX_SNIPPET))
        end
        if r.score ~= nil and type(r.score) ~= "number" then
            return bad("score must be a number when present")
        end
        if r.engines ~= nil then
            if not is_array(r.engines) or #r.engines == 0 then
                return bad("engines must be a non-empty array when present")
            end
            for j, e in ipairs(r.engines) do
                if type(e) ~= "string" then
                    return bad(string.format("engines[%d] must be a string", j))
                end
            end
        end
    end

    local size = encoded_size(payload)
    if size > M.MAX_PAYLOAD_BYTES then
        return false, string.format("payload is %d bytes, cap is %d", size, M.MAX_PAYLOAD_BYTES)
    end

    return true
end

--- Encoded size in bytes, exposed so tests and adapters can assert the budget
--- without reaching into module internals.
M.encoded_size = encoded_size

return M
