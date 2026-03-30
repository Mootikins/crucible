---
title: Testing Plugins
description: How to test Lua plugins with the built-in test runner
tags: [help, lua, testing]
---

# Testing Plugins

Crucible has a built-in test runner. Put test files in `tests/` inside your plugin directory.

## Running Tests

```bash
cru plugin test ./my-plugin                    # run all tests
cru plugin test ./my-plugin -f "search"        # filter by name
```

## Writing Tests

Test files end in `_test.lua`. Use `describe` and `it`:

```lua
-- tests/init_test.lua
describe("my-plugin", function()
  local plugin = require("init")

  it("greets by name", function()
    local result = plugin.tools.greet.fn({ name = "Alice" })
    assert.equal(result.message, "Hello, Alice!")
  end)

  it("rejects missing name", function()
    local result = plugin.tools.greet.fn({})
    assert.truthy(result.error)
  end)
end)
```

## Assertions

```lua
assert(condition, "message")         -- basic
assert.equal(expected, actual)       -- value equality
assert.truthy(value)                 -- not nil/false
assert.falsy(value)                  -- nil or false
assert.is_nil(value)                 -- nil check
assert.is_not_nil(value)             -- not nil
assert.error(fn, pattern?)           -- expect error
```

## Test Lifecycle

```lua
describe("suite", function()
  before_each(function()
    -- runs before each test
  end)

  after_each(function()
    -- runs after each test
  end)

  it("test case", function()
    -- test body
  end)

  pending("not yet implemented", function()
    -- skipped
  end)
end)
```

## Mocks

Mock Crucible modules to test without a running daemon:

```lua
before_each(function()
  test_mocks.setup({
    kiln = {
      notes = {
        { path = "note.md", title = "Test Note", tags = {} },
      },
    },
    session = {
      temperature = 0.7,
      model = "test-model",
    },
    http = {
      responses = {
        ["https://api.example.com/data"] = {
          status = 200,
          body = '{"result": "ok"}',
        },
      },
    },
    fs = {
      files = {
        ["config.toml"] = "key = 'value'",
      },
    },
  })
end)
```

Inspect what was called:

```lua
local calls = test_mocks.get_calls("kiln", "search")
assert.equal(#calls, 1)
```

## Testing Tool Functions

Call tool functions directly from the spec table:

```lua
local plugin = require("init")

-- plugin.tools.tool_name.fn(args)
local result = plugin.tools.search_kiln.fn({
  kiln = "docs",
  query = "spacing",
})
assert.truthy(result.error)  -- no daemon, so session create fails
```

## See Also

- [[Help/Extending/Creating Plugins]] — Plugin development guide
- [[Help/Lua/Language Basics]] — Lua API reference
