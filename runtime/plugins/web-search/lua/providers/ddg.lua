--- web-search provider: DuckDuckGo lite.
---
---     POST https://lite.duckduckgo.com/lite/   body: q=<urlencoded>
---
--- Keyless, no container, no account — which is why it is the out-of-box
--- default. It is also **HTML scraping**, so it will break when DuckDuckGo
--- changes the lite page. The whole design of this file follows from that:
---
---   * A parse that finds nothing is an ERROR, never an empty result set. A
---     silent `results = {}` after a markup change would look exactly like
---     "the web has nothing on this", and the model would believe it — for
---     every query, forever, with no signal that anything was wrong.
---   * Every failure message names the landmark that was missing, so whoever
---     reads it knows which selector to go re-record the fixture against.
---
--- The cost of that bias is that a genuinely empty result set is reported as a
--- failure too. That is the right way round: a loud false alarm is recoverable,
--- a silent wrong answer is not.
---
--- Adapts to `contract`; it does not redefine the shape. DDG lite has no
--- per-engine health reporting, so `degraded` comes out empty — present, as the
--- contract requires, because absence must never be a signal.

local contract = require("contract")

local M = {}

M.NAME = "ddg"
M.ENDPOINT = "https://lite.duckduckgo.com/lite/"

-- Seconds. The chain moves on rather than hanging; a search that takes longer
-- than this is worse than no search.
M.DEFAULT_TIMEOUT = 15

-- No browser spoofing. An operator who wants to block or rate-limit Crucible
-- should be able to, and that is correct behaviour. The binary's version is not
-- exposed to Lua, so this carries the plugin's.
M.USER_AGENT = "crucible-web-search/0.1"

-- ============================================================================
-- Failure
-- ============================================================================

--- Every exit from this module that is not a payload comes through here, so
--- there is exactly one error shape and it always names the provider.
---
--- Nothing that could carry a secret is ever interpolated: DDG lite is keyless,
--- and the response *body* is attacker-controlled text bound for model context,
--- so it is described (bytes, landmarks) and never quoted.
local function fail(fmt, ...)
    return nil, {
        provider = M.NAME,
        message = M.NAME .. ": " .. string.format(fmt, ...),
    }
end

-- ============================================================================
-- Text
-- ============================================================================

--- application/x-www-form-urlencoded, where a space is `+` rather than `%20`.
local function form_encode(s)
    return (s:gsub("[^%w%-%.%_%~]", function(c)
        if c == " " then return "+" end
        return string.format("%%%02X", c:byte())
    end))
end

local function percent_decode(s)
    return (s:gsub("%%(%x%x)", function(hex)
        return string.char(tonumber(hex, 16))
    end))
end

local ENTITIES = {
    amp = "&",
    lt = "<",
    gt = ">",
    quot = '"',
    apos = "'",
    nbsp = " ",
    mdash = "\u{2014}",
    ndash = "\u{2013}",
    middot = "\u{00B7}",
    hellip = "\u{2026}",
    lsquo = "\u{2018}",
    rsquo = "\u{2019}",
    ldquo = "\u{201C}",
    rdquo = "\u{201D}",
}

--- One pass, so a decoded `&` can never be re-read as the start of another
--- entity — `&amp;lt;` must end as the literal text `&lt;`, not as `<`.
--- An unrecognised name is left alone: a stray `&thorn;` in a snippet is a
--- visible artefact, which is better than dropping the text.
--- `utf8.char` RAISES on a codepoint above 0x7FFFFFFF, and remote HTML is free
--- to contain `&#99999999999;`. This function documents that it never raises
--- and is reached from `M.parse` and `unwrap_redirect`, both of which say the
--- same — so an out-of-range numeric entity is left as literal text rather
--- than decoded. A visible `&#99999999999;` in a snippet is a far better
--- outcome than an error escaping the adapter.
local MAX_CODEPOINT = 0x10FFFF

local function safe_char(n)
    if type(n) ~= "number" or n < 0 or n > MAX_CODEPOINT then return nil end
    local ok, ch = pcall(utf8.char, n)
    if ok then return ch end
    return nil
end

local function decode_entities(s)
    return (s:gsub("&(#?%w+);", function(name)
        local hex = name:match("^#[xX](%x+)$")
        if hex then return safe_char(tonumber(hex, 16)) end
        local dec = name:match("^#(%d+)$")
        if dec then return safe_char(tonumber(dec, 10)) end
        return ENTITIES[name]
    end))
end

--- Tags first, entities second. The other order would turn an escaped
--- `&lt;script&gt;` in the page text into a tag and then delete it.
local function clean(s)
    return decode_entities((s:gsub("<[^>]*>", "")))
end

