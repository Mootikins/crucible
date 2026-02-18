local test_mocks = {}
local _calls = {}
local _fixtures = {}

local function record_call(module, method, ...)
    if not _calls[module] then _calls[module] = {} end
    if not _calls[module][method] then _calls[module][method] = {} end
    table.insert(_calls[module][method], { ... })
end

local function default_fixtures()
    return {
        kiln = { notes = {}, outlinks = {}, backlinks = {}, neighbors = {} },
        graph = { notes = {}, outlinks = {}, backlinks = {}, neighbors = {} },
        http = { responses = {} },
        fs = { files = {}, dirs = {} },
        session = { temperature = 0.7, max_tokens = nil, model = "mock-model", mode = "act", thinking_budget = nil },
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
        search = note_search("kiln", f, "search", 1.0),
        outlinks = link_lookup("kiln", f, "outlinks"),
        backlinks = link_lookup("kiln", f, "backlinks"),
        neighbors = link_lookup("kiln", f, "neighbors"),
    }
end

local function create_graph_mock(fixtures)
    local f = fixtures.graph
    return {
        get_note = function(path)
            record_call("graph", "get_note", path)
            for _, note in ipairs(f.notes or {}) do
                if note.path == path then return deep_copy(note) end
            end
            return nil
        end,
        get_outlinks = link_lookup("graph", f, "outlinks"),
        get_backlinks = link_lookup("graph", f, "backlinks"),
        get_neighbors = link_lookup("graph", f, "neighbors"),
        search_semantic = note_search("graph", f, "search_semantic", 0.9),
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
    cru.graph = create_graph_mock(_fixtures)
    cru.http = create_http_mock(_fixtures)
    cru.fs = create_fs_mock(_fixtures)
    cru.session = create_session_mock(_fixtures)
    http = cru.http
    fs = cru.fs
    if crucible then
        crucible.kiln = cru.kiln
        crucible.graph = cru.graph
        crucible.http = cru.http
        crucible.fs = cru.fs
        crucible.session = cru.session
    end
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
