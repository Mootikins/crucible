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

Test files end in `_test.lua`. Use `describe` and `it`. Load the plugin under
test by its **directory name** — the module name the daemon uses — not by
`init`; the runner's `package.path` resolves `require("my-plugin")` via
`<plugins-parent>/?/init.lua`, and `require("init")` resolves nothing:

```lua
-- my-plugin/tests/init_test.lua
describe("my-plugin", function()
  local plugin = require("my-plugin")

  it("greets by name", function()
    local result = plugin.tools.greet.fn({ name = "Alice" })
    assert.equal("Hello, Alice!", result.message)
  end)

  it("rejects missing name", function()
    local result = plugin.tools.greet.fn({})
    assert.truthy(result.error)
  end)
end)
```

## Assertions

Expected value first. The runner reports a mismatch as `Expected: <first>` /
`Actual: <second>`, so passing them the other way round makes a failure read
backwards.

```lua
assert(condition, "message")            -- basic
assert.equal(expected, actual)          -- value equality (alias: assert.equals)
assert.deep_equal(expected, actual)     -- recursive table equality
assert.truthy(value)                    -- not nil/false
assert.falsy(value)                     -- nil or false
assert.is_nil(value)                    -- nil check
assert.is_not_nil(value)                -- not nil
assert.is_string(value)                 -- type checks
assert.is_number(value)
assert.is_table(value)
assert.is_function(value)
assert.has_error(fn, substring?)        -- expect an error, optionally matching
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

`test_mocks.setup(overrides)` replaces `cru.kiln`, `cru.graph`, `cru.http`, `cru.fs`, `cru.paths`, `cru.session`, and `cru.sessions` with fixture-backed mocks (mirrored onto `crucible.*` and the `http`/`fs`/`paths` globals). Overrides are merged per module key over these defaults:

```lua
kiln     = { notes = {}, outlinks = {}, backlinks = {}, neighbors = {} },
graph    = { notes = {}, outlinks = {}, backlinks = {}, neighbors = {} },
http     = { responses = {} },
fs       = { files = {}, dirs = {} },
paths    = { kiln = "/mock/kiln", workspace = "/mock/workspace",
             session = false, state = "/mock/state" },
session  = { temperature = 0.7, max_tokens = nil, model = "mock-model",
             mode = "act", thinking_budget = nil },
sessions = { info = { kiln = "/mock/kiln" }, messages = {}, response_parts = {} },
```

`test_mocks.reset()` restores the defaults and clears recorded calls.

### graph fixture

Backs `cru.graph.get_note(path)` (looked up in `notes` by `path`), `get_outlinks` / `get_backlinks` / `get_neighbors` (looked up in the same-named maps, keyed by note path), and `search_semantic` (case-insensitive substring match over each note's `title` and `content`, returning `{ path, score = 0.9 }` rows; `opts.limit` defaults to 100). The `kiln` mock's `search` works the same way with score 1.0.

```lua
test_mocks.setup({
  graph = {
    notes = { { path = "a.md", title = "Alpha", content = "links to beta" } },
    outlinks = { ["a.md"] = { "b.md" } },
  },
})
```

### paths fixture

Mirrors the real `cru.paths` shape: each accessor **raises** when its path is unconfigured rather than returning `nil`, so a plugin that pcalls `paths.kiln()` and falls back is exercised against production behavior. Mark a path unconfigured with `false`, not `nil` — a `nil` override is indistinguishable from no override and silently leaves the default in place (which is why `session = false` is the default). `paths.state(plugin)` returns `state .. "/" .. plugin`; `paths.join` follows `PathBuf::push` semantics, so an absolute component discards what preceded it.

### sessions fixture

Backs the subagent-delegation API, which the bare test VM otherwise lacks (the real module is registered by the daemon). `create(opts)` returns `{ id = "mock-session-1" }` with an incrementing counter; `get(id)` returns `info`; `messages(id, opts)` returns `messages`; `send_and_collect(id, prompt, opts)` returns an iterator that yields each entry of `response_parts` then `nil`; `configure_agent` and `end_session` are recorded no-ops.

```lua
test_mocks.setup({
  sessions = {
    response_parts = {
      { type = "text", content = "the answer" },
    },
  },
})
```

Inspect what was called on any mock:

```lua
local calls = test_mocks.get_calls("kiln", "search")
assert.equal(1, #calls)
local id_calls = test_mocks.get_calls("sessions", "create")
```

## Testing Tool Functions

Call tool functions directly from the spec table:

```lua
local plugin = require("my-plugin")

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