--- lite wraps some hits in `//duckduckgo.com/l/?uddg=<percent-encoded target>`.
--- The wrapper is a tracking hop; the model should be given the real URL, since
--- that is what it will cite and what `web_fetch` would later dereference.
local function unwrap_redirect(href)
    -- The href is read out of raw markup, so its query separators arrive as
    -- `&amp;`. Left alone they would be handed to the model, and to any later
    -- fetch, as part of the URL.
    href = decode_entities(href)
    -- Only unwrap DuckDuckGo's OWN redirector. `uddg=` was matched on any href
    -- from any host, so an indexed page carrying that parameter could rewrite
    -- its own result URL to a domain it does not control — the model then cites
    -- attacker-authored title and snippet under, say, a docs domain. It also
    -- silently mangled benign URLs that happen to use `uddg` as a query
    -- parameter, which is a plain correctness bug with no attacker at all.
    local hostless = href:gsub("^https?:", ""):gsub("^//", "")
    local host = hostless:match("^([^/]+)")
    local is_ddg_redirector = host
        and (host == "duckduckgo.com" or host:sub(-14) == ".duckduckgo.com")
        and hostless:match("^[^/]+/l/")
    if is_ddg_redirector then
        local target = href:match("[?&]uddg=([^&\"]+)")
        if target then href = percent_decode(target) end
    end
    -- lite emits protocol-relative hrefs; a bare `//host/path` is not fetchable
    -- once it leaves the page it came from.
    if href:sub(1, 2) == "//" then href = "https:" .. href end
    return href
end

-- ============================================================================
-- Parsing
-- ============================================================================

--- The result anchor in one `<tr>`, matched attribute-order-independently.
--- Pinning `href` before `class` (the order lite happens to emit today) is the
--- kind of incidental coupling that breaks on a cosmetic template change.
local function result_link(chunk)
    for attrs, inner in chunk:gmatch("<a%s+([^>]*)>(.-)</a>") do
        if attrs:find("result%-link") then
            local href = attrs:match('href="([^"]*)"')
            if href and href ~= "" then
                return href, clean(inner)
            end
        end
    end
    return nil
end

--- Parse a lite result page into rows for `contract.normalise`.
---
--- Returns `rows` or `nil, why` — `why` is a bare string; `M.search` is what
--- wraps it into the structured provider error.
--- Bytes of response body this parser will look at.
---
--- The HTTP layer buffers a whole response with no cap
--- (`crucible-core/src/http.rs` builds a bare client and reads `.text()`), so
--- without a bound here the only limit on what gets parsed is what the far end
--- chooses to send. A lite page is a few tens of KB; anything past this is not
--- a page we can read anyway.
local MAX_BODY = 512 * 1024

--- Rows a single page may contribute. A lite page carries tens.
local MAX_ROWS = 500

--- Iterate `<tr>…</tr>` chunks in linear time.
---
--- This replaced `html:gmatch("<tr.-</tr>")`, which is accidentally QUADRATIC:
--- the lazy `.-` rescans to end-of-string from every `<tr` position when no
--- `</tr>` follows, and Lua's iterative min_expand raises no "pattern too
--- complex" guard, so it runs to completion. Measured on this machine with a
--- body of `<tr>` repeated and no closing tag: 16KB 0.17s, 32KB 0.68s, 64KB
--- 2.68s, 128KB 11.03s — 4x per doubling, so ~11 minutes at 1MB.
---
--- That is not merely slow. The parse is synchronous Lua inside one
--- `Thread::resume`, so the daemon's 30s dispatch timeout cannot preempt it —
--- `tokio::time::timeout` only fires at an await point — and mlua holds the
--- Lua lock for the whole poll, and every session shares one VM. A single
--- hostile response would stall plugin tools and hooks in every session.
---
--- `find(..., plain)` advances a cursor instead of backtracking, so the whole
--- scan is O(n) regardless of what the body contains.
local function rows_of(html)
    local pos, count = 1, 0
    return function()
        while count < MAX_ROWS do
            local open = html:find("<tr", pos, true)
            if not open then return nil end
            local _, close_end = html:find("</tr>", open, true)
            if not close_end then return nil end
            count = count + 1
            local chunk = html:sub(open, close_end)
            pos = close_end + 1
            return chunk
        end
        return nil
    end
end

