pub(super) const LUA_TEST_MOCKS: &str = r#"
local test_mocks = {}
local _calls = {}
local _fixtures = {}

--- The host's real `cru.fs.mkdir`, captured while THIS CHUNK loads.
---
--- A test file cannot capture it: the runner calls `test_mocks.setup()` before
--- it loads any test file (`server/lua_plugin_suite.rs`), so a test-file-level
--- `local real = cru.fs.mkdir` captures the mock. This chunk runs earlier, at
--- `install_test_harness()`, while the table is still the real one.
local _host_mkdir = cru and cru.fs and cru.fs.mkdir

local function record_call(module, method, ...)
    if not _calls[module] then _calls[module] = {} end
    if not _calls[module][method] then _calls[module][method] = {} end
    table.insert(_calls[module][method], { ... })
end

local function default_fixtures()
    return {
        -- `roots` maps a kiln NAME to a directory, which is the one thing
        -- `cru.kiln.path` does. Empty by default: a test that stages files
        -- names its own directory, so nothing writes to a guessed path.
        kiln = { notes = {}, outlinks = {}, backlinks = {}, neighbors = {}, roots = {} },
        http = { responses = {} },
        -- `real_dirs` makes the `mkdir` mock create the directory for real, as
        -- well as recording the call. A plugin that writes with `io.open`
        -- needs a real directory under it; an in-memory `dirs` table is not
        -- one. Off by default, so no suite touches the disk by accident.
        fs = { files = {}, dirs = {}, real_dirs = false },
        -- Absolute by default: a plugin that resolves its files against the
        -- kiln has to be testable without the assertion depending on where the
        -- daemon happened to be started.
        paths = { kiln = "/mock/kiln", workspace = "/mock/workspace", session = false, state = "/mock/state" },
        session = { temperature = 0.7, max_tokens = nil, model = "mock-model", mode = "act", thinking_budget = nil },
        -- `info` mirrors the bridge's `get_session` payload: kiln NAMES in a
        -- `kilns` array, never kiln paths (see session_bridge.rs).
        sessions = {
            info = { id = "mock-session", session_type = "chat", state = "active", kilns = { "mock-kiln" } },
            messages = {},
            response_parts = {},
        },
    }
end

local function deep_copy(orig)
    if type(orig) ~= "table" then return orig end
    local copy = {}
    for k, v in pairs(orig) do copy[k] = deep_copy(v) end
    return copy
end

local function link_lookup(mod_name, fixture_data, field)
    return function(path, ...)
        record_call(mod_name, field, path, ...)
        local map = fixture_data[field] and fixture_data[field][path]
        return map and deep_copy(map) or {}
    end
end

local function note_search(mod_name, fixture_data, method_name, score)
    return function(query, opts)
        record_call(mod_name, method_name, query, opts)
        local results = {}
        local limit = (opts and opts.limit) or 100
        local count = 0
        for _, note in ipairs(fixture_data.notes or {}) do
            if count >= limit then break end
            local searchable = (note.title or "") .. " " .. (note.content or "")
            if string.find(searchable:lower(), query:lower(), 1, true) then
                table.insert(results, { path = note.path, score = score })
                count = count + 1
            end
        end
        return results
    end
end

local function create_kiln_mock(fixtures)
    local f = fixtures.kiln
    return {
        list = function(limit)
            record_call("kiln", "list", limit)
            local notes = f.notes or {}
            if limit and limit < #notes then
                local result = {}
                for i = 1, limit do result[i] = deep_copy(notes[i]) end
                return result
            end
            return deep_copy(notes)
        end,
        get = function(path)
            record_call("kiln", "get", path)
            for _, note in ipairs(f.notes or {}) do
                if note.path == path then return deep_copy(note) end
            end
            return nil
        end,
        --- Mirrors `crucible-lua/src/vault/mod.rs`: an unknown name and a
        --- traversing relative part both RAISE, so a plugin cannot pass here
        --- and fail in production.
        path = function(name, relative)
            record_call("kiln", "path", name, relative)
            local root = (f.roots or {})[name]
            if not root then error("kiln '" .. tostring(name) .. "' is not registered") end
            if relative == nil or relative == "" then return root end
            if type(relative) ~= "string" or relative:sub(1, 1) == "/" then
                error("cru.kiln.path: the relative part must be plain components")
            end
            for part in relative:gmatch("[^/]+") do
                if part == "." or part == ".." then
                    error("cru.kiln.path: the relative part must be plain components")
                end
            end
            return root .. "/" .. relative
        end,
        search = note_search("kiln", f, "search", 1.0),
        outlinks = link_lookup("kiln", f, "outlinks"),
        backlinks = link_lookup("kiln", f, "backlinks"),
        neighbors = link_lookup("kiln", f, "neighbors"),
    }
end

local function create_http_mock(fixtures)
    local default_resp = { status = 200, body = "", ok = true, headers = {} }
    local function respond(method, url, opts)
        record_call("http", method, url, opts)
        local r = (fixtures.http.responses or {})[url] or default_resp
        return { status = r.status or 200, body = r.body or "", ok = r.ok ~= false, headers = r.headers or {} }
    end
    return {
        get = function(url, opts) return respond("get", url, opts) end,
        post = function(url, opts) return respond("post", url, opts) end,
        put = function(url, opts) return respond("put", url, opts) end,
        delete = function(url, opts) return respond("delete", url, opts) end,
        request = function(opts)
            local url = opts and opts.url or ""
            return respond("request", url, opts)
        end,
    }
