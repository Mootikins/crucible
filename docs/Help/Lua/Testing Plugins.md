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

Test files end in `_test.lua`. Use `describe` and `it`. Before the runner
loads a test file, it activates the plugin on a plugin loader of its own, with
the body the daemon uses: the plugin's `setup()` runs against the real `cru.*`
modules, and an activation failure fails the run. Load the plugin under test by
its **directory name** — the module name the daemon uses — not by `init`; the
loader's searcher resolves `require("my-plugin")` through the directory that
holds the plugin, and `require("init")` resolves nothing. The runner seeds
`package.loaded` with the activated module for every plugin, so a `require` in
the suite answers the instance the runner activated, whose `setup()` already
ran. A suite that drives `setup()` itself and records its registrations
through a stub sets `package.loaded["my-plugin"] = nil` first, so the `require`
runs the file again:

```lua
-- my-plugin/tests/init_test.lua
describe("my-plugin", function()
  -- The runner seeded the activated instance; this suite wants a fresh one.
  package.loaded["my-plugin"] = nil
  local plugin = require("my-plugin")

  it("greets by name", function()
    local result = plugin.tools.greet.fn({ name = "Alice" })
    expect.equal("Hello, Alice!", result.message)
  end)

  it("rejects missing name", function()
    local result = plugin.tools.greet.fn({})
    expect.truthy(result.error)
  end)
end)
```

## Assertions

Expected value first. The runner reports a mismatch as `Expected: <first>` /
`Actual: <second>`, so passing them the other way round makes a failure read
backwards.

```lua
assert(condition, "message")            -- basic
expect.equal(expected, actual)          -- value equality (alias: expect.equals)
expect.deep_equal(expected, actual)     -- recursive table equality
expect.truthy(value)                    -- not nil/false
expect.falsy(value)                     -- nil or false
expect.is_nil(value)                    -- nil check
expect.is_not_nil(value)                -- not nil
expect.is_string(value)                 -- type checks
expect.is_number(value)
expect.is_table(value)
expect.is_function(value)
expect.has_error(fn, substring?)        -- expect an error, optionally matching
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
        ["fixture.toml"] = "key = 'value'",
      },
    },
  })
end)
```

`test_mocks.setup(overrides)` replaces `cru.kiln`, `cru.http`, `cru.fs`, `cru.paths`, and `cru.session` (whose deprecated plural alias `cru.sessions` points at the same mock) with fixture-backed mocks. There are no bare-global mirrors: `cru` is the one namespace, in tests as in production. Overrides are merged per module key over these defaults:

```lua
kiln     = { notes = {}, outlinks = {}, backlinks = {}, neighbors = {},
             roots = {} },
http     = { responses = {} },
fs       = { files = {}, dirs = {}, real_dirs = false },
paths    = { workspace = "/mock/workspace",
             session = false, state = "/mock/state" },
session  = { temperature = 0.7, max_tokens = nil, model = "mock-model",
             mode = "act" },
sessions = { info = { kiln = "/mock/kiln" }, messages = {}, response_parts = {} },
```

`test_mocks.reset()` restores the defaults and clears recorded calls.

### kiln roots

`cru.kiln.path(name, relative)` answers from `kiln.roots`, which maps a kiln
NAME to a directory. The mock raises for an unknown name, and for a relative
part that holds `..`, `.` or a leading `/` — the same two refusals the daemon
makes. A plugin that passes here therefore cannot fail in production.

```lua
test_mocks.setup({ kiln = { active = "notes", roots = { notes = "/kilns/notes" } } })
```

`kiln.active` mirrors the daemon's `cru.kiln.active` string field — the NAME of
the open kiln, absent by default the way a daemon with no open kiln leaves it
nil.

### real directories

`fs.real_dirs` makes the `cru.fs.mkdir` mock create the directory for real, as
well as recording the call. Turn it on when the code under test writes with
`io.open`, which needs a directory that exists. It is off by default, so no
suite touches the disk by accident.

Note that a test file cannot capture the host's `cru.fs` for itself: the runner
calls `test_mocks.setup()` before it loads any test file, so
`local real = cru.fs.mkdir` at the top of a suite captures the mock.

### paths fixture

Mirrors the real `cru.paths` shape: each accessor **raises** when its path is unconfigured rather than returning `nil`, so a plugin that pcalls an accessor and falls back is exercised against production behavior. There is no `paths.kiln`: kiln roots resolve by NAME through `cru.kiln.path`, and the mock mirrors the daemon’s `cru.kiln.active` string via the `kiln.active` fixture. Mark a path unconfigured with `false`, not `nil` — a `nil` override is indistinguishable from no override and silently leaves the default in place (which is why `session = false` is the default). `paths.state(plugin)` returns `state .. "/" .. plugin`. There is no `paths.join`: joining is plain string concatenation, in tests as in production.

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
expect.equal(1, #calls)
local id_calls = test_mocks.get_calls("sessions", "create")
```

## Testing Tool Functions

Call tool functions directly from the module table:

```lua
local plugin = require("my-plugin")

-- plugin.tools.tool_name.fn(args)
local result = plugin.tools.search_kiln.fn({
  kiln = "docs",
  query = "spacing",
})
expect.truthy(result.error)  -- no daemon, so session create fails
```

## See Also

- [[Help/Extending/Creating Plugins]] — Plugin development guide
- [[Help/Lua/Language Basics]] — Lua API reference
