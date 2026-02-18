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

local function create_kiln_mock(fixtures)
    local kiln = {}
    function kiln.list(limit)
        record_call("kiln", "list", limit)
        local notes = fixtures.kiln.notes or {}
        if limit and limit < #notes then
            local result = {}
            for i = 1, limit do result[i] = deep_copy(notes[i]) end
            return result
        end
        return deep_copy(notes)
    end
    function kiln.get(path)
        record_call("kiln", "get", path)
        for _, note in ipairs(fixtures.kiln.notes or {}) do
            if note.path == path then return deep_copy(note) end
        end
        return nil
    end
    function kiln.search(query, opts)
        record_call("kiln", "search", query, opts)
        local results = {}
        local limit = (opts and opts.limit) or 100
        local count = 0
        for _, note in ipairs(fixtures.kiln.notes or {}) do
            if count >= limit then break end
            local searchable = (note.title or "") .. " " .. (note.content or "")
            if string.find(searchable:lower(), query:lower(), 1, true) then
                table.insert(results, { path = note.path, score = 1.0 })
                count = count + 1
            end
        end
        return results
    end
    function kiln.outlinks(path)
        record_call("kiln", "outlinks", path)
        local links = fixtures.kiln.outlinks and fixtures.kiln.outlinks[path]
        return links and deep_copy(links) or {}
    end
    function kiln.backlinks(path)
        record_call("kiln", "backlinks", path)
        local links = fixtures.kiln.backlinks and fixtures.kiln.backlinks[path]
        return links and deep_copy(links) or {}
    end
    function kiln.neighbors(path, depth)
        record_call("kiln", "neighbors", path, depth)
        local links = fixtures.kiln.neighbors and fixtures.kiln.neighbors[path]
        return links and deep_copy(links) or {}
    end
    return kiln
end

local function create_graph_mock(fixtures)
    local graph = {}
    function graph.get_note(path)
        record_call("graph", "get_note", path)
        for _, note in ipairs(fixtures.graph.notes or {}) do
            if note.path == path then return deep_copy(note) end
        end
        return nil
    end
    function graph.get_outlinks(path)
        record_call("graph", "get_outlinks", path)
        local links = fixtures.graph.outlinks and fixtures.graph.outlinks[path]
        return links and deep_copy(links) or {}
    end
    function graph.get_backlinks(path)
        record_call("graph", "get_backlinks", path)
        local links = fixtures.graph.backlinks and fixtures.graph.backlinks[path]
        return links and deep_copy(links) or {}
    end
    function graph.get_neighbors(path, depth)
        record_call("graph", "get_neighbors", path, depth)
        local links = fixtures.graph.neighbors and fixtures.graph.neighbors[path]
        return links and deep_copy(links) or {}
    end
    function graph.search_semantic(query, opts)
        record_call("graph", "search_semantic", query, opts)
        local results = {}
        for _, note in ipairs(fixtures.graph.notes or {}) do
            local searchable = (note.title or "") .. " " .. (note.content or "")
            if string.find(searchable:lower(), query:lower(), 1, true) then
                table.insert(results, { path = note.path, score = 0.9 })
            end
        end
        return results
    end
    return graph
end

local function create_http_mock(fixtures)
    local mock_http = {}
    local default_response = { status = 200, body = "", ok = true, headers = {} }
    local function make_response(method, url, opts)
        record_call("http", method, url, opts)
        local resp = (fixtures.http.responses or {})[url] or default_response
        return { status = resp.status or 200, body = resp.body or "", ok = resp.ok ~= false, headers = resp.headers or {} }
    end
    function mock_http.get(url, opts) return make_response("get", url, opts) end
    function mock_http.post(url, opts) return make_response("post", url, opts) end
    function mock_http.put(url, opts) return make_response("put", url, opts) end
    function mock_http.delete(url, opts) return make_response("delete", url, opts) end
    function mock_http.request(opts)
        local url = opts and opts.url or ""
        record_call("http", "request", opts)
        local resp = (fixtures.http.responses or {})[url] or default_response
        return { status = resp.status or 200, body = resp.body or "", ok = resp.ok ~= false, headers = resp.headers or {} }
    end
    return mock_http
end

local function create_fs_mock(fixtures)
    local mock_fs = {}
    local files = {}
    local dirs = {}
    for k, v in pairs(fixtures.fs.files or {}) do files[k] = v end
    for k, v in pairs(fixtures.fs.dirs or {}) do dirs[k] = v end
    function mock_fs.read(path)
        record_call("fs", "read", path)
        if files[path] ~= nil then return files[path] end
        error("File not found: " .. path)
    end
    function mock_fs.write(path, content)
        record_call("fs", "write", path, content)
        files[path] = content
    end
    function mock_fs.exists(path)
        record_call("fs", "exists", path)
        return files[path] ~= nil or dirs[path] ~= nil
    end
    function mock_fs.mkdir(path)
        record_call("fs", "mkdir", path)
        dirs[path] = true
    end
    function mock_fs.list(path)
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
    end
    return mock_fs
end

local function create_session_mock(fixtures)
    local session = {}
    local state = {
        temperature = fixtures.session.temperature,
        max_tokens = fixtures.session.max_tokens,
        model = fixtures.session.model or "mock-model",
        mode = fixtures.session.mode or "act",
        thinking_budget = fixtures.session.thinking_budget,
    }
    function session.get_temperature() record_call("session", "get_temperature"); return state.temperature end
    function session.set_temperature(val) record_call("session", "set_temperature", val); state.temperature = val end
    function session.get_max_tokens() record_call("session", "get_max_tokens"); return state.max_tokens end
    function session.set_max_tokens(val) record_call("session", "set_max_tokens", val); state.max_tokens = val end
    function session.get_model() record_call("session", "get_model"); return state.model end
    function session.get_mode() record_call("session", "get_mode"); return state.mode end
    function session.set_mode(val) record_call("session", "set_mode", val); state.mode = val end
    function session.get_thinking_budget() record_call("session", "get_thinking_budget"); return state.thinking_budget end
    function session.set_thinking_budget(val) record_call("session", "set_thinking_budget", val); state.thinking_budget = val end
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