function M.parse(html)
    if type(html) ~= "string" or html == "" then
        return nil, "empty response body"
    end

    -- The lite page always echoes the query back in its own POST form. Checking
    -- for it separates "DuckDuckGo answered and the markup moved" from "we were
    -- blocked, challenged or redirected somewhere else entirely" — two failures
    -- with completely different fixes, which one generic "parse failed" would
    -- merge.
    if not html:find('action="/lite/"', 1, true) then
        return nil, string.format(
            "response is not the DuckDuckGo lite page: no `action=\"/lite/\"` form in %d bytes "
                .. "(blocked, challenged or redirected)",
            #html
        )
    end

    if #html > MAX_BODY then
        return nil, string.format(
            "response body is %d bytes, over the %d-byte parse limit: refusing to scan it",
            #html,
            MAX_BODY
        )
    end

    local rows, pending, tr_count, sponsored = {}, nil, 0, 0
    for chunk in rows_of(html) do
        tr_count = tr_count + 1
        if chunk:find("result%-sponsored") then
            sponsored = sponsored + 1
            -- Clearing `pending` matters: if DuckDuckGo ever tags only an ad's
            -- first row, its snippet row would otherwise be attached to the
            -- previous organic hit, quietly putting ad copy under a real URL.
            pending = nil
        else
            local href, title = result_link(chunk)
            if href then
                pending = { title = title, url = unwrap_redirect(href) }
                rows[#rows + 1] = pending
            elseif pending then
                -- Quote-agnostic: lite serves `class='result-snippet'` with
                -- SINGLE quotes today, while the recorded fixture uses double.
                -- Matching only double quotes cost every live search its
                -- snippets — silently, because a snippet-less row is still a
                -- valid result, so none of the "did the markup change" guards
                -- fired. `result_link` above is already quote-agnostic about
                -- the class; only this matcher was not.
                local snippet = chunk:match("class=[\"']result%-snippet[\"'][^>]*>(.-)</td>")
                if snippet then
                    pending.snippet = clean(snippet)
                    pending = nil
                end
            end
        end
    end

    -- Zero rows is ambiguous: DuckDuckGo genuinely had nothing, or the markup
    -- this scraper depends on changed. The two need different answers, and
    -- collapsing them was a real contract break — searxng and exa both return
    -- an empty result set for "nothing found", while this returned an error, so
    -- the chain fell through and the model was told every provider failed when
    -- the honest answer was "the web has nothing on this".
    --
    -- A page with result table rows in it parsed fine and found nothing. A page
    -- with none at all is the shape having changed, and that is a failure worth
    -- reporting loudly.
    if #rows == 0 and tr_count == 0 then
        return nil, string.format(
            "found no result table rows in the lite page (%d bytes, %d sponsored): "
                .. "the `<a class=\"result-link\">` markup this scraper depends on "
                .. "has almost certainly changed",
            #html,
            sponsored
        )
    end

    -- Rows but no anchors is genuinely ambiguous: DuckDuckGo had nothing, or
    -- the anchor markup moved. Nothing in the page distinguishes them
    -- reliably, and guessing either way is worse than saying so — collapsing
    -- it to an error made "the web has nothing on this" inexpressible, while
    -- collapsing it to a clean empty would let a stale scraper report "no
    -- results" forever, silently. So: an empty answer, plus a note that this
    -- is what it looked like.
    if #rows == 0 then
        return rows, nil, "no-anchors"
    end

    return rows
end

-- ============================================================================
-- The provider
-- ============================================================================

--- Search DuckDuckGo lite.
---
---   query  string
---   opts   { timeout = seconds, max_results = n }
---
--- Returns a payload in the `contract` shape, or `nil, { provider, message }`.
--- Never a bare nil: a caller that only knows "ddg returned nothing" cannot
--- tell the model what was tried, and the model retries blindly.
function M.search(query, opts)
    opts = opts or {}

    if type(query) ~= "string" or query:match("^%s*$") then
        return fail("query must be a non-empty string")
    end

    local response = http.post(M.ENDPOINT, {
        headers = {
            ["Content-Type"] = "application/x-www-form-urlencoded",
            ["User-Agent"] = M.USER_AGENT,
            ["Accept"] = "text/html",
        },
        body = "q=" .. form_encode(query),
        timeout = tonumber(opts.timeout) or M.DEFAULT_TIMEOUT,
    })

    if type(response) ~= "table" then
        return fail("http.post returned %s, not a response table", type(response))
    end
    if not response.ok then
        -- `response.error` is the transport error (DNS, TLS, timeout) built by
        -- the host; it carries no request headers, so nothing sensitive travels
        -- in it. Rate limiting shows up here as a 429 and reads as such.
        return fail(
            "POST %s failed: %s",
            M.ENDPOINT,
            response.error or ("HTTP " .. tostring(response.status))
        )
    end

    local rows, why, degraded_note = M.parse(response.body or "")
    if not rows then
        return fail("%s", why)
    end

    local payload, err = contract.normalise({
        query = query,
        provider = M.NAME,
        results = rows,
        degraded = degraded_note and { degraded_note } or nil,
        max_results = opts.max_results,
    })
    if not payload then
        return fail("parsed %d results but the contract rejected them: %s", #rows, tostring(err))
    end

    -- The second half of "never silently empty": the parse found anchors, yet
    -- nothing survived normalisation. That is a markup change too — lite grew a
    -- link shape whose title or href is somewhere else.
    --
    -- Guarded on `#rows > 0`: with no anchors at all the emptiness is the
    -- ambiguous case handled in `M.parse`, and it already carries its note.
    -- Failing here as well would put back the defect that guard removed.
    if #rows > 0 and #payload.results == 0 then
        return fail(
            "parsed %d result links but none had both a title and a url, "
                .. "so the lite page's anchor markup has changed",
            #rows
        )
    end

    return payload
end

return M
