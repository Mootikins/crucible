//! Pure Lua standard library utilities.
//!
//! Provides `cru.retry`, `cru.emitter`, and `cru.check` as embedded Lua source
//! loaded at executor init time. No new Rust code needed — these are pure Lua
//! building on the Rust-backed timer module.

use crate::lifecycle::{PluginErrorEntry, PluginErrorLog};
use mlua::{Lua, Result};
use std::sync::{Arc, Mutex};

const LUA_TEST_MOCKS: &str = r#"
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
"#;

const LUA_TEST_RUNNER: &str = r#"
-- test_runner.lua - Minimal test runner for Crucible plugins
--
-- Provides describe/it/before_each/after_each/pending globals and assert table.
-- No external dependencies — pure Lua with pcall/error only.

local COLORS = {
    reset = "\27[0m",
    green = "\27[32m",
    red = "\27[31m",
    yellow = "\27[33m",
}

local test_state = {
    suites = {},
    current_suite = nil,
    tests = {},
    before_each_stack = {},
    after_each_stack = {},
    results = {
        passed = 0,
        failed = 0,
        pending = 0,
        errors = {},
    },
}

local _original_assert = assert
local assert = setmetatable({}, {
    __call = function(_, ...)
        return _original_assert(...)
    end,
})

local function format_value(val)
    if type(val) == "string" then
        return '"' .. val .. '"'
    elseif type(val) == "table" then
        return "{...}"
    else
        return tostring(val)
    end
end

function assert.equal(expected, actual)
    if expected ~= actual then
        error(string.format(
            "Expected: %s\nActual: %s",
            format_value(expected),
            format_value(actual)
        ), 2)
    end
end

function assert.deep_equal(expected, actual)
    local function deep_eq(a, b, seen)
        seen = seen or {}
        if type(a) == "table" and type(b) == "table" then
            if seen[a] or seen[b] then
                return true
            end
            seen[a] = true
            seen[b] = true
        end
        if type(a) ~= type(b) then
            return false
        end
        if type(a) ~= "table" then
            return a == b
        end
        for k, v in pairs(a) do
            if not deep_eq(v, b[k], seen) then
                return false
            end
        end
        for k in pairs(b) do
            if a[k] == nil then
                return false
            end
        end
        return true
    end
    if not deep_eq(expected, actual) then
        error(string.format(
            "Expected: %s\nActual: %s",
            format_value(expected),
            format_value(actual)
        ), 2)
    end
end

function assert.truthy(val)
    if not val then
        error(string.format("Expected truthy value, got: %s", format_value(val)), 2)
    end
end

function assert.falsy(val)
    if val then
        error(string.format("Expected falsy value, got: %s", format_value(val)), 2)
    end
end

function assert.is_nil(val)
    if val ~= nil then
        error(string.format("Expected nil, got: %s", format_value(val)), 2)
    end
end

function assert.is_string(val)
    if type(val) ~= "string" then
        error(string.format("Expected string, got: %s", type(val)), 2)
    end
end

function assert.is_number(val)
    if type(val) ~= "number" then
        error(string.format("Expected number, got: %s", type(val)), 2)
    end
end

function assert.is_table(val)
    if type(val) ~= "table" then
        error(string.format("Expected table, got: %s", type(val)), 2)
    end
end

function assert.is_function(val)
    if type(val) ~= "function" then
        error(string.format("Expected function, got: %s", type(val)), 2)
    end
end

function assert.has_error(fn, expected_msg)
    local ok, err = pcall(fn)
    if ok then
        error("Expected function to raise an error, but it succeeded", 2)
    end
    if expected_msg and not string.find(tostring(err), expected_msg, 1, true) then
        error(string.format(
            "Expected error message to contain: %s\nActual: %s",
            expected_msg,
            tostring(err)
        ), 2)
    end
end

function describe(name, fn)
    local suite = {
        name = name,
        parent = test_state.current_suite,
        tests = {},
        before_each_fns = {},
        after_each_fns = {},
    }
    local prev_suite = test_state.current_suite
    test_state.current_suite = suite
    local ok, err = pcall(fn)
    test_state.current_suite = prev_suite
    if not ok then
        error(string.format("Error in describe block '%s': %s", name, err), 2)
    end
    for _, test in ipairs(suite.tests) do
        table.insert(test_state.tests, test)
    end
end

function it(name, fn)
    if not test_state.current_suite then
        error("it() must be called inside describe()", 2)
    end
    local test = {
        name = name,
        fn = fn,
        suite = test_state.current_suite,
        status = "pending",
        error = nil,
        traceback = nil,
    }
    table.insert(test_state.current_suite.tests, test)