end

local function create_fs_mock(fixtures)
    local files = {}
    local dirs = {}
    for k, v in pairs(fixtures.fs.files or {}) do files[k] = v end
    for k, v in pairs(fixtures.fs.dirs or {}) do dirs[k] = v end
    return {
        read = function(path)
            record_call("fs", "read", path)
            if files[path] ~= nil then return files[path] end
            error("File not found: " .. path)
        end,
        write = function(path, content)
            record_call("fs", "write", path, content)
            files[path] = content
        end,
        exists = function(path)
            record_call("fs", "exists", path)
            return files[path] ~= nil or dirs[path] ~= nil
        end,
        mkdir = function(path)
            record_call("fs", "mkdir", path)
            dirs[path] = true
            if fixtures.fs.real_dirs and _host_mkdir then
                _host_mkdir(path)
            end
        end,
        list = function(path)
            record_call("fs", "list", path)
            local result = {}
            local prefix = path
            if prefix:sub(-1) ~= "/" then prefix = prefix .. "/" end
            for k in pairs(files) do
                if k:sub(1, #prefix) == prefix then
                    local rest = k:sub(#prefix + 1)
                    if not rest:find("/") then table.insert(result, rest) end
                end
            end
            for k in pairs(dirs) do
                if k:sub(1, #prefix) == prefix then
                    local rest = k:sub(#prefix + 1)
                    if rest ~= "" and not rest:find("/") then table.insert(result, rest) end
                end
            end
            return result
        end,
    }
end

--- `cru.paths` — mirrors `crucible-lua/src/paths.rs`, where each accessor
--- RAISES when its path is not configured rather than returning nil. A plugin
--- that pcalls `kiln()` and falls back to the workspace has to be exercised
--- against that same shape, or the fallback runs first in production.
---
--- `false`, not `nil`, marks a path unconfigured: a Lua table cannot hold a nil
--- value, so `test_mocks.setup({paths = {kiln = nil}})` is indistinguishable
--- from passing no override and would silently leave the default in place.
local function create_paths_mock(fixtures)
    local f = fixtures.paths
    local function accessor(name)
        return function()
            record_call("paths", name)
            if f[name] == nil or f[name] == false then
                error(name .. " path not configured")
            end
            return f[name]
        end
    end
    return {
        kiln = accessor("kiln"),
        workspace = accessor("workspace"),
        session = accessor("session"),
        state = function(plugin)
            record_call("paths", "state", plugin)
            if not f.state then error("state path not configured") end
            return f.state .. "/" .. plugin
        end,
    }
end

local function create_session_mock(fixtures)
    local state = {
        temperature = fixtures.session.temperature,
        max_tokens = fixtures.session.max_tokens,
        model = fixtures.session.model or "mock-model",
        mode = fixtures.session.mode or "act",
        thinking_budget = fixtures.session.thinking_budget,
    }
    local session = {}
    for _, field in ipairs({"temperature", "max_tokens", "model", "mode", "thinking_budget"}) do
        session["get_" .. field] = function()
            record_call("session", "get_" .. field)
            return state[field]
        end
        session["set_" .. field] = function(val)
            record_call("session", "set_" .. field, val)
            state[field] = val
        end
    end
    return session
end

-- `cru.sessions` — the subagent-delegation API. Plugins that spin up a helper
-- session (kiln-expert's search, reflection's review pass) are untestable
-- without it: the real module is registered by the daemon, not by the bare
-- executor the plugin test runner builds.
local function create_sessions_mock(fixtures)
    local f = fixtures.sessions
    local counter = 0
    return {
        create = function(opts)
            record_call("sessions", "create", opts)
            counter = counter + 1
            return { id = string.format("mock-session-%d", counter) }, nil
        end,
        get = function(id)
            record_call("sessions", "get", id)
            return deep_copy(f.info)
        end,
        messages = function(id, opts)
            record_call("sessions", "messages", id, opts)
            return deep_copy(f.messages or {})
        end,
        configure_agent = function(id, config)
            record_call("sessions", "configure_agent", id, config)
        end,
        -- Returns an iterator over response parts, exhausting to nil.
        send_and_collect = function(id, prompt, opts)
            record_call("sessions", "send_and_collect", id, prompt, opts)
            local parts = f.response_parts or {}
            local i = 0
            return function()
                i = i + 1
                return deep_copy(parts[i])
            end, nil
        end,
        end_session = function(id)
            record_call("sessions", "end_session", id)
        end,
    }
end

function test_mocks.setup(overrides)
    overrides = overrides or {}
    _fixtures = default_fixtures()
    for module, config in pairs(overrides) do
        if _fixtures[module] then
            for k, v in pairs(config) do _fixtures[module][k] = v end
        end
    end
    _calls = {}
    cru = cru or {}
    cru.kiln = create_kiln_mock(_fixtures)
    cru.http = create_http_mock(_fixtures)
    cru.fs = create_fs_mock(_fixtures)
    cru.paths = create_paths_mock(_fixtures)
    cru.session = create_session_mock(_fixtures)
    cru.sessions = create_sessions_mock(_fixtures)
end

function test_mocks.reset()
    _calls = {}
    _fixtures = default_fixtures()
    test_mocks.setup()
end

function test_mocks.get_calls(module, method)
    if not _calls[module] then return {} end
    if not _calls[module][method] then return {} end
    return _calls[module][method]
end

_G.test_mocks = test_mocks
"#;
