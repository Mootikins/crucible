pub(super) const LUA_TEST_RUNNER: &str = r#"
-- test_runner.lua - Minimal test runner for Crucible plugins
--
-- Provides describe/it/before_each/after_each/pending globals and assert table.
-- No external dependencies — pure Lua with pcall/error only.
--
-- Results are RETURNED from run_tests(), never printed. `print` writes to the
-- host process's stdout, and the daemon that runs plugin tests is normally
-- auto-spawned with stdout and stderr set to /dev/null
-- (rpc_client/client/mod.rs), so anything printed here is written to a null
-- device and is unrecoverable by the caller who asked for the results.

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

function assert.is_not_nil(val)
    if val == nil then
        error("Expected a non-nil value, got: nil", 2)
    end
end

-- `oci`'s suite spells it `equals`; alias rather than rewrite every call site.
assert.equals = assert.equal

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

--- Full `describe` path for a test, outermost first.
---
--- Reporting the bare `it` name is ambiguous the moment two suites share one:
--- several plugins have their own "returns nil on non-JSON".
local function describe_path(suite)
    local parts = {}
    while suite do
        table.insert(parts, 1, suite.name)
        suite = suite.parent
    end
    return table.concat(parts, " / ")
end

--- Split `chunk:line: message` into file, line, and the bare message.
---
--- Every assertion here raises at level 2, so Lua has already prefixed the
--- *caller's* location — the line in the test file. That is the location worth
--- reporting. `debug.traceback()` cannot give it: its innermost frame is always
--- inside this runner, so matching the first `:%d+:` there (as this code used
--- to) reported the runner's own line, identically for every failure.
local function split_location(err)
    err = tostring(err)
    local file, line, message = string.match(err, "^(.-):(%d+): (.*)$")
    if file then
        return file, line, message
    end
    return nil, nil, err
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
    end
    for i = #after_fns, 1, -1 do
        local fns = after_fns[i]
        for _, fn in ipairs(fns) do
            local ok, err = pcall(fn)
            if not ok and test.status == "passed" then
                test.status = "failed"
                test.error = err
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
        else
            run_test(test)
            if test.status == "passed" then
                test_state.results.passed = test_state.results.passed + 1
            else
                test_state.results.failed = test_state.results.failed + 1
                local file, line, message = split_location(test.error or "Unknown error")
                table.insert(test_state.results.errors, {
                    name = test.name,
                    suite = describe_path(test.suite),
                    error = message,
                    file = file,
                    line = line,
                })
            end
        end
    end
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