end

function pending(name, fn)
    if not test_state.current_suite then
        error("pending() must be called inside describe()", 2)
    end
    local test = {
        name = name,
        fn = fn,
        suite = test_state.current_suite,
        status = "pending",
        error = nil,
        traceback = nil,
        is_pending = true,
    }
    table.insert(test_state.current_suite.tests, test)
end

function before_each(fn)
    if not test_state.current_suite then
        error("before_each() must be called inside describe()", 2)
    end
    table.insert(test_state.current_suite.before_each_fns, fn)
end

function after_each(fn)
    if not test_state.current_suite then
        error("after_each() must be called inside describe()", 2)
    end
    table.insert(test_state.current_suite.after_each_fns, fn)
end

local function get_line_number(traceback)
    local line = string.match(traceback, ":(%d+):")
    return line or "?"
end

local function run_test(test)
    local before_fns = {}
    local after_fns = {}
    local suite = test.suite
    while suite do
        table.insert(before_fns, 1, suite.before_each_fns)
        table.insert(after_fns, 1, suite.after_each_fns)
        suite = suite.parent
    end
    for _, fns in ipairs(before_fns) do
        for _, fn in ipairs(fns) do
            local ok, err = pcall(fn)
            if not ok then
                test.status = "failed"
                test.error = err
                test.traceback = debug.traceback()
                return
            end
        end
    end
    local ok, err = pcall(test.fn)
    if ok then
        test.status = "passed"
    else
        test.status = "failed"
        test.error = err
        test.traceback = debug.traceback()
    end
    for i = #after_fns, 1, -1 do
        local fns = after_fns[i]
        for _, fn in ipairs(fns) do
            local ok, err = pcall(fn)
            if not ok and test.status == "passed" then
                test.status = "failed"
                test.error = err
                test.traceback = debug.traceback()
            end
        end
    end
end

function run_tests()
    test_state.results = {
        passed = 0,
        failed = 0,
        pending = 0,
        errors = {},
    }
    for _, test in ipairs(test_state.tests) do
        if test.is_pending then
            test_state.results.pending = test_state.results.pending + 1
            print(string.format("%s⊘ %s%s", COLORS.yellow, test.name, COLORS.reset))
        else
            run_test(test)
            if test.status == "passed" then
                test_state.results.passed = test_state.results.passed + 1
                print(string.format("%s✓ %s%s", COLORS.green, test.name, COLORS.reset))
            else
                test_state.results.failed = test_state.results.failed + 1
                local line = get_line_number(test.traceback or "")
                print(string.format(
                    "%s✗ %s%s\n  %s (line %s)",
                    COLORS.red,
                    test.name,
                    COLORS.reset,
                    test.error or "Unknown error",
                    line
                ))
                table.insert(test_state.results.errors, {
                    name = test.name,
                    error = test.error,
                    traceback = test.traceback,
                })
            end
        end
    end
    local total = test_state.results.passed + test_state.results.failed + test_state.results.pending
    print(string.format(
        "\n%s%d passed%s, %s%d failed%s, %s%d pending%s (total: %d)",
        COLORS.green,
        test_state.results.passed,
        COLORS.reset,
        COLORS.red,
        test_state.results.failed,
        COLORS.reset,
        COLORS.yellow,
        test_state.results.pending,
        COLORS.reset,
        total
    ))
    return test_state.results
end

_G.describe = describe
_G.it = it
_G.pending = pending
_G.before_each = before_each
_G.after_each = after_each
_G.assert = assert
_G.run_tests = run_tests
"#;

const LUA_QOL: &str = r#"
-- ============================================================================
-- cru.inspect — Pretty-print any Lua value with cycle detection
-- ============================================================================

do
    local function inspect_impl(value, opts, seen, depth)
        opts = opts or {}
        local max_depth = opts.max_depth
        local indent_str = opts.indent or "  "
        seen = seen or {}
        depth = depth or 0

        local t = type(value)

        -- Handle nil, boolean, number
        if t == "nil" then
            return "nil"
        elseif t == "boolean" then
            return tostring(value)
        elseif t == "number" then
            return tostring(value)
        elseif t == "string" then
            return string.format("%q", value)
        elseif t == "function" then
            return "<function>"
        elseif t == "userdata" then
            return "<userdata>"
        elseif t == "thread" then
            return "<thread>"
        elseif t == "table" then
            -- Check for cycles
            if seen[value] then
                return "<cycle: table>"
            end

            -- Check depth limit
            if max_depth and depth >= max_depth then
                return "{...}"
            end

            seen[value] = true
            local indent = indent_str:rep(depth)
            local next_indent = indent_str:rep(depth + 1)
            local parts = {}

            for k, v in pairs(value) do
                local key_str
                if type(k) == "string" then
                    key_str = k
                else
                    key_str = "[" .. inspect_impl(k, opts, seen, depth + 1) .. "]"
                end
                local val_str = inspect_impl(v, opts, seen, depth + 1)
                table.insert(parts, next_indent .. key_str .. " = " .. val_str)
            end

            if #parts == 0 then
                return "{}"
            else
                return "{\n" .. table.concat(parts, ",\n") .. "\n" .. indent .. "}"
            end
        else
            return tostring(value)
        end
    end

    function cru.inspect(value, opts)
        return inspect_impl(value, opts)
    end
