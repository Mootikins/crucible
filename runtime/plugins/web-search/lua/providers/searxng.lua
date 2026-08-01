--- web-search provider: SearXNG.
---
--- `GET {url}/search?q=…&format=json` against an instance the user configured
--- and, ideally, hosts. There is no shipped instance list: nine public ones
--- were tested and all nine returned 429/403 or an anti-bot challenge, so a
--- default pool would be a feature that fails most of the time — and worse,
--- fails intermittently.
---
--- Returns `payload, nil` on success, where `payload` is whatever
--- `contract.normalise` produced, or `nil, err` where `err` is a table:
---
---     { provider = "searxng", reason = "json_disabled",
---       message = "…", instance = "http://localhost:8888", status = 403 }
---
--- Never a bare nil. `reason` is the machine-readable half so a chain can tell
--- "skip this provider, it is not configured" from "this provider is
--- misconfigured and the user needs to hear about it"; `message` is the half a
--- model or a user reads.

local contract = require("contract")

local PROVIDER = "searxng"

-- Long enough for a metasearch fan-out (SearXNG is waiting on its own upstream
-- engines), short enough that a dead instance does not hold up the chain.
local DEFAULT_TIMEOUT = 15

-- No Lua binding exposes the Crucible version, and a hardcoded one would go
-- stale silently. This still does the job a User-Agent is for here: an operator
-- who wants to identify or block Crucible traffic can. No browser spoofing —
-- being blocked is the correct outcome of someone choosing to block us.
local USER_AGENT = "crucible/web-search"

-- Substrings of the interstitials public instances serve instead of results.
-- Checked before the format=json advice, because an anti-bot page is also HTML
-- and telling that user to edit a settings.yml they do not own would be
-- confidently wrong.
local CHALLENGE_MARKERS = {
    "verifying your browser",
    "not a bot",
    "just a moment",
    "captcha",
    "enable javascript",
}

-- ============================================================================
-- Helpers
-- ============================================================================

--- A base URL may carry basic-auth credentials (`https://user:pass@host`), and
--- transport errors quote the URL back verbatim. Strip the userinfo before any
--- string derived from the URL leaves this module.
local function redact(s)
    if type(s) ~= "string" then return s end
    return (s:gsub("://[^/@%s]*@", "://"))
end

local function trim(s)
    return (s:gsub("^%s+", ""):gsub("%s+$", ""))
end

--- Percent-encode a query component. Encoding per byte is what UTF-8 queries
--- need; `%w` under the C locale is ASCII-only, so multibyte characters fall
--- through to the escape branch a byte at a time, which is correct.
local function encode(s)
    return (s:gsub("[^%w%-%._~]", function(c)
        return string.format("%%%02X", string.byte(c))
    end))
end

local function fail(reason, message, extra)
    local err = { provider = PROVIDER, reason = reason, message = message }
    for k, v in pairs(extra or {}) do
        err[k] = v
    end
    return nil, err
end

local function header(headers, name)
    if type(headers) ~= "table" then return nil end
    for k, v in pairs(headers) do
        if type(k) == "string" and k:lower() == name then return v end
    end
    return nil
end

--- HTML where JSON was asked for. SearXNG aborts with a 403 *HTML error page*
--- when the requested format is not in `search.formats`, so the body, not the
--- status, is what identifies this.
local function is_html(body, headers)
    local ctype = header(headers, "content-type")
    if type(ctype) == "string" and ctype:lower():find("text/html", 1, true) then
        return true
    end
    local head = body:sub(1, 512):lower()
    return head:find("<!doctype html", 1, true) ~= nil or head:find("<html", 1, true) ~= nil
end

local function is_challenge(body)
    local head = body:sub(1, 4096):lower()
    for _, marker in ipairs(CHALLENGE_MARKERS) do
        if head:find(marker, 1, true) then return true end
    end
    return false
end

local function decode_json(body)
    local ok, decoded = pcall(cru.json.decode, body)
    if ok and type(decoded) == "table" then return decoded end
    return nil
end

-- ============================================================================
-- The provider
-- ============================================================================

