pub(super) const LUA_TEST_RUNNER: &str = r#"
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