end

-- ============================================================================
-- cru.tbl_deep_extend — Deep merge tables with behavior semantics
-- ============================================================================

do
    local function deep_extend_impl(behavior, result, ...)
        local tables = {...}
        for _, tbl in ipairs(tables) do
            if type(tbl) == "table" then
                for k, v in pairs(tbl) do
                    if behavior == "force" then
                        -- Last wins: always override
                        if type(v) == "table" and type(result[k]) == "table" then
                            -- Recurse for nested tables
                            deep_extend_impl(behavior, result[k], v)
                        else
                            result[k] = v
                        end
                    elseif behavior == "keep" then
                        -- First wins: skip if already set
                        if result[k] == nil then
                            if type(v) == "table" then
                                -- Deep copy the table
                                result[k] = {}
                                deep_extend_impl(behavior, result[k], v)
                            else
                                result[k] = v
                            end
                        elseif type(v) == "table" and type(result[k]) == "table" then
                            -- Recurse even if key exists
                            deep_extend_impl(behavior, result[k], v)
                        end
                    end
                end
            end
        end
        return result
    end

    function cru.tbl_deep_extend(behavior, ...)
        cru.check.one_of(behavior, {"force", "keep"}, "behavior")
        local result = {}
        return deep_extend_impl(behavior, result, ...)
    end
end

-- ============================================================================
-- cru.tbl_get — Safe nested key access
-- ============================================================================

function cru.tbl_get(t, ...)
    if type(t) ~= "table" then return nil end
    local keys = {...}
    local current = t
    for _, key in ipairs(keys) do
        if type(current) ~= "table" then
            return nil
        end
        current = current[key]
        if current == nil then
            return nil
        end
    end
    return current
end

-- ============================================================================
-- cru.on_error — Overridable error handler hook
-- ============================================================================

cru.on_error = nil
"#;

const LUA_STDLIB: &str = r#"
-- ============================================================================
-- cru.retry — Exponential backoff with jitter
-- ============================================================================

function cru.retry(fn, opts)
    opts = opts or {}
    local max = opts.max_retries or 3
    local base = opts.base_delay or 1.0
    local cap = opts.max_delay or 60.0
    local use_jitter = opts.jitter ~= false
    local is_retryable = opts.retryable or function() return true end

    for attempt = 0, max do
        local ok, result = pcall(fn)
        if ok then return result end
        if attempt == max then error(result) end
        if not is_retryable(result) then error(result) end

        local delay = math.min(base * (2 ^ attempt), cap)
        if use_jitter then
            delay = delay * (0.5 + math.random() * 0.5)
        end

        -- Honor server-specified retry-after
        if type(result) == "table" and result.after then
            delay = math.max(delay, tonumber(result.after) or delay)
        end

        cru.timer.sleep(delay)
    end
end

-- ============================================================================
-- cru.emitter — Minimal event emitter
-- ============================================================================

