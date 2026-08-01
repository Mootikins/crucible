--- Recorded provider payloads, kept as raw files rather than Lua strings so
--- they stay greppable, diffable and re-recordable with curl.
---
---   searxng_search.json  GET  {instance}/search?q=…&format=json
---                        23-field hits, plus unresponsive_engines with brave
---                        rate-limited and startpage serving a CAPTCHA while
---                        the query still returned 29 results.
---   exa_jsonrpc.json     POST https://mcp.exa.ai/mcp — a JSON-RPC tools/call
---                        response whose payload is JSON *inside* a text block.
---   ddg_lite.html        POST https://lite.duckduckgo.com/lite/ with q=…
---                        including a sponsored row and a /l/?uddg= redirect.
---
--- Re-record by replacing the file; nothing here parses them.

local M = {}

-- The suite has no notion of a working directory, so locate this directory the
-- same way `require` located this file. package.path is set identically by the
-- test harness and the runtime plugin loader, so this resolves in both.
local self_path = package.searchpath("web-search.tests.fixtures", package.path)
if not self_path then
    error("web-search fixtures: cannot locate tests/fixtures on package.path")
end
M.dir = self_path:gsub("[/\\]init%.lua$", "")

local cache = {}

--- Raw bytes of a fixture file. Deliberately `io.open` and not the `fs`
--- binding: the plugin test runner calls `test_mocks.setup()` before it loads
--- any suite, and that replaces the global `fs` with an in-memory mock whose
--- `read` errors on every real path. `fs.read` here made the whole suite fail
--- to load.
function M.raw(name)
    if not cache[name] then
        local path = M.dir .. "/" .. name
        local handle = assert(io.open(path, "rb"), "fixture not readable: " .. path)
        cache[name] = handle:read("a")
        handle:close()
    end
    return cache[name]
end

--- A fixture decoded as JSON. `oq` is registered by the daemon's plugin
--- runtime and is absent from the bare executor the test harness builds, so go
--- through cru.json, which setup_globals always installs.
function M.json(name)
    return cru.json.decode(M.raw(name))
end

return M