--- Search a SearXNG instance.
---
--- `opts.url` is the instance base URL (required — there is no default).
--- `opts.timeout` seconds, `opts.max_results` forwarded to the contract, and
--- `opts.headers` merged in for instances behind an auth proxy. Nothing from
--- `opts.headers` is ever echoed back: a bearer token in an error message is a
--- credential leak into model context.
return function(query, opts)
    opts = opts or {}

    if type(query) ~= "string" or trim(query) == "" then
        return fail("invalid_query", "searxng: query must be a non-empty string")
    end

    local base = type(opts.url) == "string" and trim(opts.url) or ""
    base = base:gsub("/+$", "")
    if base == "" then
        return fail(
            "unconfigured",
            "searxng: no instance URL configured. Point it at an instance you host — "
                .. "public instances rate-limit or challenge automated queries."
        )
    end
    local instance = redact(base)

    local headers = { ["User-Agent"] = USER_AGENT, Accept = "application/json" }
    for k, v in pairs(opts.headers or {}) do
        headers[k] = v
    end

    local timeout = math.floor(tonumber(opts.timeout) or DEFAULT_TIMEOUT)
    if timeout < 1 then timeout = DEFAULT_TIMEOUT end

    local url = string.format("%s/search?q=%s&format=json", base, encode(trim(query)))
    local response = http.get(url, { headers = headers, timeout = timeout })

    -- The response's `ok` flag is a convenience over the status; the status is
    -- the fact. A transport failure (DNS, refused, timed out) reports status 0
    -- and carries the reason in `error`.
    local status = tonumber(response.status) or 0
    if status == 0 then
        return fail(
            "transport",
            string.format(
                "searxng: could not reach %s (%s). Timed out after %ds, or the instance is down.",
                instance,
                redact(response.error) or "no response",
                timeout
            ),
            { instance = instance }
        )
    end

    local body = type(response.body) == "string" and response.body or ""

    if is_html(body, response.headers) then
        if is_challenge(body) then
            return fail(
                "blocked",
                string.format(
                    "searxng: %s served an anti-bot challenge instead of results (HTTP %d). "
                        .. "Public instances defend the JSON API precisely because it is the "
                        .. "automation-friendly one; use an instance you host.",
                    instance,
                    status
                ),
                { instance = instance, status = status }
            )
        end
        return fail(
            "json_disabled",
            string.format(
                "searxng: %s returned HTML, not JSON (HTTP %d) — the JSON API is disabled. "
                    .. "SearXNG ships with `search.formats: [html]`; add `json` to it in the "
                    .. "instance's settings.yml:\n"
                    .. "    search:\n      formats:\n        - html\n        - json\n"
                    .. "then restart the instance.",
                instance,
                status
            ),
            { instance = instance, status = status }
        )
    end

    if status < 200 or status >= 300 then
        return fail(
            "http_error",
            string.format("searxng: %s returned HTTP %d", instance, status),
            { instance = instance, status = status }
        )
    end

    local decoded = decode_json(body)
    if not decoded or type(decoded.results) ~= "table" then
        return fail(
            "bad_response",
            string.format(
                "searxng: %s returned HTTP %d with a body that is not a SearXNG JSON "
                    .. "search response",
                instance,
                status
            ),
            { instance = instance, status = status }
        )
    end

    -- SearXNG hits are forwarded whole: `content` is one of the contract's
    -- snippet aliases and the contract's allowlist discards the other 18
    -- fields, so a rename pass here would only be a second place to maintain.
    -- Zero results is a real answer, not a failure — the payload says so.
    local payload, err = contract.normalise({
        query = query,
        provider = PROVIDER,
        results = decoded.results,
        degraded = decoded.unresponsive_engines,
        max_results = opts.max_results,
    })
    if not payload then
        return fail("bad_response", "searxng: " .. tostring(err), { instance = instance })
    end

    -- Deliberately not run through `contract.validate` on the way out: a single
    -- pathological hit can legitimately exceed the byte cap (normalise keeps one
    -- result rather than returning an empty list), and validate rejects that.
    -- Turning a usable answer into an error would be the worse failure.
    return payload
end