do
    local Emitter = {}
    Emitter.__index = Emitter

    local function get_fn(entry)
        if type(entry) == 'table' then
            return entry.fn
        end
        return entry
    end

    function Emitter.new()
        return setmetatable({ _listeners = {} }, Emitter)
    end

    function Emitter:on(event, fn, owner)
        if not self._listeners[event] then
            self._listeners[event] = {}
        end
        local list = self._listeners[event]
        local id = #list + 1
        if owner ~= nil then
            list[id] = { fn = fn, owner = owner }
        else
            list[id] = fn
        end
        return id
    end

    function Emitter:once(event, fn, owner)
        local id
        id = self:on(event, function(...)
            self:off(event, id)
            fn(...)
        end, owner)
        return id
    end

    function Emitter:off(event, id)
        if self._listeners[event] then
            self._listeners[event][id] = false
        end
    end

    function Emitter:emit(event, ...)
        local listeners = self._listeners[event]
        if not listeners then return end
        for i = 1, #listeners do
            local entry = listeners[i]
            if entry then
                local fn = get_fn(entry)
                if fn then
                    local ok, err = pcall(fn, ...)
                    if not ok then
                        cru.log("warn", "emitter handler error on '" .. event .. "': " .. tostring(err))
                        if cru.errors and cru.errors._capture then
                            local owner = (type(entry) == 'table' and entry.owner) or "unknown"
                            cru.errors._capture(owner, tostring(err), "emitter:emit('" .. tostring(event) .. "')")
                        end
                    end
                end
            end
        end
    end

    -- Fire-and-forget emit: returns immediately, errors are silently swallowed.
    -- Semantically "async" — callers must not depend on handler completion or
    -- error propagation.  In pure Lua this still executes synchronously.
    function Emitter:emit_async(event, ...)
        local listeners = self._listeners[event]
        if not listeners then return end
        for i = 1, #listeners do
            local entry = listeners[i]
            if entry then
                local fn = get_fn(entry)
                if fn then
                    pcall(fn, ...)
                end
            end
        end
    end

    -- Count active listeners for an event (excludes removed ones)
    function Emitter:count(event)
        local listeners = self._listeners[event]
        if not listeners then return 0 end
        local n = 0
        for i = 1, #listeners do
            if listeners[i] then n = n + 1 end
        end
        return n
    end

    function Emitter:unregister_owner(owner)
        for _, listeners in pairs(self._listeners) do
            for i = 1, #listeners do
                local entry = listeners[i]
                if type(entry) == 'table' and entry.owner == owner then
                    listeners[i] = false
                end
            end
        end
    end

    function Emitter:off_all(event)
        if event then
            self._listeners[event] = nil
        else
            self._listeners = {}
        end
    end

    -- Global shared emitter singleton (stored in closure scope)
    local _global_emitter = nil
    local function get_global()
        if not _global_emitter then
            _global_emitter = Emitter.new()
        end
        return _global_emitter
    end

    cru.emitter = { new = Emitter.new, global = get_global }
end

-- ============================================================================
-- cru.check — Argument validation
-- ============================================================================

do
    local check = {}

    local function fail(name, expected, got)
        error(string.format("%s: expected %s, got %s", name, expected, type(got)), 3)
    end

    function check.string(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "string" then fail(name, "string", val) end
    end

    function check.number(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "number" then fail(name, "number", val) end
        if opts then
            if opts.min and val < opts.min then
                error(string.format("%s: must be >= %s, got %s", name, opts.min, val), 2)
            end
            if opts.max and val > opts.max then
                error(string.format("%s: must be <= %s, got %s", name, opts.max, val), 2)
            end
        end
    end

    function check.boolean(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "boolean" then fail(name, "boolean", val) end
    end

    function check.table(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "table" then fail(name, "table", val) end
    end

    function check.one_of(val, choices, name, opts)
        if opts and opts.optional and val == nil then return end
        for _, v in ipairs(choices) do
            if val == v then return end
        end
        error(string.format("%s: must be one of [%s], got %s",
            name, table.concat(choices, ", "), tostring(val)), 2)
    end

    function check.func(val, name, opts)
        if opts and opts.optional and val == nil then return end
        if type(val) ~= "function" then fail(name, "function", val) end
    end

    cru.check = check
end

-- ============================================================================
-- cru.service — Supervised service lifecycle
-- ============================================================================

do
    local Service = {}
    Service._services = {}

    function Service.define(spec)
        cru.check.table(spec, "spec")
        cru.check.string(spec.name, "spec.name")
        cru.check.string(spec.desc, "spec.desc")
        cru.check.func(spec.start, "spec.start")
        cru.check.func(spec.stop, "spec.stop", { optional = true })
        cru.check.func(spec.health, "spec.health", { optional = true })

        local name = spec.name
        local restart = spec.restart or {}
        local max_retries  = restart.max_retries or 10
        local base_delay   = restart.base_delay  or 1.0
        local max_delay    = restart.max_delay    or 60.0

        -- Resolve config schema defaults and secrets
        local resolved_config = nil
        if spec.config then
            resolved_config = {}
            local plugin_upper = name:upper():gsub("[^A-Z0-9]", "_")
            for key, schema in pairs(spec.config) do
                local val = nil
                -- Secret resolution: env var first
                if schema.secret then
                    local env_key = "CRUCIBLE_" .. plugin_upper .. "_" .. key:upper():gsub("[^A-Z0-9]", "_")
                    val = os.getenv(env_key)
                end
                -- Fall back to plugin config
                if val == nil then
                    local ok, cfg_val = pcall(function()
                        return crucible.config.get(name .. "." .. key)
                    end)
                    if ok and cfg_val ~= nil then val = cfg_val end
                end
                -- Fall back to schema default
                if val == nil and schema.default ~= nil then
                    val = schema.default
                end
                resolved_config[key] = val
            end
        end

        local entry = {
            name      = name,
            desc      = spec.desc,
            running   = false,
            healthy   = nil,
            start_fn  = spec.start,
            stop_fn   = spec.stop,
            health_fn = spec.health,
            config    = resolved_config,
        }
        Service._services[name] = entry

        -- Build the wrapper function the daemon spawns
        local function wrapped()
            entry.running = true
            cru.log("info", "service '" .. name .. "' starting")

            local ok, err = pcall(function()
                cru.retry(function()
                    entry.running = true
                    spec.start()
                end, {
                    max_retries = max_retries,
                    base_delay  = base_delay,
                    max_delay   = max_delay,
                    retryable   = function(e)
                        return type(e) ~= "table" or e.retryable ~= false
                    end,
                })
            end)

            entry.running = false
            if not ok then
                cru.log("warn", "service '" .. name .. "' stopped: " .. tostring(err))
            else
                cru.log("info", "service '" .. name .. "' completed")
            end
        end

        return { desc = spec.desc, fn = wrapped }
    end

    function Service.status(name)
        local entry = Service._services[name]
        if not entry then return nil end
        local healthy = nil
        if entry.health_fn then
            local ok, h = pcall(entry.health_fn)
            healthy = ok and h or false
        end
        return { running = entry.running, healthy = healthy, name = entry.name, desc = entry.desc }
    end

    function Service.list()
        local out = {}
        for _, entry in pairs(Service._services) do
            local healthy = nil
            if entry.health_fn then
                local ok, h = pcall(entry.health_fn)
                healthy = ok and h or false
            end
            out[#out + 1] = { name = entry.name, desc = entry.desc, running = entry.running, healthy = healthy }
        end
        return out
    end

    function Service.stop(name)
        local entry = Service._services[name]
        if not entry then return false end
        if entry.stop_fn then
            local ok, err = pcall(entry.stop_fn)
            if not ok then
                cru.log("warn", "service '" .. name .. "' stop error: " .. tostring(err))
            end
        end
        entry.running = false
        return true
    end

    cru.service = Service
end
"#;

const LUA_HEALTH: &str = r#"
-- health.lua - Plugin self-diagnostics (Neovim vim.health-inspired)
--
-- Provides cru.health module for plugins to report health status.
-- Plugins write a health.lua returning {check = function() ... end}
--
-- Usage:
--   cru.health.start("my-plugin")
--   cru.health.ok("Database connected")
--   cru.health.warn("Cache miss rate high", {"Consider increasing cache size"})
--   cru.health.error("API key missing", {"Set CRUCIBLE_API_KEY env var"})
--   local results = cru.health.get_results()
--   -- results = {
--   --   name = "my-plugin",
--   --   healthy = false,  -- error makes this false
--   --   checks = [
--   --     {level="ok", msg="Database connected"},
--   --     {level="warn", msg="Cache miss rate high", advice=["..."]},
--   --     {level="error", msg="API key missing", advice=["..."]},
--   --   ]
--   -- }

local health = {}

-- Internal state: current check results
local _state = {
    name = nil,
    checks = {},
    healthy = true,
}

-- Start a new health check section (resets state)
function health.start(name)
    cru.check.string(name, "name")
    _state = {
        name = name,
        checks = {},
        healthy = true,
    }
end

-- Add an OK check (does not affect healthy status)
function health.ok(msg)
    cru.check.string(msg, "msg")
    table.insert(_state.checks, {
        level = "ok",
        msg = msg,
    })
end

-- Add a warning check (does not affect healthy status)
function health.warn(msg, advice)
    cru.check.string(msg, "msg")
    if advice ~= nil then
        cru.check.table(advice, "advice")
    end
    table.insert(_state.checks, {
        level = "warn",
        msg = msg,
        advice = advice,
    })
end

-- Add an error check (sets healthy = false)
function health.error(msg, advice)
    cru.check.string(msg, "msg")
    if advice ~= nil then
        cru.check.table(advice, "advice")
    end
    _state.healthy = false
    table.insert(_state.checks, {
        level = "error",
        msg = msg,
        advice = advice,
    })
end

-- Add an info check (does not affect healthy status)
function health.info(msg)
    cru.check.string(msg, "msg")
    table.insert(_state.checks, {
        level = "info",
        msg = msg,
    })
end

-- Get current results and reset state
function health.get_results()
    local results = {
        name = _state.name,
        healthy = _state.healthy,
        checks = _state.checks,
    }
    -- Reset state for next check
    _state = {
        name = nil,
        checks = {},
        healthy = true,
    }
    return results
end

-- Register as global cru.health
cru.health = health
"#;

/// Register the pure Lua standard library (retry, emitter, check, test_runner, health).
///
/// Must be called after `setup_globals` creates the `cru` table and after
/// `register_timer_module` (since `cru.retry` depends on `cru.timer.sleep`).
pub fn register_lua_stdlib(lua: &Lua) -> Result<()> {
    lua.load(LUA_TEST_RUNNER).set_name("test_runner").exec()?;
    lua.load(LUA_TEST_MOCKS).set_name("test_mocks").exec()?;
    lua.load(LUA_STDLIB).exec()?;

    let cru = lua.globals().get::<mlua::Table>("cru")?;
    let errors_table = lua.create_table()?;

    let capture_fn = lua.create_function(
        |lua, (plugin, error, context): (String, String, String)| -> Result<()> {
            let error_log = lua
                .app_data_ref::<Arc<Mutex<PluginErrorLog>>>()
                .map(|shared| Arc::clone(&*shared));

            if let Some(shared) = error_log {
                if let Ok(mut guard) = shared.lock() {
                    guard.push(PluginErrorEntry {
                        plugin,
                        error,
                        context,
                        timestamp: std::time::Instant::now(),
                    });
                }
            }

            Ok(())
        },
    )?;
    errors_table.set("_capture", capture_fn)?;

    let recent_fn = lua.create_function(|lua, n: Option<usize>| {
        let limit = n.unwrap_or(10);
        let result = lua.create_table()?;
        let error_log = lua
            .app_data_ref::<Arc<Mutex<PluginErrorLog>>>()
            .map(|shared| Arc::clone(&*shared));

        if let Some(shared) = error_log {
            if let Ok(guard) = shared.lock() {
                let entries = guard.recent(limit);
                for (idx, entry) in entries.into_iter().enumerate() {
                    let row = lua.create_table()?;
                    row.set("plugin", entry.plugin.as_str())?;
                    row.set("error", entry.error.as_str())?;
                    row.set("context", entry.context.as_str())?;
                    row.set("age_secs", entry.timestamp.elapsed().as_secs_f64())?;
                    result.set(idx + 1, row)?;
                }
            }
        }

        Ok(result)
    })?;
    errors_table.set("recent", recent_fn)?;
    cru.set("errors", errors_table)?;

    lua.load(LUA_QOL).set_name("qol").exec()?;
    lua.load(LUA_HEALTH).set_name("health").exec()?;
    lua.load("_G.inspect = cru.inspect").exec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Table;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        // Create cru namespace
        lua.load("cru = cru or {}").exec().unwrap();
        // Need a mock cru.log for emitter error handling
        lua.load(
            r#"
            cru.log = function(level, msg) end
        "#,
        )
        .exec()
        .unwrap();
        // Need cru.timer.sleep for retry (mock it for fast tests)
        lua.load(
            r#"
            cru.timer = { sleep = function(secs) end }
        "#,
        )
        .exec()
        .unwrap();
        register_lua_stdlib(&lua).unwrap();
        lua
    }

    #[test]
    fn test_emitter_on_emit() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function(v) got = v end)
                em:emit("test", 42)
                return got
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_emitter_once() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local count = 0
                em:once("test", function() count = count + 1 end)
                em:emit("test")
                em:emit("test")
                return count
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_emitter_off() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local count = 0
                local id = em:on("test", function() count = count + 1 end)
                em:emit("test")
                em:off("test", id)
                em:emit("test")
                return count
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_emitter_error_handling() {
        let lua = setup_lua();
        // Handler errors should not propagate
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function() error("boom") end)
                em:on("test", function() got = 1 end)
                em:emit("test")
                return got
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_retry_succeeds() {
        let lua = setup_lua();
        let result: (String, i32) = lua
            .load(
                r#"
                local attempts = 0
                local result = cru.retry(function()
                    attempts = attempts + 1
                    if attempts < 3 then error({ retryable = true }) end
                    return "ok"
                end, { max_retries = 5, base_delay = 0.001 })
                return result, attempts
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, "ok");
        assert_eq!(result.1, 3);
    }

    #[test]
    fn test_retry_exhausted() {
        let lua = setup_lua();
        let result = lua
            .load(
                r#"
                cru.retry(function()
                    error("always fails")
                end, { max_retries = 2, base_delay = 0.001 })
                "#,
            )
            .exec();
        assert!(result.is_err());
    }

    #[test]
    fn test_retry_non_retryable() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local attempts = 0
                pcall(cru.retry, function()
                    attempts = attempts + 1
                    error("fatal")
                end, {
                    max_retries = 5,
                    base_delay = 0.001,
                    retryable = function() return false end,
                })
                return attempts
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_check_string() {
        let lua = setup_lua();
        // Valid
        lua.load(r#"cru.check.string("hello", "name")"#)
            .exec()
            .unwrap();
        // Invalid
        assert!(lua.load(r#"cru.check.string(42, "name")"#).exec().is_err());
        // Optional nil
        lua.load(r#"cru.check.string(nil, "name", { optional = true })"#)
            .exec()
            .unwrap();
        // Optional non-nil wrong type
        assert!(lua
            .load(r#"cru.check.string(42, "name", { optional = true })"#)
            .exec()
            .is_err());
    }

    #[test]
    fn test_check_number_with_range() {
        let lua = setup_lua();
        lua.load(r#"cru.check.number(5, "count", { min = 1, max = 10 })"#)
            .exec()
            .unwrap();
        assert!(lua
            .load(r#"cru.check.number(0, "count", { min = 1 })"#)
            .exec()
            .is_err());
        assert!(lua
            .load(r#"cru.check.number(11, "count", { max = 10 })"#)
            .exec()
            .is_err());
    }

    #[test]
    fn test_check_one_of() {
        let lua = setup_lua();
        lua.load(r#"cru.check.one_of("json", {"json", "text"}, "format")"#)
            .exec()
            .unwrap();
        assert!(lua
            .load(r#"cru.check.one_of("xml", {"json", "text"}, "format")"#)
            .exec()
            .is_err());
    }

    #[test]
    fn test_check_table() {
        let lua = setup_lua();
        lua.load(r#"cru.check.table({}, "opts")"#).exec().unwrap();
        assert!(lua
            .load(r#"cru.check.table("string", "opts")"#)
            .exec()
            .is_err());
    }

    #[test]
    fn test_check_modules_exist() {
        let lua = setup_lua();
        let cru: Table = lua.globals().get("cru").unwrap();

        assert!(cru.get::<Table>("emitter").is_ok());
        let emitter: Table = cru.get("emitter").unwrap();
        assert!(emitter.get::<mlua::Function>("new").is_ok());
        assert!(emitter.get::<mlua::Function>("global").is_ok());
        assert!(cru.get::<Table>("check").is_ok());
        assert!(cru.get::<mlua::Function>("retry").is_ok());
        assert!(cru.get::<Table>("health").is_ok());
    }

    #[test]
    fn test_emitter_preserves_registration_order() {
        let lua = setup_lua();
        let result: String = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local order = ""
                em:on("test", function() order = order .. "a" end)
                em:on("test", function() order = order .. "b" end)
                em:on("test", function() order = order .. "c" end)
                em:emit("test")
                return order
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, "abc");
    }

    #[test]
    fn test_emitter_count() {
        let lua = setup_lua();
        let result: (i32, i32, i32) = lua
            .load(
                r#"
                local em = cru.emitter.new()
                -- no listeners yet
                local c0 = em:count("test")
                em:on("test", function() end)
                em:on("test", function() end)
                local c2 = em:count("test")
                -- unknown event
                local c_none = em:count("unknown")
                return c0, c2, c_none
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, 0);
        assert_eq!(result.1, 2);
        assert_eq!(result.2, 0);
    }

    #[test]
    fn test_emitter_count_excludes_removed() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local id1 = em:on("test", function() end)
                em:on("test", function() end)
                em:off("test", id1)
                return em:count("test")
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_emitter_emit_async_fires_listeners() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function(v) got = v end)
                em:emit_async("test", 99)
                return got
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 99);
    }

    #[test]
    fn test_emitter_emit_async_swallows_errors() {
        let lua = setup_lua();
        // emit_async should not propagate handler errors
        let result: i32 = lua
            .load(
                r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function() error("boom") end)
                em:on("test", function() got = 1 end)
                em:emit_async("test")
                return got
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn test_emitter_global_returns_same_instance() {
        let lua = setup_lua();
        let result: bool = lua
            .load(
                r#"
                local g1 = cru.emitter.global()
                local g2 = cru.emitter.global()
                return g1 == g2
                "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_emitter_global_is_functional() {
        let lua = setup_lua();
        let result: i32 = lua
            .load(
                r#"
                local g = cru.emitter.global()
                local got = 0
                g:on("evt", function(v) got = v end)
                -- Access via a second global() call to prove shared state
                cru.emitter.global():emit("evt", 77)
                return got
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result, 77);
    }

    #[test]
    fn test_emitter_global_independent_from_new() {
        let lua = setup_lua();
        let result: (i32, i32) = lua
            .load(
                r#"
                local g = cru.emitter.global()
                local e = cru.emitter.new()
                local g_got = 0
                local e_got = 0
                g:on("test", function() g_got = g_got + 1 end)
                e:on("test", function() e_got = e_got + 1 end)
                g:emit("test")
                return g_got, e_got
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, 1);
        assert_eq!(result.1, 0);
    }

    #[test]
    fn test_emitter_count_after_once_fires() {
        let lua = setup_lua();
        let result: (i32, i32) = lua
            .load(
                r#"
                local em = cru.emitter.new()
                em:once("test", function() end)
                local before = em:count("test")
                em:emit("test")
                local after = em:count("test")
                return before, after
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, 1);
        assert_eq!(result.1, 0);
    }

    #[tokio::test]
    async fn test_retry_with_real_timer() {
        let lua = Lua::new();
        lua.load("cru = cru or {}").exec().unwrap();
        lua.load(r#"cru.log = function() end"#).exec().unwrap();
        crate::timer::register_timer_module(&lua).unwrap();
        register_lua_stdlib(&lua).unwrap();

        let start = std::time::Instant::now();
        let result: (String, i32) = lua
            .load(
                r#"
                local attempts = 0
                local result = cru.retry(function()
                    attempts = attempts + 1
                    if attempts < 3 then error({ retryable = true }) end
                    return "ok"
                end, { max_retries = 5, base_delay = 0.01 })
                return result, attempts
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.0, "ok");
        assert_eq!(result.1, 3);
        // Verify real async sleep was used (at least some time passed)
        assert!(start.elapsed().as_millis() >= 10);
    }

    #[test]
    fn test_service_define_returns_descriptor() {
        let lua = setup_lua();
        let result: (String, bool) = lua
            .load(
                r#"
                local started = false
                local svc = cru.service.define({
                    name = "test",
                    desc = "Test service",
                    start = function() started = true end,
                })
                return svc.desc, type(svc.fn) == "function"
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(result.0, "Test service");
        assert!(result.1);
    }

    #[test]
    fn test_service_define_validates_required_fields() {
        let lua = setup_lua();
        // Missing start fn
        assert!(lua
            .load(r#"cru.service.define({ name = "x", desc = "x" })"#)
            .exec()
            .is_err());
        // Missing name
        assert!(lua
            .load(r#"cru.service.define({ desc = "x", start = function() end })"#)
            .exec()
            .is_err());
    }

    #[test]
    fn test_service_list_and_status() {
        let lua = setup_lua();
        let (count, name, running): (i32, String, bool) = lua
            .load(
                r#"
                cru.service.define({
                    name = "svc1",
                    desc = "Service One",
                    start = function() end,
                    health = function() return true end,
                })
                local list = cru.service.list()
                local st = cru.service.status("svc1")
                return #list, st.name, st.running
                "#,
            )
            .eval()
            .unwrap();
        assert!(count >= 1);
        assert_eq!(name, "svc1");
        assert!(!running); // Not started yet, just defined
    }

    #[test]
    fn test_service_stop() {
        let lua = setup_lua();
        let stopped: bool = lua
            .load(
                r#"
                local was_stopped = false
                cru.service.define({
                    name = "stoppable",
                    desc = "Can stop",
                    start = function() end,
                    stop = function() was_stopped = true end,
                })
                cru.service.stop("stoppable")
                return was_stopped
                "#,
            )
            .eval()
            .unwrap();
        assert!(stopped);
    }

    #[test]
    fn test_service_config_resolution() {
        let lua = setup_lua();
        // Mock crucible.config.get to return nil (no config file)
        lua.load(
            r#"
            crucible = crucible or {}
            crucible.config = { get = function() return nil end }
            "#,
        )
        .exec()
        .unwrap();

        let val: i32 = lua
            .load(
                r#"
                cru.service.define({
                    name = "cfgtest",
                    desc = "Config test",
                    start = function() end,
                    config = {
                        port = { type = "number", default = 8080 },
                    },
                })
                local entry = cru.service._services["cfgtest"]
                return entry.config.port
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(val, 8080);
    }
}
